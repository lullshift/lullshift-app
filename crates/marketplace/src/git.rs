//! The hardened `git` runner (ADR-015, TECHNICAL_ARCHITECTURE section 7.2).
//!
//! Every call is an argument vector (never a shell), with a fixed environment and a fixed set of
//! `-c` options that turn off hooks, filesystem monitors, LFS filters, submodule recursion and
//! every transport except https and ssh. Calls have a timeout that kills the whole process tree
//! and a cap on how much output they may produce. `git`'s stderr is only used to pick a
//! [`GitFailureKind`]; it is dropped afterwards because it can contain URLs and credentials.
//!
//! `file://` repositories are possible only in tests: [`Git::for_tests`] is `cfg(test)` here, and
//! for the desktop crate's tests it is also compiled when the `unsafe-test-file-urls` feature is
//! on in a debug build. The feature is requested only from a dev-dependency (resolver 3 does not
//! unify dev-dependency features into normal builds) and is additionally gated on
//! `debug_assertions`, so no release build contains it.

use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::error::{Error, GitFailureKind};

/// Time allowed for a call that talks to the network.
pub const NETWORK_TIMEOUT: Duration = Duration::from_secs(60);
/// Time allowed for a call that only touches the local cache.
pub const LOCAL_TIMEOUT: Duration = Duration::from_secs(10);
/// Output cap for small answers (`rev-parse`, `ls-tree` of one folder, ...).
pub const SMALL_OUTPUT: usize = 4 * 1024 * 1024;
/// Most a single fetch may add to a cache repository's object folder (review M2).
pub const FETCH_MAX_BYTES: u64 = 200 * 1024 * 1024;
const WATCH_EVERY: u32 = 10;
const STDERR_KEEP: usize = 16 * 1024;
const READER_GRACE: Duration = Duration::from_secs(2);

/// Environment variables removed before every call. They would point `git` somewhere other than
/// the cache repository, or inject configuration.
const SCRUBBED_ENV: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_NAMESPACE",
    "GIT_COMMON_DIR",
    "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG_COUNT",
    "GIT_EXTERNAL_DIFF",
    "GIT_TEMPLATE_DIR",
    // Would switch off certificate checks, replace the user configuration or the helper
    // programs, or write URLs and credentials into traces (review L2).
    "GIT_SSL_NO_VERIFY",
    "GIT_CURL_VERBOSE",
    "GIT_CONFIG_GLOBAL",
    "GIT_EXEC_PATH",
];

/// Every inherited variable starting with this is removed too (`GIT_TRACE`, `GIT_TRACE2_EVENT`,
/// `GIT_TRACE_PACKET`, `GIT_TRACE_CURL`, ...).
const SCRUBBED_PREFIX: &str = "GIT_TRACE";

/// The names to remove from a child's environment, given the names the process inherited.
fn scrub_list<I: IntoIterator<Item = OsString>>(inherited: I) -> Vec<OsString> {
    let mut out: Vec<OsString> = SCRUBBED_ENV.iter().map(OsString::from).collect();
    out.extend(inherited.into_iter().filter(|n| {
        // Windows environment names are case-insensitive.
        n.to_string_lossy()
            .to_ascii_uppercase()
            .starts_with(SCRUBBED_PREFIX)
    }));
    out
}

/// What kind of call, which decides the timeout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Class {
    Network,
    Local,
}

/// What a finished call returned. `stderr` stays inside the crate.
pub(crate) struct Output {
    pub code: i32,
    pub stdout: Vec<u8>,
    stderr: String,
}

impl Output {
    pub fn success(&self) -> bool {
        self.code == 0
    }

    /// The failure category for a non-zero exit.
    pub fn failure(&self) -> GitFailureKind {
        classify_failure(self.code, &self.stderr)
    }

    /// The raw stderr. Tests only; production code never sees it.
    #[cfg(test)]
    pub fn stderr_for_tests(&self) -> &str {
        &self.stderr
    }
}

/// A located `git` plus the settings every call shares.
#[derive(Debug, Clone)]
pub struct Git {
    exe: PathBuf,
    hooks_dir: PathBuf,
    allow_file: bool,
    network_timeout: Duration,
    local_timeout: Duration,
    fetch_cap: u64,
    fsck: bool,
    /// How host names are looked up before a fetch (the system resolver outside tests).
    resolve: crate::net::Resolve,
    /// Tests only: replaces the proxy lookup so the machine's own settings don't matter.
    proxy_override: Option<bool>,
}

