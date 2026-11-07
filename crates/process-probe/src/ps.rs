//! macOS and other Unix without `/proc`: `/bin/ps`, with a short deadline. Safe code only.
//! Start times have one-second resolution, which is enough to tell a reused PID apart.

use std::io::Read;
use std::process::Stdio;
use std::time::{Duration, Instant};

use crate::parse::{PsLine, key_from, parse_ps_line, ps_command};
use crate::{Info, Probe, ProcessId};

const DEADLINE: Duration = Duration::from_millis(800);

/// Runs `ps` for one PID. `Ok(None)`: ps ran and found no such process.
fn run(pid: u32) -> Result<Option<PsLine>, ()> {
    let mut child = ps_command(pid)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| ())?;
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < DEADLINE => {
                std::thread::sleep(Duration::from_millis(5));
            }
            _ => {
                // Our own helper process, never the agent's.
                let _ = child.kill();
                let _ = child.wait();
                return Err(());
            }
        }
    };
    let mut out = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        let _ = (&mut stdout).take(4096).read_to_string(&mut out);
    }
    let line = out.lines().find(|l| !l.trim().is_empty());
    match (status.success(), line) {
        (true, Some(line)) => parse_ps_line(line).map(Some).ok_or(()),
        // `ps -p` exits 1 and prints nothing when the process does not exist.
        (false, None) if status.code() == Some(1) => Ok(None),
        _ => Err(()),
    }
}

fn id_of(pid: u32, line: &PsLine) -> ProcessId {
    ProcessId {
        pid,
        start: key_from(&[line.started.as_bytes()]),
    }
}

pub(crate) fn check(pid: u32) -> Probe {
    match run(pid) {
        Ok(Some(line)) if line.zombie => Probe::Gone,
        Ok(Some(line)) => Probe::Running(id_of(pid, &line)),
        Ok(None) => Probe::Gone,
        Err(()) => Probe::Unknown,
    }
}

pub(crate) fn info(pid: u32) -> Option<Info> {
    let line = run(pid).ok().flatten().filter(|l| !l.zombie)?;
    Some(Info {
        id: id_of(pid, &line),
        parent: (line.ppid != 0).then_some(line.ppid),
        name: line.name,
    })
}
