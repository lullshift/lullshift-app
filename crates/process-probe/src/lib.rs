//! Read-only process lookups (owner decision 35, ADR-025).
//!
//! Two questions, both answered without ever signalling, killing or otherwise acting on a
//! process:
//!
//! 1. **Hook side:** which process is the agent behind this hook? [`agent_process`] walks up from
//!    the hook's own process, skipping shells (`cmd`, `powershell`, `bash`, ...), and returns the
//!    first other ancestor together with its start key. The walk is bounded ([`MAX_DEPTH`]) and
//!    gives up (`None`) on anything odd: a missing parent, a parent that started after its child
//!    (its PID was reused), a loop.
//! 2. **App side:** is a recorded process still the same one? [`check`] reads the process by PID
//!    and returns [`Probe::Gone`] only when the operating system says no such process exists (or
//!    it is a zombie), [`Probe::Running`] with the current start key otherwise, and
//!    [`Probe::Unknown`] whenever it could not tell (access denied, `/proc` unreadable, `ps`
//!    failed). The caller compares start keys, so a reused PID is never mistaken for the agent.
//!
//! A start key is opaque: only equality means anything. It is the creation time on Windows, the
//! boot id plus start tick on Linux, and a hash of `ps`'s start time on macOS and other Unix.
//!
//! Every lookup is cheap (a few system calls) and failure-tolerant: errors become `None` or
//! `Unknown`, never a panic.

#![deny(clippy::undocumented_unsafe_blocks, unsafe_op_in_unsafe_fn)]

#[cfg(target_os = "linux")]
mod linux;
mod parse;
#[cfg(all(unix, not(target_os = "linux")))]
mod ps;
#[cfg(windows)]
mod windows;

/// Most ancestors the hook walk looks at.
pub const MAX_DEPTH: usize = 8;

/// The largest start key stored: keeps it inside a signed 63-bit database integer.
pub const MAX_START_KEY: u64 = (1 << 62) - 1;

/// A process and the moment it started, which together tell it apart from a later process that
/// reuses the same PID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProcessId {
    pub pid: u32,
    /// Opaque start key; never zero, never above [`MAX_START_KEY`].
    pub start: u64,
}

/// What the walk and the lookups know about one process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Info {
    pub id: ProcessId,
    pub parent: Option<u32>,
    /// Executable name as the system reports it (use [`is_shell`], which strips directories
    /// and a `.exe` suffix).
    pub name: String,
}

/// Result of looking at one PID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Probe {
    /// The process exists, with this start key.
    Running(ProcessId),
    /// The system says there is no such process (or it has exited and awaits cleanup).
    Gone,
    /// Could not tell. Callers must treat this as "no change".
    Unknown,
}

/// The largest PID the system can hand out (Windows: 32 bit; Unix: a signed `pid_t`).
pub const fn max_pid() -> u32 {
    if cfg!(windows) {
        u32::MAX - 1
    } else {
        i32::MAX as u32
    }
}

/// A PID worth looking at: not 0 and within the system's range. A PID that comes from outside
/// (the hook pipe) is checked with this before anything uses it.
pub fn valid_pid(pid: u32) -> bool {
    (1..=max_pid()).contains(&pid)
}

/// A start key worth storing.
pub fn valid_start(start: u64) -> bool {
    (1..=MAX_START_KEY).contains(&start)
}

/// Whether an executable name is a command shell, which the hook walk looks through. The name
/// may carry a directory and a `.exe` suffix; case is ignored.
pub fn is_shell(name: &str) -> bool {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let lower = base.to_ascii_lowercase();
    let stem = lower.strip_suffix(".exe").unwrap_or(&lower);
    // A login shell shows up as `-bash`.
    let stem = stem.strip_prefix('-').unwrap_or(stem);
    matches!(
        stem,
        "sh" | "bash"
            | "zsh"
            | "fish"
            | "dash"
            | "ash"
            | "ksh"
            | "mksh"
            | "csh"
            | "tcsh"
            | "cmd"
            | "powershell"
            | "pwsh"
            | "nu"
            | "xonsh"
    )
}