impl Git {
    /// Finds `git` on `PATH` and prepares the empty hooks directory under `app_dir`
    /// (`<app_dir>/empty-hooks`; `core.hooksPath` points here, so no hook can run).
    pub fn new(app_dir: &Path) -> Result<Self, Error> {
        let exe = find_git().ok_or(GitFailureKind::GitMissing)?;
        Self::with_executable(exe, app_dir, false)
    }

    /// Like `new`, but with `file://` allowed. Exists only in this crate's tests and in debug
    /// builds that opt in with the `unsafe-test-file-urls` feature (a dev-dependency of the
    /// desktop crate); a release build cannot contain it.
    #[cfg(any(test, all(feature = "unsafe-test-file-urls", debug_assertions)))]
    pub fn for_tests(app_dir: &Path) -> Result<Self, Error> {
        let exe = find_git().ok_or(GitFailureKind::GitMissing)?;
        Self::with_executable(exe, app_dir, true)
    }

    fn with_executable(exe: PathBuf, app_dir: &Path, allow_file: bool) -> Result<Self, Error> {
        let hooks_dir = app_dir.join("empty-hooks");
        std::fs::create_dir_all(&hooks_dir).map_err(Error::io("create the hooks folder"))?;
        // Keep it empty: a file placed here by anything else would become a hook.
        let mut entries =
            std::fs::read_dir(&hooks_dir).map_err(Error::io("read the hooks folder"))?;
        if entries.next().is_some() {
            return Err(Error::Io {
                what: "use the hooks folder because it isn't empty",
                kind: std::io::ErrorKind::InvalidData,
            });
        }
        Ok(Self {
            exe,
            hooks_dir,
            allow_file,
            network_timeout: NETWORK_TIMEOUT,
            local_timeout: LOCAL_TIMEOUT,
            fetch_cap: FETCH_MAX_BYTES,
            fsck: true,
            resolve: crate::net::system_resolve,
            proxy_override: None,
        })
    }

    /// Replaces the name resolver and the proxy lookup. For tests.
    #[cfg(test)]
    pub(crate) fn with_network_for_tests(
        mut self,
        resolve: crate::net::Resolve,
        proxy: Option<bool>,
    ) -> Self {
        self.resolve = resolve;
        self.proxy_override = proxy;
        self
    }

    /// The resolver used to check a destination before a fetch.
    pub(crate) fn resolver(&self) -> crate::net::Resolve {
        self.resolve
    }

    /// True if an https fetch of `url` would go through a proxy the user configured: one of
    /// curl's proxy variables, or `http.proxy` (also per URL) in the user's global Git
    /// configuration (system configuration is off for every call). A proxy resolves the name
    /// itself, so the address check and the pin can't apply behind one (ADR-015 amendment).
    /// A failed lookup counts as "no proxy", which refuses rather than fetching unpinned.
    pub(crate) fn proxy_configured(&self, url: &crate::validate::GitUrl) -> bool {
        if let Some(answer) = self.proxy_override {
            return answer;
        }
        if crate::net::env_proxy_configured(|k| std::env::var_os(k)) {
            return true;
        }
        // `--global` so no repository found from the working folder can answer.
        self.run(
            None,
            &[
                "config",
                "--global",
                "--get-urlmatch",
                "http.proxy",
                url.as_str(),
            ],
            Class::Local,
            None,
            SMALL_OUTPUT,
        )
        .is_ok_and(|out| out.success() && !out.stdout.trim_ascii().is_empty())
    }

    /// Replaces the timeouts. For tests.
    #[cfg(test)]
    pub(crate) fn with_timeouts(mut self, network: Duration, local: Duration) -> Self {
        self.network_timeout = network;
        self.local_timeout = local;
        self
    }

    /// Turns object checking off, so a test can reach the later defences with a tree that Git's
    /// own check would already refuse. For tests.
    #[cfg(any(test, all(feature = "unsafe-test-file-urls", debug_assertions)))]
    pub fn without_fsck_for_tests(mut self) -> Self {
        self.fsck = false;
        self
    }

