//! Linux: `/proc`. Safe code only.

use std::io::ErrorKind;

use crate::parse::{ProcStat, key_from, parse_proc_stat};
use crate::{Info, Probe, ProcessId};

/// The boot id, or `None` if it can't be read: a start key without it could collide across a
/// reboot, so the process is then unknown rather than keyed.
fn boot_id() -> Option<String> {
    std::fs::read_to_string("/proc/sys/kernel/random/boot_id")
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

/// The inode of this process's PID namespace. Two processes see the same PIDs only if it is
/// equal.
pub(crate) fn pid_namespace() -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata("/proc/self/ns/pid")
        .ok()
        .map(|m| m.ino())
        .filter(|i| crate::valid_start(*i))
}

fn start_key(ticks: u64, boot: &str) -> u64 {
    // The boot id is part of the key: after a reboot the same PID can start at the same tick.
    key_from(&[boot.as_bytes(), ticks.to_string().as_bytes()])
}

fn read_stat(pid: u32) -> Result<ProcStat, Probe> {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(text) => parse_proc_stat(&text).ok_or(Probe::Unknown),
        Err(e) if e.kind() == ErrorKind::NotFound => {
            // Gone only if /proc itself works (a missing /proc says nothing).
            if std::fs::metadata("/proc/self/stat").is_ok() {
                Err(Probe::Gone)
            } else {
                Err(Probe::Unknown)
            }
        }
        Err(_) => Err(Probe::Unknown),
    }
}

pub(crate) fn check(pid: u32) -> Probe {
    match read_stat(pid) {
        Ok(s) if s.state == 'Z' || s.state == 'X' => Probe::Gone,
        Ok(s) => match boot_id() {
            Some(boot) => Probe::Running(ProcessId {
                pid,
                start: start_key(s.start_ticks, &boot),
            }),
            None => Probe::Unknown,
        },
        Err(p) => p,
    }
}

pub(crate) fn info(pid: u32) -> Option<Info> {
    let s = read_stat(pid).ok()?;
    if s.state == 'Z' || s.state == 'X' {
        return None;
    }
    let name = std::fs::read_to_string(format!("/proc/{pid}/comm"))
        .ok()?
        .trim()
        .to_owned();
    let boot = boot_id()?;
    Some(Info {
        id: ProcessId {
            pid,
            start: start_key(s.start_ticks, &boot),
        },
        parent: (s.ppid != 0).then_some(s.ppid),
        name,
    })
}
