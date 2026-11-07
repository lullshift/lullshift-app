//! Packaged-app context and stale connection files (decision 33, ADR-010 amendment).
//!
//! Windows gives a program started from inside an MSIX or Store app (for example a terminal in
//! the Claude desktop app) that app's package identity, and redirects its AppData writes into a
//! private copy under `%LOCALAPPDATA%\Packages\<family>\LocalCache\Roaming`. Reads fall through
//! to the real AppData only when no private copy exists. So:
//!
//! - Lullshift started inside a package writes `ipc/endpoint.json` and `ipc/token` where agents
//!   outside the package can't see them. [`packaged_context`] tells the UI to warn about this.
//! - A private copy left by such a run outlives it. Agents running inside that package later
//!   read the old address and token, and their events go nowhere. [`connection_health`] finds
//!   those copies, and [`connection_fix_stale`] deletes them when the user asks.
//!
//! Scope, strictly: only `endpoint.json` and `token` (plus reading the hook's data-free failure
//! marker), only at `<Packages>\<family>\LocalCache\Roaming\<our data dir name>\ipc\`, never
//! through a reparse point (symlink, junction, mount point), at most [`MAX_PACKAGES`] package
//! folders and [`SCAN_BUDGET`] per scan. File contents are compared in memory and never logged
//! or returned. Nothing is deleted without the user pressing Fix, and the private database copy
//! and every other file are left alone.

use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use agent_adapters::hook_failure::{self, FailureMarker, FailureReason};
use agent_ipc::token::TOKEN_FILE;
use agent_ipc::transport::{ENDPOINT_FILE, IPC_DIR};
use serde::Serialize;
use tauri::{AppHandle, Manager};
use time::OffsetDateTime;
use zeroize::Zeroizing;

use crate::Core;

/// Package folders looked at per scan; a machine with more is reported as a partial scan.
pub(crate) const MAX_PACKAGES: usize = 2_000;
/// Time allowed for one scan; past it the scan stops and is reported as partial.
pub(crate) const SCAN_BUDGET: Duration = Duration::from_secs(3);
/// Larger "connection files" are not ours and are neither read nor deleted.
const MAX_CONNECTION_FILE_BYTES: u64 = 16 * 1024;
/// Failure markers from further in the future than this are ignored as implausible.
const CLOCK_SLACK: time::Duration = time::Duration::minutes(5);
/// The most failure notes the UI gets.
const MAX_FAILURES: usize = 8;

/// One of the two connection files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ConnFile {
    /// `endpoint.json`: where the app listens.
    Endpoint,
    /// `token`: the shared secret. Its content never leaves this module.
    Token,
}

impl ConnFile {
    const ALL: [Self; 2] = [Self::Endpoint, Self::Token];

    fn name(self) -> &'static str {
        match self {
            Self::Endpoint => ENDPOINT_FILE,
            Self::Token => TOKEN_FILE,
        }
    }
}

/// A Windows package, by family name, with a readable name for the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PackageRef {
    /// For example `Claude_pzs8sxrjxfjjc`.
    family: String,
    /// The part before the publisher id, for example `Claude`.
    name: String,
}

impl PackageRef {
    /// `None` unless `family` looks like a package family name: 1 to 128 ASCII letters, digits,
    /// dots, dashes and underscores, not starting with a dot. Anything else is never shown or
    /// used to build a path.
    pub(crate) fn new(family: &str) -> Option<Self> {
        let ok = (1..=128).contains(&family.len())
            && !family.starts_with('.')
            // Windows drops a trailing dot, so `X.` would name the folder `X`.
            && !family.ends_with('.')
            && family
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
            && !is_device_name(family);
        if !ok {
            return None;
        }
        let name = family
            .rsplit_once('_')
            .map(|(n, _)| n)
            .filter(|n| !n.is_empty())
            .unwrap_or(family);
        Some(Self {
            family: family.to_owned(),
            name: name.to_owned(),
        })
    }
}

/// A reserved Windows device name (`CON`, `NUL`, `COM1`, ...), with or without an extension,
/// in any case. Such a name opens a device, not a folder.
fn is_device_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
}

/// Compares two byte strings in time that depends only on their lengths, so a token copy is
/// never compared byte by byte with an early exit.
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    let n = a.len().max(b.len());
    let mut diff = u8::from(a.len() != b.len());
    for i in 0..n {
        diff |= a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0);
    }
    diff == 0
}

/// Where to look. Built from the app's own paths in [`Roots::for_app`]; tests point it at a
/// temporary folder.
pub(crate) struct Roots {
    /// `%LOCALAPPDATA%\Packages`.
    pub(crate) packages: PathBuf,
    /// The last component of the app data dir (`app.lullshift.desktop`), as the app uses it.
    pub(crate) dir_name: OsString,
    /// The app's real `ipc` directory.
    pub(crate) real_ipc: PathBuf,
}

impl Roots {
    fn for_app(app: &AppHandle) -> Option<Self> {
        let data = app.path().app_data_dir().ok()?;
        let local = app.path().local_data_dir().ok()?;
        Some(Self {
            packages: local.join("Packages"),
            dir_name: data.file_name()?.to_owned(),
            real_ipc: data.join(IPC_DIR),
        })
    }
}