    /// Replaces the fetch size cap. For tests.
    #[cfg(test)]
    pub(crate) fn with_fetch_cap(mut self, cap: u64) -> Self {
        self.fetch_cap = cap;
        self
    }

    fn timeout(&self, class: Class) -> Duration {
        match class {
            Class::Network => self.network_timeout,
            Class::Local => self.local_timeout,
        }
    }

    /// The path of the `git` executable in use.
    pub fn executable(&self) -> &Path {
        &self.exe
    }

    /// The `-c key=value` options every call carries. The first part is ADR-015 verbatim; the
    /// last two stop automatic maintenance from deleting commits that installs are pinned to.
    fn config(&self) -> Vec<OsString> {
        let mut pairs: Vec<OsString> = vec![
            {
                let mut s = OsString::from("core.hooksPath=");
                s.push(&self.hooks_dir);
                s
            },
            "core.fsmonitor=false".into(),
            "protocol.allow=never".into(),
            "protocol.https.allow=always".into(),
            "protocol.ssh.allow=always".into(),
            "submodule.recurse=false".into(),
            "core.symlinks=false".into(),
            "core.longpaths=true".into(),
            "filter.lfs.required=false".into(),
            "filter.lfs.process=".into(),
            "filter.lfs.smudge=".into(),
            "filter.lfs.clean=".into(),
            // No redirects: a repository address must not hand us to another host (review L1).
            "http.followRedirects=false".into(),
            "http.sslVerify=true".into(),
            "gc.auto=0".into(),
            "maintenance.auto=false".into(),
        ];
        if self.fsck {
            // Objects from a repository are checked as they arrive (review L3).
            pairs.push("transfer.fsckObjects=true".into());
            pairs.push("fetch.fsckObjects=true".into());
        }
        if self.allow_file {
            pairs.push("protocol.file.allow=always".into());
        }
        pairs
            .into_iter()
            .flat_map(|p| [OsString::from("-c"), p])
            .collect()
    }

    fn command(&self, git_dir: Option<&Path>, args: &[&str]) -> Command {
        self.command_with(&[], git_dir, args)
    }

    /// Like `command`, plus extra `-c` settings after the fixed ones. Only fixed keys built by
    /// this crate are passed here (the resolution pin), never text from a repository.
    fn command_with(&self, extra: &[String], git_dir: Option<&Path>, args: &[&str]) -> Command {
        let mut cmd = Command::new(&self.exe);
        cmd.args(self.config());
        for setting in extra {
            cmd.arg("-c").arg(setting);
        }
        if let Some(dir) = git_dir {
            cmd.arg("--git-dir").arg(dir);
        }
        cmd.args(args);
        for name in scrub_list(std::env::vars_os().map(|(k, _)| k)) {
            cmd.env_remove(name);
        }
        cmd.env(
            "GIT_ALLOW_PROTOCOL",
            if self.allow_file {
                "file:https:ssh"
            } else {
                "https:ssh"
            },
        )
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_LFS_SKIP_SMUDGE", "1")
        .env("GIT_NO_LAZY_FETCH", "1")
        // So stderr is English and can be classified.
        .env("LC_ALL", "C")
        .env("LANGUAGE", "C");
        // GIT_ASKPASS, SSH_ASKPASS, GCM_INTERACTIVE and GIT_SSH_COMMAND are the user's: they are
        // how credential managers and keys work. They are inherited untouched.
        cmd.current_dir(&self.hooks_dir);
        cmd
    }

    /// Runs `git [-c ...] [--git-dir D] args...`. Never uses a shell. `args` must already have
    /// `--end-of-options` (or `--`) before any URL, ref or path.
    pub(crate) fn run(
        &self,
        git_dir: Option<&Path>,
        args: &[&str],
        class: Class,
        stdin: Option<Vec<u8>>,
        max_output: usize,
    ) -> Result<Output, Error> {
        let timeout = self.timeout(class);
        let mut cmd = self.command(git_dir, args);
        run_process(&mut cmd, stdin, timeout, max_output, None)
    }

