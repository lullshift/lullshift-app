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
use std::sync::Arc;
use std::time::{Duration, Instant};

use agent_adapters::hook_failure::{self, FailureMarker, FailureReason};
use agent_ipc::token::TOKEN_FILE;
use agent_ipc::transport::{ENDPOINT_FILE, IPC_DIR};
use serde::Serialize;
use tauri::{AppHandle, Manager};
use time::OffsetDateTime;

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
            && family
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'));
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

/// The bytes of a [`plain_file`], read with the same cap.
fn read_plain(path: &Path, cap: u64) -> Option<Vec<u8>> {
    if !plain_file(path, cap) {
        return None;
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .ok()?
        .take(cap + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() as u64 <= cap).then_some(bytes)
}

/// `<packages>\<family>\LocalCache\Roaming\<dir_name>\ipc`, if every step is a real directory.
fn package_ipc_dir(roots: &Roots, family: &str) -> Option<PathBuf> {
    PackageRef::new(family)?;
    if !plain_dir(&roots.packages) {
        return None;
    }
    let mut path = roots.packages.clone();
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

fn trimmed(bytes: &[u8]) -> &[u8] {
    bytes.trim_ascii()
}

/// Whether the private copy of `file` in `ipc` differs from the real one. `None` when there is
/// no private copy we would touch (missing, a link, or too large).
fn is_stale(ipc: &Path, real_ipc: &Path, file: ConnFile) -> Option<bool> {
    let private = read_plain(&ipc.join(file.name()), MAX_CONNECTION_FILE_BYTES)?;
    let real = read_plain(&real_ipc.join(file.name()), MAX_CONNECTION_FILE_BYTES);
    Some(real.as_deref().map(trimmed) != Some(trimmed(&private)))
}

fn stale_files(ipc: &Path, real_ipc: &Path) -> Vec<ConnFile> {
    ConnFile::ALL
        .into_iter()
        .filter(|f| is_stale(ipc, real_ipc, *f) == Some(true))
        .collect()
}

fn read_marker(ipc: &Path) -> Option<FailureMarker> {
    hook_failure::parse(&read_plain(
        &ipc.join(hook_failure::MARKER_FILE),
        hook_failure::MAX_MARKER_BYTES,
    )?)
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
        let marker = read_marker(&ipc);
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
    /// Stale but couldn't be deleted, or no longer a plain file.
    pub(crate) failed: Vec<ConnFile>,
}

/// Deletes the stale connection files in `family`'s private copy, checking everything again
/// first: the family name, every directory on the way (no reparse points), that each file is
/// still a regular file, and that it still differs from the real one. Nothing else is touched.
pub(crate) fn fix(roots: &Roots, family: &str) -> Result<FixReport, &'static str> {
    PackageRef::new(family).ok_or("invalid_package")?;
    let ipc = package_ipc_dir(roots, family).ok_or("not_found")?;
    let mut report = FixReport::default();
    for file in ConnFile::ALL {
        let path = ipc.join(file.name());
        if fs::symlink_metadata(&path).is_err() {
            continue;
        }
        match is_stale(&ipc, &roots.real_ipc, file) {
            None => report.failed.push(file),
            Some(false) => report.current.push(file),
            Some(true) => {
                // Last look right before deleting: same path checks, still a plain file.
                let still_ours = package_ipc_dir(roots, family).as_deref() == Some(ipc.as_path())
                    && plain_file(&path, MAX_CONNECTION_FILE_BYTES);
                if still_ours && fs::remove_file(&path).is_ok() {
                    report.deleted.push(file);
                } else {
                    report.failed.push(file);
                }
            }
        }
    }
    Ok(report)
}

/// Whether a failure marker still means something: written during this run of the app (a
/// failure while Lullshift was closed is expected), not in the future, and after the last event
/// that adapter delivered.
pub(crate) fn marker_is_relevant(
    marker: &FailureMarker,
    app_started: OffsetDateTime,
    last_success: Option<OffsetDateTime>,
    now: OffsetDateTime,
) -> bool {
    marker.utc >= app_started.replace_nanosecond(0).unwrap_or(app_started)
        && marker.utc <= now + CLOCK_SLACK
        && last_success.is_none_or(|t| marker.utc > t)
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
    let relevant = |m: &FailureMarker| {
        marker_is_relevant(m, core.started_utc, last.get(m.adapter).copied(), now)
    };
    let mut failures: Vec<HookFailureView> = Vec::new();
    if let Some(m) = roots.and_then(|r| read_marker(&r.real_ipc))
        && relevant(&m)
    {
        failures.push(HookFailureView {
            adapter: m.adapter,
            reason: m.reason,
            utc: m.utc,
            package: None,
        });
    }
    for copy in &scanned.copies {
        if let Some(m) = copy.marker.as_ref().filter(|m| relevant(m)) {
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
    tauri::async_runtime::spawn_blocking(move || fix(&roots, &family).map_err(str::to_owned))
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
        ] {
            assert!(PackageRef::new(bad).is_none(), "{bad}");
        }
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
}