/// Walks up from `start_pid` and returns the first ancestor that is not a shell.
///
/// `lookup` returns what is known about a PID, or `None` when it cannot be read. The walk
/// returns `None` (unknown, never a guess) when a lookup fails, when a parent started after its
/// child (the parent PID was reused, so it is not the real parent), when a process is its own
/// parent, or when more than [`MAX_DEPTH`] shells are stacked.
pub fn first_non_shell_ancestor(
    start_pid: u32,
    mut lookup: impl FnMut(u32) -> Option<Info>,
) -> Option<ProcessId> {
    let mut current = lookup(start_pid)?;
    for _ in 0..MAX_DEPTH {
        let parent_pid = current.parent?;
        if !valid_pid(parent_pid) || parent_pid == current.id.pid {
            return None;
        }
        let parent = lookup(parent_pid)?;
        if parent.id.start > current.id.start {
            return None;
        }
        if !is_shell(&parent.name) {
            return Some(parent.id);
        }
        current = parent;
    }
    None
}

/// The agent process behind the running hook: the first non-shell ancestor of this process.
/// `None` when it can't be told.
pub fn agent_process() -> Option<ProcessId> {
    agent_process_impl()
}

/// Looks at one PID. See [`Probe`].
pub fn check(pid: u32) -> Probe {
    if !valid_pid(pid) {
        return Probe::Unknown;
    }
    check_impl(pid)
}

#[cfg(windows)]
fn agent_process_impl() -> Option<ProcessId> {
    windows::agent_process()
}
#[cfg(windows)]
fn check_impl(pid: u32) -> Probe {
    windows::check(pid)
}

#[cfg(target_os = "linux")]
fn agent_process_impl() -> Option<ProcessId> {
    first_non_shell_ancestor(std::process::id(), linux::info)
}
#[cfg(target_os = "linux")]
fn check_impl(pid: u32) -> Probe {
    linux::check(pid)
}

#[cfg(all(unix, not(target_os = "linux")))]
fn agent_process_impl() -> Option<ProcessId> {
    first_non_shell_ancestor(std::process::id(), ps::info)
}
#[cfg(all(unix, not(target_os = "linux")))]
fn check_impl(pid: u32) -> Probe {
    ps::check(pid)
}