    /// Runs a network call that writes into the cache repository `repo`, and stops it (killing
    /// the whole process tree) if `repo`'s object folder grows by more than the fetch cap
    /// (review M2). The size is polled while it runs and checked once more when it ends.
    pub(crate) fn run_fetch(
        &self,
        repo: &Path,
        args: &[&str],
        max_output: usize,
        resolve_pin: Option<&str>,
    ) -> Result<Output, Error> {
        let objects = repo.join("objects");
        let watch = Watch {
            baseline: dir_size(&objects),
            dir: objects,
            cap: self.fetch_cap,
        };
        // `http.curloptResolve` (Git 2.37+) makes curl use the addresses Lullshift checked
        // instead of resolving the name again (decision 18.8). Older Git ignores the key.
        let extra: Vec<String> = resolve_pin
            .map(|pin| format!("http.curloptResolve={pin}"))
            .into_iter()
            .collect();
        let mut cmd = self.command_with(&extra, Some(repo), args);
        run_process(
            &mut cmd,
            None,
            self.timeout(Class::Network),
            max_output,
            Some(&watch),
        )
    }

    /// Like [`Git::run`], but a non-zero exit is an error with an honest category.
    pub(crate) fn run_ok(
        &self,
        git_dir: Option<&Path>,
        args: &[&str],
        class: Class,
        stdin: Option<Vec<u8>>,
        max_output: usize,
    ) -> Result<Vec<u8>, Error> {
        let out = self.run(git_dir, args, class, stdin, max_output)?;
        if out.success() {
            Ok(out.stdout)
        } else {
            Err(out.failure().into())
        }
    }

    /// Runs `git` with arbitrary arguments in the hardened environment. For tests only.
    #[cfg(test)]
    pub(crate) fn run_raw(&self, args: &[&str], class: Class) -> Result<Output, Error> {
        self.run(None, args, class, None, SMALL_OUTPUT)
    }
}

/// Looks for `git` in the absolute entries of `PATH`.
fn find_git() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let name = if cfg!(windows) { "git.exe" } else { "git" };
    std::env::split_paths(&path)
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// Tests only: which `git` subcommands this thread started, so a test can prove a refused fetch
/// never ran `git fetch`. Each test runs on its own thread, so they don't see each other.
#[cfg(test)]
pub(crate) mod spawned {
    use std::cell::RefCell;
    use std::process::Command;

    thread_local! {
        static LOG: RefCell<Vec<Vec<String>>> = const { RefCell::new(Vec::new()) };
    }

    pub(super) fn record(cmd: &Command) {
        let args = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        LOG.with(|l| l.borrow_mut().push(args));
    }

    /// Clears this thread's log.
    pub fn reset() {
        LOG.with(|l| l.borrow_mut().clear());
    }

    /// True if any call this thread started since [`reset`] had `word` as an argument.
    pub fn any_with(word: &str) -> bool {
        LOG.with(|l| l.borrow().iter().any(|args| args.iter().any(|a| a == word)))
    }
}

fn run_process(
    cmd: &mut Command,
    stdin: Option<Vec<u8>>,
    timeout: Duration,
    max_output: usize,
    watch: Option<&Watch>,
) -> Result<Output, Error> {
    cmd.stdin(if stdin.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    })
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: no console flashes when the GUI app runs git.
        cmd.creation_flags(0x0800_0000);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Its own process group, so the whole tree can be killed.
        cmd.process_group(0);
    }
    #[cfg(test)]
    spawned::record(cmd);
    let mut child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            Error::Git(GitFailureKind::GitMissing)
        } else {
            Error::Io {
                what: "start git",
                kind: e.kind(),
            }
        }
    })?;

    if let (Some(data), Some(mut pipe)) = (stdin, child.stdin.take()) {
        std::thread::spawn(move || {
            // A broken pipe just means git exited or was killed.
            let _ = pipe.write_all(&data);
        });
    }
    let out = Capture::start(child.stdout.take(), max_output);
    let err = Capture::start(child.stderr.take(), STDERR_KEEP);

    let started = Instant::now();
    let mut ticks = 0_u32;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(e) => {
                kill_tree(&mut child);
                return Err(Error::Io {
                    what: "wait for git",
                    kind: e.kind(),
                });
            }
        }
        if out.overflowed.load(Ordering::Relaxed) {
            kill_tree(&mut child);
            return Err(GitFailureKind::OutputTooLarge.into());
        }
        if started.elapsed() >= timeout {
            kill_tree(&mut child);
            return Err(GitFailureKind::Timeout.into());
        }
        ticks = ticks.wrapping_add(1);
        if ticks.is_multiple_of(WATCH_EVERY) && watch.is_some_and(Watch::exceeded) {
            kill_tree(&mut child);
            return Err(GitFailureKind::OutputTooLarge.into());
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    out.wait(READER_GRACE);
    err.wait(READER_GRACE);
    if watch.is_some_and(Watch::exceeded) {
        return Err(GitFailureKind::OutputTooLarge.into());
    }
    if out.overflowed.load(Ordering::Relaxed) {
        return Err(GitFailureKind::OutputTooLarge.into());
    }
    Ok(Output {
        code: status.code().unwrap_or(-1),
        stdout: out.take(),
        stderr: String::from_utf8_lossy(&err.take()).into_owned(),
    })
}

