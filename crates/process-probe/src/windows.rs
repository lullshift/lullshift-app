//! Windows: Toolhelp snapshot for parent PIDs and names, `OpenProcess` +
//! `GetProcessTimes` for the start time. Every `unsafe` block is one Win32 call (or a `Drop`
//! that closes a handle), checked against the Win32 docs and the `windows-sys` 0.61.2 bindings.
//! Processes are opened with `PROCESS_QUERY_LIMITED_INFORMATION` only: no terminate, no read or
//! write of memory.

use std::collections::HashMap;

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_INVALID_PARAMETER, FILETIME, GetLastError, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Threading::{
    GetExitCodeProcess, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};

use crate::{Info, Probe, ProcessId, first_non_shell_ancestor, valid_start};

/// `GetExitCodeProcess` reports this while the process is still running.
const STILL_ACTIVE: u32 = 259;
/// Rows read from a snapshot, at most (a real machine has a few hundred).
const MAX_ROWS: usize = 200_000;

/// A handle closed on drop.
struct Owned(HANDLE);

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: the handle was returned by a successful OpenProcess or
        // CreateToolhelp32Snapshot call, is owned by this value alone and is closed once.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

fn filetime_u64(t: FILETIME) -> u64 {
    (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime)
}

pub(crate) fn check(pid: u32) -> Probe {
    // SAFETY: plain value arguments; the result is checked before use.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        // SAFETY: no arguments; reads the calling thread's last error right after the failure.
        let err = unsafe { GetLastError() };
        // "The parameter is incorrect" is what Windows answers for a PID that does not exist.
        // Anything else (access denied for another user's or an elevated process) is unknown.
        return if err == ERROR_INVALID_PARAMETER {
            Probe::Gone
        } else {
            Probe::Unknown
        };
    }
    let handle = Owned(handle);
    let mut code = 0u32;
    // SAFETY: `handle` is a live process handle with query access; `code` is a writable u32.
    if unsafe { GetExitCodeProcess(handle.0, &mut code) } == 0 {
        return Probe::Unknown;
    }
    if code != STILL_ACTIVE {
        // An open handle keeps an exited process's object alive; it is not running.
        return Probe::Gone;
    }
    let zero = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let (mut created, mut exited, mut kernel, mut user) = (zero, zero, zero, zero);
    // SAFETY: `handle` is a live process handle with query access; the four out parameters are
    // writable FILETIME values that outlive the call.
    let ok =
        unsafe { GetProcessTimes(handle.0, &mut created, &mut exited, &mut kernel, &mut user) };
    let start = filetime_u64(created);
    if ok == 0 || !valid_start(start) {
        return Probe::Unknown;
    }
    // An exit time that is set means the process has exited, even if its exit code happened to
    // be 259 (STILL_ACTIVE).
    if filetime_u64(exited) != 0 {
        return Probe::Gone;
    }
    Probe::Running(ProcessId { pid, start })
}

/// Parent PID and executable name of every process, from one snapshot.
fn snapshot() -> Option<HashMap<u32, (u32, String)>> {
    // SAFETY: plain value arguments; the result is checked before use.
    let snap = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snap == INVALID_HANDLE_VALUE || snap.is_null() {
        return None;
    }
    let snap = Owned(snap);
    let mut entry = PROCESSENTRY32W {
        dwSize: u32::try_from(std::mem::size_of::<PROCESSENTRY32W>()).ok()?,
        ..PROCESSENTRY32W::default()
    };
    let mut rows = HashMap::new();
    // SAFETY: `snap` is a live snapshot handle; `entry` is a writable PROCESSENTRY32W with
    // `dwSize` set, as the API requires.
    let mut more = unsafe { Process32FirstW(snap.0, &mut entry) } != 0;
    while more && rows.len() < MAX_ROWS {
        let len = entry
            .szExeFile
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(entry.szExeFile.len());
        let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
        rows.insert(entry.th32ProcessID, (entry.th32ParentProcessID, name));
        // SAFETY: as above.
        more = unsafe { Process32NextW(snap.0, &mut entry) } != 0;
    }
    Some(rows)
}

pub(crate) fn agent_process() -> Option<ProcessId> {
    let rows = snapshot()?;
    first_non_shell_ancestor(std::process::id(), |pid| {
        let (parent, name) = rows.get(&pid)?;
        match check(pid) {
            Probe::Running(id) => Some(Info {
                id,
                parent: (*parent != 0).then_some(*parent),
                name: name.clone(),
            }),
            _ => None,
        }
    })
}