fn is_reparse(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        meta.file_type().is_symlink() || meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        meta.file_type().is_symlink()
    }
}

/// A real directory: exists, is a directory, and is not a reparse point.
fn plain_dir(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_dir() && !is_reparse(&m))
}

/// A regular file of at most `cap` bytes that is not a reparse point.
fn plain_file(path: &Path, cap: u64) -> bool {
    fs::symlink_metadata(path)
        .is_ok_and(|m| m.file_type().is_file() && !is_reparse(&m) && m.len() <= cap)
}

/// The bytes of a [`plain_file`], read with the same cap. Used for the app's own real `ipc`
/// folder only; files in other packages' storage go through [`read_private`].
fn read_plain(path: &Path, cap: u64) -> Option<Zeroizing<Vec<u8>>> {
    if !plain_file(path, cap) {
        return None;
    }
    read_capped(fs::File::open(path).ok()?, cap)
}

fn read_capped(file: fs::File, cap: u64) -> Option<Zeroizing<Vec<u8>>> {
    let mut bytes = Zeroizing::new(Vec::new());
    file.take(cap + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() as u64 <= cap).then_some(bytes)
}

/// Opens `ipc\name` in a package's private copy. On Windows the open never follows a reparse
/// point at the file itself, and the open handle must be a plain file of at most `cap` bytes
/// whose final path is exactly `ipc\name` (`ipc` is built on the canonical Packages folder), so
/// a junction anywhere on the way, even one swapped in after the directory checks, is refused.
/// `delete` also asks for delete access, so the same handle can delete the file.
fn open_private(ipc: &Path, name: &str, cap: u64, delete: bool) -> Option<fs::File> {
    let path = ipc.join(name);
    #[cfg(windows)]
    {
        windows_package::file::open_checked(&path, &path, cap, delete).ok()
    }
    #[cfg(not(windows))]
    {
        // The scan and Fix run only on Windows; this keeps the logic testable elsewhere.
        let _ = delete;
        if !plain_file(&path, cap) {
            return None;
        }
        fs::File::open(&path).ok()
    }
}

/// Deletes the file behind a handle from [`open_private`] with `delete`.
fn delete_private(file: fs::File, path: &Path) -> bool {
    #[cfg(windows)]
    {
        let _ = path;
        windows_package::file::delete(&file).is_ok()
    }
    #[cfg(not(windows))]
    {
        drop(file);
        fs::remove_file(path).is_ok()
    }
}

/// The bytes of `ipc\name` in a package's private copy, read through the checked handle.
fn read_private(ipc: &Path, name: &str, cap: u64) -> Option<Zeroizing<Vec<u8>>> {
    read_capped(open_private(ipc, name, cap, false)?, cap)
}

/// `<packages>\<family>\LocalCache\Roaming\<dir_name>\ipc`, if every step is a real directory.
/// Built on the canonical Packages folder, so it is the exact final path Windows reports for
/// files in it.
fn package_ipc_dir(roots: &Roots, family: &str) -> Option<PathBuf> {
    PackageRef::new(family)?;
    if !plain_dir(&roots.packages) {
        return None;
    }
    let mut path = fs::canonicalize(&roots.packages).ok()?;
    for part in [
        OsString::from(family),
        "LocalCache".into(),
        "Roaming".into(),
        roots.dir_name.clone(),
        IPC_DIR.into(),
    ] {
        path.push(part);
        if !plain_dir(&path) {
            return None;
        }
    }
    Some(path)
}

/// Whether `private` differs from the real file's content (whitespace at the ends aside).
fn differs(private: &[u8], real_ipc: &Path, file: ConnFile) -> bool {
    let real = read_plain(&real_ipc.join(file.name()), MAX_CONNECTION_FILE_BYTES);
    match real {
        Some(r) => !ct_eq(r.trim_ascii(), private.trim_ascii()),
        None => true,
    }
}

/// Whether the private copy of `file` in `ipc` differs from the real one. `None` when there is
/// no private copy we would touch (missing, a link, or too large).
fn is_stale(ipc: &Path, real_ipc: &Path, file: ConnFile) -> Option<bool> {
    let private = read_private(ipc, file.name(), MAX_CONNECTION_FILE_BYTES)?;
    Some(differs(&private, real_ipc, file))
}

fn stale_files(ipc: &Path, real_ipc: &Path) -> Vec<ConnFile> {
    ConnFile::ALL
        .into_iter()
        .filter(|f| is_stale(ipc, real_ipc, *f) == Some(true))
        .collect()
}

fn read_marker_with(bytes: Option<Zeroizing<Vec<u8>>>) -> Option<FailureMarker> {
    hook_failure::parse(&bytes?)
}

/// What one package's private copy holds that matters.
#[derive(Debug)]
pub(crate) struct PackageCopy {
    pub(crate) package: PackageRef,
    pub(crate) stale: Vec<ConnFile>,
    pub(crate) marker: Option<FailureMarker>,
}

#[derive(Debug, Default)]
pub(crate) struct Scan {
    /// Packages with stale files or a failure marker in their private copy.
    pub(crate) copies: Vec<PackageCopy>,
    /// The package or time limit was reached before every folder was looked at.
    pub(crate) truncated: bool,
}