/// A folder whose growth is limited while a call runs.
struct Watch {
    dir: PathBuf,
    baseline: u64,
    cap: u64,
}

impl Watch {
    fn exceeded(&self) -> bool {
        dir_size(&self.dir).saturating_sub(self.baseline) > self.cap
    }
}

/// Total size of the files under `dir`. Links are not followed; unreadable entries count as 0.
fn dir_size(dir: &Path) -> u64 {
    let Ok(read) = std::fs::read_dir(dir) else {
        return 0;
    };
    read.flatten()
        .map(|e| match std::fs::symlink_metadata(e.path()) {
            Ok(m) if m.is_dir() => dir_size(&e.path()),
            Ok(m) if m.is_file() => m.len(),
            _ => 0,
        })
        .sum()
}

/// Kills `child` and everything it started, then reaps it.
fn kill_tree(child: &mut Child) {
    let pid = child.id().to_string();
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = Command::new("taskkill")
            .args(["/PID", &pid, "/T", "/F"])
            .creation_flags(0x0800_0000)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(unix)]
    {
        let _ = Command::new("kill")
            .args(["-KILL", "--", &format!("-{pid}")])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Reads a pipe on a thread into a buffer of at most `cap` bytes. Further bytes are dropped
/// (stderr) or flagged (stdout: `overflowed`).
struct Capture {
    buf: Arc<Mutex<Vec<u8>>>,
    done: Arc<AtomicBool>,
    overflowed: Arc<AtomicBool>,
}

impl Capture {
    fn start<R: Read + Send + 'static>(pipe: Option<R>, cap: usize) -> Self {
        let buf = Arc::new(Mutex::new(Vec::new()));
        let done = Arc::new(AtomicBool::new(false));
        let overflowed = Arc::new(AtomicBool::new(false));
        let (b, d, o) = (buf.clone(), done.clone(), overflowed.clone());
        // stdout overflow is an error; stderr overflow is silently truncated.
        let flag_overflow = cap != STDERR_KEEP;
        std::thread::spawn(move || {
            if let Some(mut pipe) = pipe {
                let mut chunk = [0u8; 16 * 1024];
                while let Ok(n) = pipe.read(&mut chunk) {
                    if n == 0 {
                        break;
                    }
                    let mut guard = b.lock().unwrap_or_else(|e| e.into_inner());
                    let room = cap.saturating_sub(guard.len());
                    guard.extend_from_slice(&chunk[..n.min(room)]);
                    if n > room && flag_overflow {
                        o.store(true, Ordering::Relaxed);
                        break;
                    }
                }
            }
            d.store(true, Ordering::Relaxed);
        });
        Self {
            buf,
            done,
            overflowed,
        }
    }

    /// Waits for the reader to see end-of-file, up to `grace`. A background process that kept
    /// the pipe open must not hang us.
    fn wait(&self, grace: Duration) {
        let start = Instant::now();
        while !self.done.load(Ordering::Relaxed) && start.elapsed() < grace {
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn take(&self) -> Vec<u8> {
        std::mem::take(&mut *self.buf.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

/// Picks an honest category from a failed call's exit code and stderr. The text is matched,
/// never kept or shown.
pub fn classify_failure(code: i32, stderr: &str) -> GitFailureKind {
    let _ = code;
    let s = stderr.to_ascii_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|n| s.contains(n));
    if has(&[
        "authentication failed",
        "terminal prompts disabled",
        "could not read username",
        "could not read password",
        "permission denied",
        "access denied",
        "host key verification failed",
        "returned error: 401",
        "returned error: 403",
        "http 401",
        "http 403",
        "invalid credentials",
        "bad credentials",
    ]) {
        return GitFailureKind::AuthDenied;
    }
    if has(&[
        "could not resolve host",
        "could not resolve hostname",
        "name or service not known",
        "temporary failure in name resolution",
        "failed to connect",
        "couldn't connect to server",
        "connection refused",
        "connection timed out",
        "connection reset",
        "network is unreachable",
        "no route to host",
        "unable to connect",
        "operation timed out",
        "ssl connect error",
        "could not connect",
    ]) {
        return GitFailureKind::Offline;
    }
    if has(&[
        "repository not found",
        "returned error: 404",
        "http 404",
        "does not appear to be a git repository",
        "couldn't find remote ref",
        "could not find remote branch",
        "not our ref",
        "no such remote ref",
        "invalid refspec",
    ]) {
        return GitFailureKind::NotFound;
    }
    // Git says this for both a private repository and a missing one; do not guess "missing".
    if has(&["could not read from remote repository", "publickey"]) {
        return GitFailureKind::AuthDenied;
    }
    if has(&[
        "remote branch",
        "unknown revision",
        "bad revision",
        "pathspec",
        "does not exist",
        "not found",
    ]) {
        return GitFailureKind::NotFound;
    }
    GitFailureKind::Failed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stderr_maps_to_honest_categories() {
        use GitFailureKind::*;
        for (stderr, want) in [
            (
                "fatal: unable to access 'https://x.example/o/r/': Could not resolve host: x.example",
                Offline,
            ),
            (
                "fatal: unable to access 'https://x.example/': Failed to connect to x.example port 443 after 21000 ms: Couldn't connect to server",
                Offline,
            ),
            (
                "ssh: connect to host x.example port 22: Network is unreachable",
                Offline,
            ),
            (
                "ssh: connect to host x.example port 22: Connection timed out",
                Offline,
            ),
            (
                "fatal: Authentication failed for 'https://x.example/o/r/'",
                AuthDenied,
            ),
            (
                "fatal: could not read Username for 'https://x.example': terminal prompts disabled",
                AuthDenied,
            ),
            (
                "git@x.example: Permission denied (publickey).\nfatal: Could not read from remote repository.",
                AuthDenied,
            ),
            (
                "fatal: unable to access 'https://x.example/o/r/': The requested URL returned error: 403",
                AuthDenied,
            ),
            ("Host key verification failed.", AuthDenied),
            (
                "remote: Repository not found.\nfatal: repository 'https://x.example/o/r/' not found",
                NotFound,
            ),
            (
                "fatal: unable to access 'https://x.example/o/r/': The requested URL returned error: 404",
                NotFound,
            ),
            ("fatal: couldn't find remote ref nope", NotFound),
            (
                "fatal: 'https://x.example/o/r' does not appear to be a git repository",
                NotFound,
            ),
            ("fatal: something odd", Failed),
            ("", Failed),
        ] {
            assert_eq!(classify_failure(128, stderr), want, "{stderr}");
        }
    }

    #[test]
    fn config_matches_adr_015() {
        let dir = tempfile::tempdir().expect("tmp");
        let git = Git::new(dir.path()).expect("git on PATH");
        let args: Vec<String> = git
            .config()
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let joined = args.join(" ");
        for must in [
            "-c core.fsmonitor=false",
            "-c protocol.allow=never",
            "-c protocol.https.allow=always",
            "-c protocol.ssh.allow=always",
            "-c submodule.recurse=false",
            "-c core.symlinks=false",
            "-c core.longpaths=true",
            "-c filter.lfs.required=false",
            "-c filter.lfs.process=",
            "-c filter.lfs.smudge=",
            "-c filter.lfs.clean=",
            "-c http.followRedirects=false",
            "-c http.sslVerify=true",
            "-c transfer.fsckObjects=true",
            "-c fetch.fsckObjects=true",
            "empty-hooks",
        ] {
            assert!(joined.contains(must), "{must} missing from {joined}");
        }
        assert!(
            !joined.contains("protocol.file"),
            "production runner must not allow file://"
        );
        assert!(!git.allow_file);
    }

    #[test]
    fn inherited_tracing_and_tls_overrides_are_scrubbed() {
        let names = scrub_list(
            [
                "GIT_TRACE",
                "GIT_TRACE2",
                "git_trace2_event",
                "GIT_TRACE_PACKET",
                "GIT_TRACE_PERFORMANCE",
                "GIT_TRACE_SETUP",
                "GIT_TRACE_CURL",
                "PATH",
                "GIT_ASKPASS",
            ]
            .map(OsString::from),
        );
        let has = |n: &str| names.iter().any(|x| x == n);
        for must in [
            "GIT_SSL_NO_VERIFY",
            "GIT_CURL_VERBOSE",
            "GIT_CONFIG_GLOBAL",
            "GIT_EXEC_PATH",
            "GIT_DIR",
            "GIT_TRACE",
            "GIT_TRACE2",
            "git_trace2_event",
            "GIT_TRACE_PACKET",
            "GIT_TRACE_PERFORMANCE",
            "GIT_TRACE_SETUP",
            "GIT_TRACE_CURL",
        ] {
            assert!(has(must), "{must} not scrubbed");
        }
        assert!(!has("PATH"), "unrelated variables stay");
        assert!(!has("GIT_ASKPASS"), "the user's credential helpers stay");

        // And the child command really carries the removals.
        let dir = tempfile::tempdir().expect("tmp");
        let git = Git::new(dir.path()).expect("git on PATH");
        let cmd = git.command(None, &["version"]);
        for name in [
            "GIT_SSL_NO_VERIFY",
            "GIT_CONFIG_GLOBAL",
            "GIT_EXEC_PATH",
            "GIT_CURL_VERBOSE",
        ] {
            assert!(
                cmd.get_envs().any(|(k, v)| k == name && v.is_none()),
                "{name} is not removed from the child"
            );
        }
    }

    #[test]
    fn config_without_fsck_is_only_for_tests() {
        let dir = tempfile::tempdir().expect("tmp");
        let lenient = Git::for_tests(dir.path())
            .expect("git")
            .without_fsck_for_tests();
        let joined = lenient
            .config()
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(!joined.contains("fsckObjects"));
        let strict = Git::for_tests(dir.path()).expect("git");
        assert!(strict.fsck, "the default runner checks objects");
    }
    #[test]
    fn finds_git_and_reports_missing() {
        assert!(find_git().is_some(), "these tests need git on PATH");
        let dir = tempfile::tempdir().expect("tmp");
        let missing = Git::with_executable(dir.path().join("no-such-git.exe"), dir.path(), false)
            .expect("construct");
        let err = missing.run_raw(&["--version"], Class::Local).err();
        assert_eq!(err, Some(Error::Git(GitFailureKind::GitMissing)));
    }

    #[test]
    fn timeout_kills_the_call() {
        let dir = tempfile::tempdir().expect("tmp");
        let git = Git::for_tests(dir.path())
            .expect("git")
            .with_timeouts(Duration::from_millis(300), Duration::from_millis(300));
        let started = Instant::now();
        // An alias runs through git's own shell; `sleep` is a grandchild process.
        let err = git
            .run_raw(&["-c", "alias.hang=!sleep 30", "hang"], Class::Local)
            .err();
        assert_eq!(err, Some(Error::Git(GitFailureKind::Timeout)));
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "returned promptly"
        );
    }

    #[test]
    fn output_cap_stops_the_call() {
        let dir = tempfile::tempdir().expect("tmp");
        let git = Git::for_tests(dir.path()).expect("git");
        let err = git.run(None, &["--exec-path"], Class::Local, None, 3).err();
        assert_eq!(err, Some(Error::Git(GitFailureKind::OutputTooLarge)));
    }

    #[test]
    fn hooks_dir_must_stay_empty() {
        let dir = tempfile::tempdir().expect("tmp");
        let git = Git::new(dir.path()).expect("git");
        std::fs::write(
            dir.path().join("empty-hooks").join("post-checkout"),
            b"#!/bin/sh\n",
        )
        .expect("write");
        let again = Git::with_executable(git.exe.clone(), dir.path(), false);
        assert!(again.is_err(), "a non-empty hooks folder is refused");
    }
}
