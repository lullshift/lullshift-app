//! Careful file access for the log scan: plain files and folders only (no symlinks, junctions or
//! other reparse points, no network paths), bounded windows, a file budget and a deadline.

use std::fs::{self, File, Metadata};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use time::OffsetDateTime;

/// Files and folders the scan may look at, until when, and whether the user cancelled.
pub struct Budget {
    until: Instant,
    entries_left: usize,
    cancel: Option<Arc<AtomicBool>>,
    /// Set once a cap, the deadline or a cancel stopped the scan early.
    pub incomplete: bool,
}

impl Budget {
    pub fn new(deadline: Duration, max_entries: usize) -> Self {
        let now = Instant::now();
        Self {
            until: now.checked_add(deadline).unwrap_or(now),
            entries_left: max_entries,
            cancel: None,
            incomplete: false,
        }
    }

    /// Stops the scan as soon as `cancel` is set.
    pub fn with_cancel(mut self, cancel: Arc<AtomicBool>) -> Self {
        self.cancel = Some(cancel);
        self
    }

    /// The same deadline and cancel flag with its own entry allowance (one agent's share).
    pub fn share(&self, max_entries: usize) -> Self {
        Self {
            until: self.until,
            entries_left: max_entries,
            cancel: self.cancel.clone(),
            incomplete: false,
        }
    }

    pub fn until(&self) -> Instant {
        self.until
    }

    pub fn cancel_flag(&self) -> Option<Arc<AtomicBool>> {
        self.cancel.clone()
    }

    pub fn cancelled(&self) -> bool {
        self.cancel
            .as_ref()
            .is_some_and(|c| c.load(Ordering::Relaxed))
    }

    /// False (and marks the scan incomplete) once time is up or the user cancelled.
    pub fn time_left(&mut self) -> bool {
        if Instant::now() >= self.until || self.cancelled() {
            self.incomplete = true;
            return false;
        }
        true
    }

    /// Takes one directory entry from the budget; false once it is spent or time is up.
    pub fn take_entry(&mut self) -> bool {
        if self.entries_left == 0 {
            self.incomplete = true;
            return false;
        }
        if !self.time_left() {
            return false;
        }
        self.entries_left -= 1;
        true
    }
}

/// A file time as a date, or `None` when it is outside the range a date can hold (a file can
/// carry any time; `OffsetDateTime::from(SystemTime)` would panic on those).
pub fn file_time(t: SystemTime) -> Option<OffsetDateTime> {
    let nanos = match t.duration_since(UNIX_EPOCH) {
        Ok(after) => i128::try_from(after.as_nanos()).ok()?,
        Err(before) => -i128::try_from(before.duration().as_nanos()).ok()?,
    };
    OffsetDateTime::from_unix_timestamp_nanos(nanos).ok()
}

/// True when `path` resolves to a local drive. The path is resolved first (every link,
/// junction and drive mapping on the way), so a mapped network drive, which resolves to a
/// `\\?\UNC\…` path, is refused, and so is a path that can't be resolved or resolves to
/// anything but `\\?\X:\…`.
#[cfg(windows)]
pub fn is_local(path: &Path) -> bool {
    if is_remote_or_device(path) {
        return false;
    }
    let Ok(resolved) = fs::canonicalize(path) else {
        return false;
    };
    let text = resolved.as_os_str().to_string_lossy();
    let Some(rest) = text.strip_prefix(r"\\?\") else {
        return false;
    };
    let b = rest.as_bytes();
    b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'\\'
}

/// Other systems: network mounts can't be told apart cheaply, so only the path form is
/// checked (ADR-024 says so).
#[cfg(not(windows))]
pub fn is_local(path: &Path) -> bool {
    !is_remote_or_device(path)
}

/// A UNC or device path would make Windows reach another machine just to read a log.
pub fn is_remote_or_device(path: &Path) -> bool {
    let text = path.as_os_str().to_string_lossy();
    text.starts_with(r"\\") || text.starts_with("//")
}

#[cfg(windows)]
pub fn is_reparse_point(meta: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
pub fn is_reparse_point(_: &Metadata) -> bool {
    false
}

/// Metadata of `path` itself (links not followed) if it is a plain folder.
pub fn plain_dir(path: &Path) -> Option<Metadata> {
    if is_remote_or_device(path) || !path.is_absolute() {
        return None;
    }
    let m = fs::symlink_metadata(path).ok()?;
    (m.file_type().is_dir() && !is_reparse_point(&m)).then_some(m)
}

/// Metadata of `path` itself (links not followed) if it is a plain file.
pub fn plain_file(path: &Path) -> Option<Metadata> {
    if is_remote_or_device(path) || !path.is_absolute() {
        return None;
    }
    let m = fs::symlink_metadata(path).ok()?;
    (m.file_type().is_file() && !is_reparse_point(&m)).then_some(m)
}

/// What a folder holds, each entry checked without following links. `Err` when the folder
/// can't be read at all. Stops (and marks the budget incomplete) when the budget runs out.
pub fn list(dir: &Path, budget: &mut Budget) -> Result<Vec<(PathBuf, Metadata)>, ()> {
    let entries = fs::read_dir(dir).map_err(|_| ())?;
    let mut out = Vec::new();
    for entry in entries {
        if !budget.take_entry() {
            break;
        }
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.file_type().is_symlink() || is_reparse_point(&meta) {
            continue;
        }
        out.push((path, meta));
    }
    Ok(out)
}

/// The start and the end of a file, never the middle.
#[derive(Debug, Default)]
pub struct Window {
    pub head: Vec<u8>,
    pub tail: Vec<u8>,
    /// The head is the whole file (then `tail` is the same bytes).
    pub whole: bool,
    /// The tail starts mid-file, so its first line is cut and skipped.
    pub tail_cut: bool,
}

#[cfg(unix)]
fn open_no_follow(path: &Path) -> Option<File> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .ok()
}