/// Looks through at most `max` package folders within `budget` for private copies of our
/// connection files that differ from the real ones, and for failure markers.
pub(crate) fn scan(roots: &Roots, max: usize, budget: Duration) -> Scan {
    let mut out = Scan::default();
    if !plain_dir(&roots.packages) {
        return out;
    }
    let Ok(entries) = fs::read_dir(&roots.packages) else {
        return out;
    };
    let started = Instant::now();
    for (seen, entry) in entries.enumerate() {
        if seen >= max || started.elapsed() >= budget {
            out.truncated = true;
            break;
        }
        let Ok(entry) = entry else { continue };
        let name = entry.file_name();
        let Some(package) = name.to_str().and_then(PackageRef::new) else {
            continue;
        };
        let Some(ipc) = package_ipc_dir(roots, &package.family) else {
            continue;
        };
        let stale = stale_files(&ipc, &roots.real_ipc);
        let marker = read_marker_with(read_private(
            &ipc,
            hook_failure::MARKER_FILE,
            hook_failure::MAX_MARKER_BYTES,
        ));
        if !stale.is_empty() || marker.is_some() {
            out.copies.push(PackageCopy {
                package,
                stale,
                marker,
            });
        }
    }
    out.copies
        .sort_by(|a, b| a.package.family.cmp(&b.package.family));
    out
}

/// What Fix did, per file.
#[derive(Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FixReport {
    pub(crate) deleted: Vec<ConnFile>,
    /// Found up to date when Fix ran, so left alone.
    pub(crate) current: Vec<ConnFile>,
    /// Stale but couldn't be deleted, or no longer a plain file at the expected path.
    pub(crate) failed: Vec<ConnFile>,
}

/// Whether `family` is a folder name in a fresh listing of the Packages folder (exact match).
fn listed(roots: &Roots, family: &str) -> bool {
    fs::read_dir(&roots.packages).is_ok_and(|entries| {
        entries
            .take(MAX_PACKAGES)
            .filter_map(Result::ok)
            .any(|e| e.file_name().to_str() == Some(family))
    })
}

/// Deletes the stale connection files in `family`'s private copy. The family name must be
/// valid and appear in a fresh listing; every directory on the way must be a real directory;
/// each file is then opened by handle (see [`open_private`]), its content compared through that
/// handle, and only a still-stale file is deleted through that same handle. Nothing else is
/// touched.
pub(crate) fn fix(roots: &Roots, family: &str) -> Result<FixReport, &'static str> {
    fix_with(roots, family, |_| {})
}

/// [`fix`] with a hook that runs after the directory checks and before each file is opened, so
/// tests can swap a folder for a junction in between.
fn fix_with(
    roots: &Roots,
    family: &str,
    mut before_open: impl FnMut(&Path),
) -> Result<FixReport, &'static str> {
    PackageRef::new(family).ok_or("invalid_package")?;
    if !listed(roots, family) {
        return Err("not_found");
    }
    let ipc = package_ipc_dir(roots, family).ok_or("not_found")?;
    let mut report = FixReport::default();
    for file in ConnFile::ALL {
        let path = ipc.join(file.name());
        if fs::symlink_metadata(&path).is_err() {
            continue;
        }
        before_open(&ipc);
        let Some(handle) = open_private(&ipc, file.name(), MAX_CONNECTION_FILE_BYTES, true) else {
            report.failed.push(file);
            continue;
        };
        let Ok(handle_clone) = handle.try_clone() else {
            report.failed.push(file);
            continue;
        };
        let Some(content) = read_capped(handle_clone, MAX_CONNECTION_FILE_BYTES) else {
            report.failed.push(file);
            continue;
        };
        if !differs(&content, &roots.real_ipc, file) {
            report.current.push(file);
        } else if delete_private(handle, &path) {
            report.deleted.push(file);
        } else {
            report.failed.push(file);
        }
    }
    Ok(report)
}

/// One Fix at a time: two concurrent calls would race on the same files and report the loser's
/// files as failed.
static FIX_LOCK: Mutex<()> = Mutex::new(());
/// When Fix last ran for each package in this run of the app. Markers in that package written
/// before it are about the problem Fix just removed.
static FIXED: Mutex<Vec<(String, OffsetDateTime)>> = Mutex::new(Vec::new());

fn fixed_at(family: &str) -> Option<OffsetDateTime> {
    FIXED
        .lock()
        .ok()?
        .iter()
        .find(|(f, _)| f == family)
        .map(|(_, t)| *t)
}

fn record_fix(family: &str, at: OffsetDateTime) {
    if let Ok(mut fixed) = FIXED.lock() {
        fixed.retain(|(f, _)| f != family);
        fixed.push((family.to_owned(), at));
    }
}

