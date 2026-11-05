//! Session title from Claude Code's transcript file (Sprint 05, S05-T11, ADR-013).
//!
//! Claude Code appends `{"type":"custom-title","customTitle":"…","sessionId":"…"}` records to
//! the transcript when the user names a session (`/rename`, `-n`). The last one is current.
//! The transcript format is internal to Claude Code and may change; every problem here means
//! "no title", silently, and the hook carries on (zero-effect contract).
//!
//! Call [`read_title`] only when the user opted in to capture. It reads at most the last
//! [`TAIL_BYTES`] of one file, whose name must be exactly `<session_id>.jsonl`, and keeps
//! nothing but the cleaned title. The title is never logged.

use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use agent_ipc::MAX_TITLE_CHARS;
use serde::Deserialize;
use serde_json::Value;

use crate::redact;

/// Only this much of the end of the transcript is read.
pub const TAIL_BYTES: u64 = 256 * 1024;
/// Lines longer than this are never parsed (a title record is a few hundred bytes).
const MAX_LINE_BYTES: usize = 16 * 1024;
const RECORD_TYPE: &str = "custom-title";

/// The cleaned title of the session whose transcript is at `path`, or `None`.
pub fn read_title(path: &Path, session_id: &str) -> Option<String> {
    if !path.is_absolute() || session_id.is_empty() {
        return None;
    }
    if path.file_name()? != std::ffi::OsString::from(format!("{session_id}.jsonl")).as_os_str() {
        return None;
    }
    // Network and device paths are never opened: a UNC path would make Windows connect to
    // another machine (and offer the user's credentials) just to read a title.
    if is_remote_or_device(path) {
        return None;
    }
    // A link could point anywhere; only a plain file is read.
    let meta = fs::symlink_metadata(path).ok()?;
    if !meta.file_type().is_file() || is_reparse_point(&meta) {
        return None;
    }
    // Opened without following a final link and without blocking (a FIFO swapped in after the
    // check above must not hang the hook); then the handle must be the same plain file.
    let mut file = open_no_follow(path)?;
    let opened = file.metadata().ok()?;
    if !opened.is_file() || is_reparse_point(&opened) || !same_file(&meta, &opened) {
        return None;
    }
    let len = opened.len();
    let start = len.saturating_sub(TAIL_BYTES);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut tail = Vec::new();
    file.take(TAIL_BYTES).read_to_end(&mut tail).ok()?;
    title_from_tail(&tail, start > 0)
}

fn is_remote_or_device(path: &Path) -> bool {
    let text = path.as_os_str().to_string_lossy();
    text.starts_with(r"\\") || text.starts_with("//")
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

#[cfg(unix)]
fn same_file(a: &fs::Metadata, b: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    a.dev() == b.dev() && a.ino() == b.ino()
}

#[cfg(not(unix))]
fn same_file(a: &fs::Metadata, b: &fs::Metadata) -> bool {
    // The handle was opened on the reparse point itself, so a swapped-in link fails
    // `is_reparse_point` on the handle; size and creation time catch a plain-file swap.
    a.len() == b.len() && a.created().ok() == b.created().ok()
}

#[cfg(windows)]
fn is_reparse_point(meta: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_point(_: &fs::Metadata) -> bool {
    false
}

#[derive(Deserialize)]
struct Record {
    #[serde(rename = "type")]
    kind: Option<Value>,
    #[serde(rename = "customTitle")]
    custom_title: Option<Value>,
}

/// The last title record in `tail`. When `partial_start` is true the window began mid-file, so
/// everything up to the first line break is a cut-off line and is skipped.
pub fn title_from_tail(tail: &[u8], partial_start: bool) -> Option<String> {
    let body = if partial_start {
        let nl = tail.iter().position(|&b| b == b'\n')?;
        &tail[nl + 1..]
    } else {
        tail
    };
    body.split(|&b| b == b'\n')
        .rev()
        .filter(|line| !line.is_empty() && line.len() <= MAX_LINE_BYTES)
        .filter(|line| contains(line, RECORD_TYPE.as_bytes()))
        .find_map(|line| {
            let rec: Record = serde_json::from_slice(line).ok()?;
            if rec.kind.as_ref().and_then(Value::as_str) != Some(RECORD_TYPE) {
                return None;
            }
            clean_title(rec.custom_title.as_ref()?.as_str()?)
        })
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// Same cleaning as captured text, then one line of at most [`MAX_TITLE_CHARS`] characters.
/// The cut comes last, so it can't leave half a secret behind.
pub fn clean_title(raw: &str) -> Option<String> {
    let cleaned = redact::clean(raw, 2_000);
    let one_line = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let cut: String = one_line.chars().take(MAX_TITLE_CHARS).collect();
    let cut = cut.trim_end().to_owned();
    (!cut.is_empty()).then_some(cut)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_line_of_a_cut_window_is_skipped() {
        let full = b"{\"type\":\"custom-title\",\"customTitle\":\"A\"}\n";
        assert_eq!(title_from_tail(full, false).as_deref(), Some("A"));
        assert_eq!(title_from_tail(full, true), None);
        let two = b"{\"type\":\"custom-title\",\"customTitle\":\"A\"}\n{\"type\":\"custom-title\",\"customTitle\":\"B\"}\n";
        assert_eq!(title_from_tail(two, true).as_deref(), Some("B"));
        assert_eq!(title_from_tail(b"no newline at all", true), None);
    }

    #[test]
    fn network_and_device_paths_are_never_opened() {
        assert!(is_remote_or_device(Path::new(r"\\host\share\s.jsonl")));
        assert!(is_remote_or_device(Path::new(r"\\?\C:\x\s.jsonl")));
        assert!(is_remote_or_device(Path::new(r"\\.\pipe\s.jsonl")));
        assert!(is_remote_or_device(Path::new("//host/share/s.jsonl")));
        assert!(!is_remote_or_device(Path::new(
            "/home/u/.claude/projects/p/s.jsonl"
        )));
        assert_eq!(read_title(Path::new(r"\\host\share\s.jsonl"), "s"), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_fifo_is_not_read_and_does_not_block() {
        let dir = std::env::temp_dir().join(format!("lullshift-fifo-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("dir");
        let fifo = dir.join("s.jsonl");
        let made = std::process::Command::new("mkfifo").arg(&fifo).status();
        if !made.map(|s| s.success()).unwrap_or(false) {
            eprintln!("skipped: mkfifo isn't available");
            return;
        }
        assert_eq!(read_title(&fifo, "s"), None);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn clean_title_rules() {
        assert_eq!(clean_title("  a \n b ").as_deref(), Some("a b"));
        assert_eq!(clean_title(" \u{200b} "), None);
        let cut = clean_title(&"word ".repeat(100)).expect("title");
        assert!((115..=120).contains(&cut.len()), "{}", cut.len());
    }
}