#[cfg(windows)]
fn open_no_follow(path: &Path) -> Option<File> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    fs::OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .ok()
}

#[cfg(not(any(unix, windows)))]
fn open_no_follow(_: &Path) -> Option<File> {
    None
}

/// Reads at most `head_max` bytes from the start and `tail_max` from the end of a plain file.
pub fn read_window(path: &Path, head_max: u64, tail_max: u64) -> Option<Window> {
    plain_file(path)?;
    let mut file = open_no_follow(path)?;
    let meta = file.metadata().ok()?;
    if !meta.is_file() || is_reparse_point(&meta) {
        return None;
    }
    let len = meta.len();
    let mut head = Vec::new();
    (&mut file).take(head_max).read_to_end(&mut head).ok()?;
    if len <= head_max {
        return Some(Window {
            tail: head.clone(),
            head,
            whole: true,
            tail_cut: false,
        });
    }
    let start = len.saturating_sub(tail_max);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut tail = Vec::new();
    file.take(tail_max).read_to_end(&mut tail).ok()?;
    Some(Window {
        head,
        tail,
        whole: false,
        tail_cut: start > 0,
    })
}

/// Complete lines of a window. `cut_start`: the first line began before the window and is
/// skipped. `cut_end`: the last line may go on past the window and is skipped unless it ended
/// with a newline. Empty lines are dropped.
pub fn lines(buf: &[u8], cut_start: bool, cut_end: bool) -> impl Iterator<Item = &[u8]> {
    let mut body = buf;
    if cut_start {
        body = match body.iter().position(|&b| b == b'\n') {
            Some(i) => &body[i + 1..],
            None => &[],
        };
    }
    if cut_end && !body.ends_with(b"\n") {
        body = match body.iter().rposition(|&b| b == b'\n') {
            Some(i) => &body[..=i],
            None => &[],
        };
    }
    body.split(|&b| b == b'\n')
        .map(|l| l.strip_suffix(b"\r").unwrap_or(l))
        .filter(|l| !l.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_skip_cut_edges() {
        let all: Vec<_> = lines(b"a\nb\nc", false, false).collect();
        assert_eq!(all, [b"a".as_slice(), b"b", b"c"]);
        let cut: Vec<_> = lines(b"tail-of-a\nb\nc-start", true, true).collect();
        assert_eq!(cut, [b"b".as_slice()]);
        let ended: Vec<_> = lines(b"x\r\ny\n", true, true).collect();
        assert_eq!(ended, [b"y".as_slice()]);
        assert_eq!(lines(b"no newline at all", true, true).count(), 0);
    }

    #[test]
    fn window_reads_only_the_edges() {
        let dir = tempfile::tempdir().expect("tmp");
        let p = dir.path().join("f.jsonl");
        let mut big = b"HEAD\n".to_vec();
        big.extend(std::iter::repeat_n(b'm', 10_000));
        big.extend(b"\nTAIL\n");
        fs::write(&p, &big).expect("write");
        let w = read_window(&p, 16, 16).expect("read");
        assert!(!w.whole && w.tail_cut);
        assert_eq!(w.head.len(), 16);
        assert_eq!(w.tail.len(), 16);
        assert!(!w.head.contains(&b'T') || w.head.starts_with(b"HEAD"));
        assert!(w.tail.ends_with(b"TAIL\n"));
        let small = dir.path().join("s.jsonl");
        fs::write(&small, b"one\n").expect("write");
        let w = read_window(&small, 16, 16).expect("read");
        assert!(w.whole);
        assert_eq!(w.head, w.tail);
    }

    #[test]
    fn budget_runs_out() {
        let mut b = Budget::new(Duration::from_secs(5), 2);
        assert!(b.take_entry() && b.take_entry());
        assert!(!b.take_entry());
        assert!(b.incomplete);
        let mut late = Budget::new(Duration::ZERO, 10);
        assert!(!late.time_left());
        assert!(late.incomplete);
    }

    #[test]
    fn relative_and_network_paths_are_refused() {
        assert!(plain_dir(Path::new("relative")).is_none());
        assert!(is_remote_or_device(Path::new(r"\\server\share")));
        assert!(is_remote_or_device(Path::new("//server/share")));
        assert!(!is_local(Path::new(r"\\server\share")));
        let dir = tempfile::tempdir().expect("tmp");
        assert!(is_local(dir.path()));
        if cfg!(windows) {
            assert!(!is_local(&dir.path().join("missing")));
        }
    }

    #[test]
    fn out_of_range_file_times_are_none() {
        let far = UNIX_EPOCH + Duration::from_secs(400_000_000_000);
        assert!(file_time(far).is_none());
        let before = UNIX_EPOCH - Duration::from_secs(1);
        assert_eq!(file_time(before).map(|t| t.unix_timestamp()), Some(-1));
        assert!(file_time(SystemTime::now()).is_some());
    }
}