/// Whether a failure marker still means something: written during this run of the app (a
/// failure while Lullshift was closed is expected), not in the future, and after `last_ok`.
/// For the real data folder `last_ok` is the last event that adapter delivered. For a package's
/// private copy it is only the last Fix of that package: the app can't tell whether an event
/// came from inside that package, so a success from elsewhere must not hide its failure.
pub(crate) fn marker_is_relevant(
    marker: &FailureMarker,
    app_started: OffsetDateTime,
    last_ok: Option<OffsetDateTime>,
    now: OffsetDateTime,
) -> bool {
    marker.utc >= app_started.replace_nanosecond(0).unwrap_or(app_started)
        && marker.utc <= now + CLOCK_SLACK
        && last_ok.is_none_or(|t| marker.utc > t)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackagedContext {
    /// The package Lullshift itself runs inside, or `None` (also on macOS and Linux).
    package: Option<PackageRef>,
}

fn current_package() -> Option<PackageRef> {
    windows_package::current_package_family_name().and_then(|f| PackageRef::new(&f))
}

/// Whether Lullshift itself runs inside another app's package. Cheap; the Sessions view asks.
#[tauri::command]
pub fn packaged_context() -> PackagedContext {
    PackagedContext {
        package: current_package(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ScanStatus {
    /// Every package folder was looked at.
    Complete,
    /// The folder or time limit was reached; some packages weren't looked at.
    Partial,
    /// Not looked at: Lullshift runs inside a package itself, or this isn't Windows.
    Skipped,
    /// The app's folders couldn't be determined.
    Unavailable,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StaleCopyView {
    package: PackageRef,
    files: Vec<ConnFile>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookFailureView {
    adapter: &'static str,
    reason: FailureReason,
    #[serde(with = "time::serde::rfc3339")]
    utc: OffsetDateTime,
    /// The package whose private copy held the marker; `None` for the real data folder.
    package: Option<PackageRef>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionHealth {
    package: Option<PackageRef>,
    scan: ScanStatus,
    stale: Vec<StaleCopyView>,
    failures: Vec<HookFailureView>,
}

/// The latest time each provider's event was received, from the stored sessions.
fn last_success_by_provider(core: &Core) -> HashMap<String, OffsetDateTime> {
    let mut out: HashMap<String, OffsetDateTime> = HashMap::new();
    let Ok(guard) = core.store.lock() else {
        return out;
    };
    let Some(records) = guard.as_ref().and_then(|s| s.sessions().ok()) else {
        return out;
    };
    for r in records {
        let t = out.entry(r.provider).or_insert(r.last_received_utc);
        if r.last_received_utc > *t {
            *t = r.last_received_utc;
        }
    }
    out
}

fn health(core: &Core, roots: Option<&Roots>, package: Option<PackageRef>) -> ConnectionHealth {
    let (scan_status, scanned) = match roots {
        None => (ScanStatus::Unavailable, Scan::default()),
        Some(_) if package.is_some() || !cfg!(windows) => (ScanStatus::Skipped, Scan::default()),
        Some(r) => {
            let s = scan(r, MAX_PACKAGES, SCAN_BUDGET);
            let status = if s.truncated {
                ScanStatus::Partial
            } else {
                ScanStatus::Complete
            };
            (status, s)
        }
    };
    let now = OffsetDateTime::now_utc();
    let last = last_success_by_provider(core);
    let relevant = |m: &FailureMarker, package: Option<&PackageRef>| {
        let last_ok = match package {
            None => last.get(m.adapter).copied(),
            Some(p) => fixed_at(&p.family),
        };
        marker_is_relevant(m, core.started_utc, last_ok, now)
    };
    let mut failures: Vec<HookFailureView> = Vec::new();
    if let Some(m) = roots.and_then(|r| {
        read_marker_with(read_plain(
            &r.real_ipc.join(hook_failure::MARKER_FILE),
            hook_failure::MAX_MARKER_BYTES,
        ))
    }) && relevant(&m, None)
    {
        failures.push(HookFailureView {
            adapter: m.adapter,
            reason: m.reason,
            utc: m.utc,
            package: None,
        });
    }
    for copy in &scanned.copies {
        if let Some(m) = copy
            .marker
            .as_ref()
            .filter(|m| relevant(m, Some(&copy.package)))
        {
            failures.push(HookFailureView {
                adapter: m.adapter,
                reason: m.reason,
                utc: m.utc,
                package: Some(copy.package.clone()),
            });
        }
    }
    failures.sort_by_key(|f| std::cmp::Reverse(f.utc));
    failures.truncate(MAX_FAILURES);
    ConnectionHealth {
        package,
        scan: scan_status,
        stale: scanned
            .copies
            .into_iter()
            .filter(|c| !c.stale.is_empty())
            .map(|c| StaleCopyView {
                package: c.package,
                files: c.stale,
            })
            .collect(),
        failures,
    }
}

/// Packaged context, stale private copies of the connection files, and recent hook failures,
/// for Integrations. Reads only; deletes nothing.
#[tauri::command]
pub async fn connection_health(
    app: AppHandle,
    core: tauri::State<'_, Arc<Core>>,
) -> Result<ConnectionHealth, String> {
    let core = Arc::clone(&core);
    tauri::async_runtime::spawn_blocking(move || {
        health(&core, Roots::for_app(&app).as_ref(), current_package())
    })
    .await
    .map_err(|_| "internal".to_owned())
}

/// Deletes the stale connection files found in one package's private copy, after the user
/// pressed Fix. Refused while Lullshift itself runs inside a package (its own reads and writes
/// are redirected then) and outside Windows.
#[tauri::command]
pub async fn connection_fix_stale(app: AppHandle, family: String) -> Result<FixReport, String> {
    if !cfg!(windows) {
        return Err("unsupported".into());
    }
    if current_package().is_some() {
        return Err("packaged".into());
    }
    let roots = Roots::for_app(&app).ok_or("unavailable")?;
    tauri::async_runtime::spawn_blocking(move || {
        let _one_at_a_time = FIX_LOCK.lock().map_err(|_| "internal".to_owned())?;
        let report = fix(&roots, &family).map_err(str::to_owned)?;
        record_fix(&family, OffsetDateTime::now_utc());
        Ok(report)
    })
    .await
    .map_err(|_| "internal".to_owned())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_adapters::hook_failure::FailureReason;

    const DIR: &str = "app.lullshift.desktop";
    const REAL_TOKEN: &str = "1111111111111111111111111111111111111111111111111111111111111111";
    const OLD_TOKEN: &str = "2222222222222222222222222222222222222222222222222222222222222222";
    const REAL_ENDPOINT: &str = r#"{"protocol":1,"transport":"windows_named_pipe","address":"\\\\.\\pipe\\lullshift-ipc-v1-new"}"#;
    const OLD_ENDPOINT: &str = r#"{"protocol":1,"transport":"windows_named_pipe","address":"\\\\.\\pipe\\lullshift-ipc-v1-old"}"#;

    struct Fixture {
        _tmp: tempfile::TempDir,
        roots: Roots,
    }

    fn fixture() -> Fixture {
        let tmp = tempfile::tempdir().expect("tempdir");
        let real_ipc = tmp.path().join("Roaming").join(DIR).join("ipc");
        fs::create_dir_all(&real_ipc).expect("real ipc");
        fs::write(real_ipc.join("endpoint.json"), REAL_ENDPOINT).expect("endpoint");
        fs::write(real_ipc.join("token"), REAL_TOKEN).expect("token");
        let packages = tmp.path().join("Local").join("Packages");
        fs::create_dir_all(&packages).expect("packages");
        Fixture {
            roots: Roots {
                packages,
                dir_name: DIR.into(),
                real_ipc,
            },
            _tmp: tmp,
        }
    }

    impl Fixture {
        /// A package's private ipc dir with the given files (name, content).
        fn package(&self, family: &str, files: &[(&str, &str)]) -> PathBuf {
            let ipc = self
                .roots
                .packages
                .join(family)
                .join("LocalCache")
                .join("Roaming")
                .join(DIR)
                .join("ipc");
            fs::create_dir_all(&ipc).expect("package ipc");
            for (name, content) in files {
                fs::write(ipc.join(name), content).expect("file");
            }
            ipc
        }
    }

    /// A directory link at `link` pointing to `target`: a junction on Windows (no admin rights
    /// needed), a symlink elsewhere. `false` if the system wouldn't create one.
    fn dir_link(target: &Path, link: &Path) -> bool {
        #[cfg(windows)]
        {
            std::process::Command::new("cmd")
                .arg("/C")
                .arg("mklink")
                .arg("/J")
                .arg(link)
                .arg(target)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok_and(|s| s.success())
        }
        #[cfg(not(windows))]
        {
            std::os::unix::fs::symlink(target, link).is_ok()
        }
    }

    #[test]
    fn package_names_are_validated_and_shortened() {
        let p = PackageRef::new("Claude_pzs8sxrjxfjjc").expect("valid");
        assert_eq!(
            (p.family.as_str(), p.name.as_str()),
            ("Claude_pzs8sxrjxfjjc", "Claude")
        );
        assert_eq!(
            PackageRef::new("Microsoft.WindowsTerminal_8wekyb3d8bbwe")
                .expect("valid")
                .name,
            "Microsoft.WindowsTerminal"
        );
        assert_eq!(
            PackageRef::new("NoUnderscore").expect("valid").name,
            "NoUnderscore"
        );
        for bad in [
            "",
            "..",
            ".hidden_x",
            "a/b",
            "a\\b",
            "a b",
            "név_x",
            &"x".repeat(129),
            "Claude_abc.",
            "CON",
            "nul",
            "Aux.txt",
            "com1",
            "LPT9.x_y",
        ] {
            assert!(PackageRef::new(bad).is_none(), "{bad}");
        }
        // Names that only start like a device are fine.
        assert!(PackageRef::new("CONSOLE_x").is_some());
        assert!(PackageRef::new("COM0_x").is_some());
        assert!(PackageRef::new("Contoso.App_x").is_some());
    }

    #[test]
    fn constant_time_compare_matches_equality() {
        assert!(ct_eq(b"abc", b"abc"));
        assert!(ct_eq(b"", b""));
        assert!(!ct_eq(b"abc", b"abd"));
        assert!(!ct_eq(b"abc", b"abcd"));
        assert!(!ct_eq(b"abc\0", b"abc"));
    }

    #[test]
    fn finds_only_copies_that_differ_from_the_real_files() {
        let f = fixture();
        f.package(
            "Claude_pzs8sxrjxfjjc",
            &[("endpoint.json", OLD_ENDPOINT), ("token", OLD_TOKEN)],
        );
        // Same content (whitespace aside): not stale.
        f.package(
            "Same_abc",
            &[
                ("endpoint.json", REAL_ENDPOINT),
                ("token", &format!("{REAL_TOKEN}\n")),
            ],
        );
        // Only the token is old.
        f.package(
            "Half_abc",
            &[("endpoint.json", REAL_ENDPOINT), ("token", OLD_TOKEN)],
        );
        // Other packages, and ours without an ipc folder.
        fs::create_dir_all(f.roots.packages.join("Other_abc").join("LocalCache")).expect("other");
        fs::create_dir_all(
            f.roots
                .packages
                .join("NoIpc_abc")
                .join("LocalCache")
                .join("Roaming")
                .join(DIR),
        )
        .expect("noipc");
        let s = scan(&f.roots, MAX_PACKAGES, SCAN_BUDGET);
        assert!(!s.truncated);
        let got: Vec<(&str, &[ConnFile])> = s
            .copies
            .iter()
            .map(|c| (c.package.family.as_str(), c.stale.as_slice()))
            .collect();
        assert_eq!(
            got,
            [
                (
                    "Claude_pzs8sxrjxfjjc",
                    &[ConnFile::Endpoint, ConnFile::Token][..]
                ),
                ("Half_abc", &[ConnFile::Token][..]),
            ]
        );
    }

    #[test]
    fn a_private_copy_is_stale_when_the_real_file_is_missing() {
        let f = fixture();
        fs::remove_file(f.roots.real_ipc.join("token")).expect("rm");
        f.package("P_abc", &[("token", REAL_TOKEN)]);
        let s = scan(&f.roots, MAX_PACKAGES, SCAN_BUDGET);
        assert_eq!(s.copies[0].stale, [ConnFile::Token]);
    }

    #[test]
    fn the_scan_stops_at_the_package_limit() {
        let f = fixture();
        for i in 0..5 {
            fs::create_dir(f.roots.packages.join(format!("P{i}_abc"))).expect("pkg");
        }
        assert!(scan(&f.roots, 3, SCAN_BUDGET).truncated);
        assert!(!scan(&f.roots, 5, SCAN_BUDGET).truncated);
        assert!(scan(&f.roots, 5, Duration::ZERO).truncated);
    }

    #[test]
    fn links_anywhere_on_the_path_are_refused() {
        let f = fixture();
        // A real private copy somewhere else, reached through a link at each level in turn.
        let elsewhere = f._tmp.path().join("elsewhere");
        let target_ipc = elsewhere
            .join("LocalCache")
            .join("Roaming")
            .join(DIR)
            .join("ipc");
        fs::create_dir_all(&target_ipc).expect("target");
        fs::write(target_ipc.join("token"), OLD_TOKEN).expect("token");
        if !dir_link(&elsewhere, &f.roots.packages.join("Linked_abc")) {
            eprintln!("skipped: directory links can't be created here");
            return;
        }
        // The ipc folder itself as a link, inside an otherwise real package folder.
        let real_pkg_dir = f
            .roots
            .packages
            .join("InnerLink_abc")
            .join("LocalCache")
            .join("Roaming")
            .join(DIR);
        fs::create_dir_all(&real_pkg_dir).expect("pkg");
        assert!(dir_link(&target_ipc, &real_pkg_dir.join("ipc")));
        let s = scan(&f.roots, MAX_PACKAGES, SCAN_BUDGET);
        assert!(s.copies.is_empty(), "{:?}", s.copies);
        assert_eq!(fix(&f.roots, "Linked_abc"), Err("not_found"));
        assert_eq!(fix(&f.roots, "InnerLink_abc"), Err("not_found"));
        assert!(
            target_ipc.join("token").exists(),
            "nothing behind a link is deleted"
        );
    }

    #[cfg(unix)]
    #[test]
    fn linked_files_are_neither_read_nor_deleted() {
        let f = fixture();
        let outside = f._tmp.path().join("outside-token");
        fs::write(&outside, OLD_TOKEN).expect("outside");
        let ipc = f.package("P_abc", &[]);
        std::os::unix::fs::symlink(&outside, ipc.join("token")).expect("symlink");
        assert!(scan(&f.roots, MAX_PACKAGES, SCAN_BUDGET).copies.is_empty());
        let r = fix(&f.roots, "P_abc").expect("fix");
        assert!(r.deleted.is_empty());
        assert!(ipc.join("token").symlink_metadata().is_ok() && outside.exists());
    }

    #[test]
    fn fix_deletes_exactly_the_stale_files() {
        let f = fixture();
        let ipc = f.package(
            "Claude_pzs8sxrjxfjjc",
            &[
                ("endpoint.json", OLD_ENDPOINT),
                ("token", OLD_TOKEN),
                ("last-hook-failure.json", "{}"),
                ("unrelated.txt", "x"),
            ],
        );
        let db = ipc.parent().expect("data").join("lullshift.db");
        fs::write(&db, "private database").expect("db");
        let r = fix(&f.roots, "Claude_pzs8sxrjxfjjc").expect("fix");
        assert_eq!(r.deleted, [ConnFile::Endpoint, ConnFile::Token]);
        assert!(r.failed.is_empty() && r.current.is_empty());
        assert!(!ipc.join("endpoint.json").exists() && !ipc.join("token").exists());
        assert!(ipc.join("unrelated.txt").exists() && ipc.join("last-hook-failure.json").exists());
        assert!(db.exists(), "the private database copy stays");
        // The real files are untouched.
        assert_eq!(
            fs::read_to_string(f.roots.real_ipc.join("token")).expect("real"),
            REAL_TOKEN
        );
        // Running it again finds nothing to do.
        assert_eq!(
            fix(&f.roots, "Claude_pzs8sxrjxfjjc").expect("again"),
            FixReport::default()
        );
    }

    #[test]
    fn fix_rechecks_staleness_and_refuses_bad_names() {
        let f = fixture();
        let ipc = f.package(
            "P_abc",
            &[("endpoint.json", OLD_ENDPOINT), ("token", OLD_TOKEN)],
        );
        // Between the scan and Fix, the copy became current (for example the real app wrote
        // the same token): it is kept.
        fs::write(ipc.join("token"), REAL_TOKEN).expect("now current");
        let r = fix(&f.roots, "P_abc").expect("fix");
        assert_eq!(r.deleted, [ConnFile::Endpoint]);
        assert_eq!(r.current, [ConnFile::Token]);
        assert!(ipc.join("token").exists());
        for bad in ["..", "../Roaming", "P_abc/../../x", "", "C:\\x"] {
            assert_eq!(fix(&f.roots, bad), Err("invalid_package"), "{bad}");
        }
        assert_eq!(fix(&f.roots, "Missing_abc"), Err("not_found"));
    }

    #[test]
    fn oversized_or_directory_connection_files_are_left_alone() {
        let f = fixture();
        let ipc = f.package("P_abc", &[]);
        fs::write(ipc.join("token"), vec![b'a'; 20_000]).expect("big");
        fs::create_dir(ipc.join("endpoint.json")).expect("dir");
        assert!(scan(&f.roots, MAX_PACKAGES, SCAN_BUDGET).copies.is_empty());
        let r = fix(&f.roots, "P_abc").expect("fix");
        assert!(r.deleted.is_empty());
        assert_eq!(r.failed, [ConnFile::Endpoint, ConnFile::Token]);
        assert!(ipc.join("token").exists() && ipc.join("endpoint.json").is_dir());
    }

    #[test]
    fn markers_are_read_from_private_copies_strictly() {
        let f = fixture();
        f.package(
            "Claude_pzs8sxrjxfjjc",
            &[(
                "last-hook-failure.json",
                r#"{"adapter":"claude-code","reason":"rejected","utc":"2026-11-07T13:05:00Z"}"#,
            )],
        );
        f.package(
            "Bad_abc",
            &[(
                "last-hook-failure.json",
                r#"{"adapter":"claude-code","reason":"rejected","utc":"2026-11-07T13:05:00Z","sessionId":"s"}"#,
            )],
        );
        let s = scan(&f.roots, MAX_PACKAGES, SCAN_BUDGET);
        assert_eq!(s.copies.len(), 1);
        let m = s.copies[0].marker.as_ref().expect("marker");
        assert_eq!(
            (m.adapter, m.reason),
            ("claude-code", FailureReason::Rejected)
        );
        assert!(s.copies[0].stale.is_empty());
    }

    #[test]
    fn markers_count_only_after_start_and_after_the_last_success() {
        use time::macros::datetime;
        let m = FailureMarker {
            adapter: "claude-code",
            reason: FailureReason::Unreachable,
            utc: datetime!(2026-11-07 13:05:00 UTC),
        };
        let start = datetime!(2026-11-07 13:00:00.5 UTC);
        let now = datetime!(2026-11-07 13:10:00 UTC);
        assert!(marker_is_relevant(&m, start, None, now));
        assert!(marker_is_relevant(
            &m,
            start,
            Some(datetime!(2026-11-07 13:04:00 UTC)),
            now
        ));
        // A later event got through: the failure is over.
        assert!(!marker_is_relevant(
            &m,
            start,
            Some(datetime!(2026-11-07 13:06:00 UTC)),
            now
        ));
        // Written while Lullshift wasn't running (this launch started later).
        assert!(!marker_is_relevant(
            &m,
            datetime!(2026-11-07 13:06:00 UTC),
            None,
            now
        ));
        // From the future.
        assert!(!marker_is_relevant(
            &m,
            start,
            None,
            datetime!(2026-11-07 12:59:00 UTC)
        ));
        // The same second the app started still counts (markers carry whole seconds).
        let at_start = FailureMarker {
            utc: datetime!(2026-11-07 13:00:00 UTC),
            ..m
        };
        assert!(marker_is_relevant(&at_start, start, None, now));
    }

    #[test]
    fn health_reports_relevant_markers_and_stale_copies() {
        let f = fixture();
        let core = Core::for_tests(f._tmp.path());
        let now = OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .expect("now");
        let marker = format!(r#"{{"adapter":"codex","reason":"unreachable","utc":"{now}"}}"#);
        fs::write(f.roots.real_ipc.join("last-hook-failure.json"), &marker).expect("real marker");
        f.package(
            "Claude_pzs8sxrjxfjjc",
            &[("token", OLD_TOKEN), ("last-hook-failure.json", &marker)],
        );
        let h = health(&core, Some(&f.roots), None);
        if cfg!(windows) {
            assert_eq!(h.scan, ScanStatus::Complete);
            assert_eq!(h.stale.len(), 1);
            assert_eq!(h.stale[0].files, [ConnFile::Token]);
            assert_eq!(h.failures.len(), 2);
        } else {
            assert_eq!(h.scan, ScanStatus::Skipped);
            assert!(h.stale.is_empty());
            assert_eq!(h.failures.len(), 1);
        }
        assert!(h.failures.iter().any(|x| x.package.is_none()));
        // The token's content is never part of what the UI gets.
        let json = serde_json::to_string(&h).expect("json");
        assert!(!json.contains(OLD_TOKEN) && !json.contains(REAL_TOKEN));
        // Packaged: no scan, nothing about other packages.
        let p = health(
            &core,
            Some(&f.roots),
            PackageRef::new("Claude_pzs8sxrjxfjjc"),
        );
        assert_eq!(p.scan, ScanStatus::Skipped);
        assert!(p.stale.is_empty());
        assert_eq!(health(&core, None, None).scan, ScanStatus::Unavailable);
    }

    /// Review M1: a junction swapped in for `ipc` after the directory checks must not redirect
    /// the delete to the same-named file in the junction's target.
    #[cfg(windows)]
    #[test]
    fn a_junction_swapped_in_before_the_delete_is_refused() {
        let f = fixture();
        let ipc = f.package(
            "Claude_pzs8sxrjxfjjc",
            &[("endpoint.json", OLD_ENDPOINT), ("token", OLD_TOKEN)],
        );
        let victim = f._tmp.path().join("victim");
        fs::create_dir(&victim).expect("victim");
        fs::write(victim.join("token"), OLD_TOKEN).expect("victim token");
        fs::write(victim.join("endpoint.json"), OLD_ENDPOINT).expect("victim endpoint");
        let moved = ipc.with_file_name("ipc-moved");
        let mut swapped = false;
        let r = fix_with(&f.roots, "Claude_pzs8sxrjxfjjc", |dir| {
            if !swapped {
                fs::rename(dir, &moved).expect("move ipc away");
                assert!(dir_link(&victim, dir), "junction");
                swapped = true;
            }
        })
        .expect("fix");
        assert!(r.deleted.is_empty(), "{r:?}");
        assert_eq!(r.failed, [ConnFile::Endpoint, ConnFile::Token]);
        assert_eq!(
            fs::read_to_string(victim.join("token")).expect("victim token survives"),
            OLD_TOKEN
        );
        assert!(victim.join("endpoint.json").exists());
        // The originals were not touched either.
        assert!(moved.join("token").exists() && moved.join("endpoint.json").exists());
    }

    #[test]
    fn fix_needs_the_name_in_a_fresh_listing() {
        let f = fixture();
        f.package("P_abc", &[("token", OLD_TOKEN)]);
        // Same folder under another case: Windows would open it, but it isn't the listed name.
        assert_eq!(fix(&f.roots, "p_ABC"), Err("not_found"));
        assert_eq!(
            fix(&f.roots, "P_abc").expect("fix").deleted,
            [ConnFile::Token]
        );
    }

    #[test]
    fn a_success_outside_a_package_does_not_hide_its_failure() {
        use local_store::IngestOutcome;
        let f = fixture();
        let core = Core::for_tests(f._tmp.path());
        let marker_time = OffsetDateTime::now_utc().replace_nanosecond(0).expect("ns");
        let text = format!(
            r#"{{"adapter":"claude-code","reason":"rejected","utc":"{}"}}"#,
            marker_time
                .format(&time::format_description::well_known::Rfc3339)
                .expect("fmt")
        );
        f.package("Hidden_abc", &[("last-hook-failure.json", &text)]);
        fs::write(f.roots.real_ipc.join("last-hook-failure.json"), &text).expect("real");
        // A later Claude Code event, from a session outside any package.
        let later = marker_time + time::Duration::seconds(5);
        let event = agent_core::parse_event(&format!(
            r#"{{"schemaVersion":1,"eventId":"7d6f1f8e-1c1a-4b5e-9a35-111111111111","provider":"claude-code","providerSessionId":"s","eventType":"working","timestampUtc":"{}","capabilities":[],"metadata":{{}}}}"#,
            later.format(&time::format_description::well_known::Rfc3339).expect("fmt")
        ))
        .expect("event");
        let stored = core
            .store
            .lock()
            .expect("lock")
            .as_mut()
            .expect("store")
            .ingest(&event, later)
            .expect("ingest");
        assert!(matches!(stored, IngestOutcome::Stored { .. }));
        let h = health(&core, Some(&f.roots), None);
        if cfg!(windows) {
            // The real-folder marker is over; the package's is not.
            assert_eq!(h.failures.len(), 1, "{:?}", h.failures);
            assert!(h.failures[0].package.is_some());
            // After a Fix of that package, its older marker is over too.
            record_fix("Hidden_abc", later);
            assert!(health(&core, Some(&f.roots), None).failures.is_empty());
        } else {
            assert!(h.failures.is_empty());
        }
    }
}