#[cfg(not(any(unix, windows)))]
fn agent_process_impl() -> Option<ProcessId> {
    None
}
#[cfg(not(any(unix, windows)))]
fn check_impl(_pid: u32) -> Probe {
    Probe::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn info(pid: u32, parent: Option<u32>, start: u64, name: &str) -> Info {
        Info {
            id: ProcessId { pid, start },
            parent,
            name: name.to_owned(),
        }
    }

    fn table(rows: &[Info]) -> impl FnMut(u32) -> Option<Info> {
        let map: HashMap<u32, Info> = rows.iter().map(|i| (i.id.pid, i.clone())).collect();
        move |pid| map.get(&pid).cloned()
    }

    #[test]
    fn shell_names_ignore_case_directory_suffix_and_login_dash() {
        for name in [
            "cmd.exe",
            "CMD.EXE",
            "PowerShell.exe",
            "pwsh",
            r"C:\Program Files\Git\bin\bash.exe",
            "/bin/zsh",
            "-bash",
            "sh",
            "fish",
        ] {
            assert!(is_shell(name), "{name}");
        }
        for name in [
            "node.exe",
            "claude.exe",
            "claude",
            "codex",
            "Cursor.exe",
            "bash-completion",
            "shell.exe",
            "",
            "cmd.exe.bak",
        ] {
            assert!(!is_shell(name), "{name}");
        }
    }

    #[test]
    fn the_direct_parent_is_the_agent_when_it_is_not_a_shell() {
        let rows = [
            info(10, Some(5), 300, "lullshift-desktop.exe"),
            info(5, Some(1), 200, "claude.exe"),
        ];
        assert_eq!(
            first_non_shell_ancestor(10, table(&rows)),
            Some(ProcessId { pid: 5, start: 200 })
        );
    }

    #[test]
    fn shells_between_the_hook_and_the_agent_are_skipped() {
        // hook -> cmd -> bash -> pwsh -> agent -> terminal
        let rows = [
            info(10, Some(9), 600, "lullshift-desktop.exe"),
            info(9, Some(8), 500, "cmd.exe"),
            info(8, Some(7), 400, "bash.exe"),
            info(7, Some(6), 300, "pwsh.exe"),
            info(6, Some(1), 200, "node.exe"),
            info(1, None, 100, "WindowsTerminal.exe"),
        ];
        assert_eq!(
            first_non_shell_ancestor(10, table(&rows)),
            Some(ProcessId { pid: 6, start: 200 })
        );
    }

    #[test]
    fn too_many_shells_is_unknown_not_a_guess() {
        let mut rows = vec![info(100, Some(99), 10_000, "hook")];
        for i in 0..(MAX_DEPTH as u32 + 2) {
            rows.push(info(99 - i, Some(98 - i), 9_000 - u64::from(i), "sh"));
        }
        assert_eq!(first_non_shell_ancestor(100, table(&rows)), None);
    }

    #[test]
    fn a_parent_that_started_after_its_child_is_a_reused_pid() {
        let rows = [
            info(10, Some(5), 300, "lullshift-desktop.exe"),
            // PID 5 now belongs to a process created after the hook started.
            info(5, Some(1), 900, "claude.exe"),
        ];
        assert_eq!(first_non_shell_ancestor(10, table(&rows)), None);
    }

    #[test]
    fn a_missing_parent_a_loop_and_a_bad_pid_are_unknown() {
        let rows = [info(10, Some(5), 300, "hook")];
        assert_eq!(
            first_non_shell_ancestor(10, table(&rows)),
            None,
            "no parent row"
        );
        let rows = [info(10, Some(10), 300, "hook")];
        assert_eq!(
            first_non_shell_ancestor(10, table(&rows)),
            None,
            "own parent"
        );
        let rows = [info(10, Some(0), 300, "hook")];
        assert_eq!(first_non_shell_ancestor(10, table(&rows)), None, "pid 0");
        let rows = [info(10, None, 300, "hook")];
        assert_eq!(
            first_non_shell_ancestor(10, table(&rows)),
            None,
            "no parent id"
        );
        assert_eq!(
            first_non_shell_ancestor(77, table(&[])),
            None,
            "self unreadable"
        );
    }

    #[test]
    fn pids_and_start_keys_are_range_checked() {
        assert!(!valid_pid(0));
        assert!(valid_pid(1));
        assert!(valid_pid(4242));
        assert!(!valid_pid(u32::MAX));
        assert!(!valid_start(0));
        assert!(valid_start(1));
        assert!(valid_start(MAX_START_KEY));
        assert!(!valid_start(MAX_START_KEY + 1));
        assert_eq!(check(0), Probe::Unknown);
    }

    fn long_running_child() -> std::process::Child {
        let mut cmd = if cfg!(windows) {
            let mut c = std::process::Command::new("ping");
            c.args(["-n", "60", "127.0.0.1"]);
            c
        } else {
            let mut c = std::process::Command::new("sleep");
            c.arg("60");
            c
        };
        cmd.stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .stdin(std::process::Stdio::null())
            .spawn()
            .expect("spawn a long-running child")
    }

    /// A real process: seen as running with a stable start key, then gone once it has exited.
    #[test]
    fn a_real_child_is_running_then_gone() {
        let mut child = long_running_child();
        let pid = child.id();
        let Probe::Running(first) = check(pid) else {
            panic!("the child is running");
        };
        assert_eq!(first.pid, pid);
        assert!(valid_start(first.start));
        assert_eq!(check(pid), Probe::Running(first), "stable start key");
        child.kill().expect("kill our own child");
        child.wait().expect("wait");
        // A killed child may take a moment to vanish from the process table.
        let mut gone = false;
        for _ in 0..50 {
            if check(pid) == Probe::Gone {
                gone = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        assert!(gone, "the exited child is reported gone");
    }

    #[test]
    fn this_process_is_running_with_a_valid_key() {
        match check(std::process::id()) {
            Probe::Running(id) => assert!(valid_start(id.start)),
            other => panic!("expected running, got {other:?}"),
        }
    }

    #[test]
    fn the_hook_walk_finds_an_older_non_shell_ancestor_or_nothing() {
        let me = std::process::id();
        let Probe::Running(mine) = check(me) else {
            panic!("self is running");
        };
        if let Some(agent) = agent_process() {
            assert_ne!(agent.pid, me);
            assert!(agent.start <= mine.start, "an ancestor is never younger");
            assert!(valid_pid(agent.pid) && valid_start(agent.start));
        }
    }
}
