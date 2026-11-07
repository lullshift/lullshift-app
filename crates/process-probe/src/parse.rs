//! Parsers for what `/proc` and `ps` print. Compiled on every platform so they are tested
//! everywhere; only some are used on each.
#![allow(dead_code)]

/// FNV-1a, 64 bit. Used to fold text into an opaque start key.
fn fnv1a(parts: &[&[u8]]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for part in parts {
        for &b in *part {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        // Separator, so ("ab","c") and ("a","bc") differ.
        h ^= 0xff;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Folds text into a valid start key (1..=2^62-1).
pub(crate) fn key_from(parts: &[&[u8]]) -> u64 {
    (fnv1a(parts) & crate::MAX_START_KEY).max(1)
}

/// What `/proc/<pid>/stat` holds that the probe needs.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ProcStat {
    /// Process state letter, `Z` for a zombie.
    pub state: char,
    pub ppid: u32,
    /// Start time in clock ticks since boot (field 22).
    pub start_ticks: u64,
}

/// Parses one `/proc/<pid>/stat` line. The command name sits in parentheses and may itself
/// contain spaces and parentheses, so the fields are counted after the *last* `)`.
pub(crate) fn parse_proc_stat(text: &str) -> Option<ProcStat> {
    let rest = &text[text.rfind(')')? + 1..];
    let mut fields = rest.split_whitespace();
    let state = fields.next()?.chars().next()?;
    let ppid = fields.next()?.parse().ok()?;
    // After ppid: pgrp, session, tty_nr, tpgid, flags, minflt, cminflt, majflt, cmajflt, utime,
    // stime, cutime, cstime, priority, nice, num_threads, itrealvalue (17 fields), then
    // starttime.
    let start_ticks = fields.nth(17)?.parse().ok()?;
    Some(ProcStat {
        state,
        ppid,
        start_ticks,
    })
}

/// What one `ps -o stat= -o ppid= -o lstart= -o comm= -p <pid>` line holds.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct PsLine {
    pub zombie: bool,
    pub ppid: u32,
    /// The start time text, whitespace-normalised (`Fri Nov 7 10:00:00 2026`).
    pub started: String,
    /// The executable's file name.
    pub name: String,
}

/// Parses a `ps` line: `STAT PPID DOW MON DAY HH:MM:SS YEAR COMMAND...`. The command is the
/// rest of the line and may contain spaces; only its file name is kept.
pub(crate) fn parse_ps_line(line: &str) -> Option<PsLine> {
    let mut it = line.split_whitespace();
    let stat = it.next()?;
    let ppid = it.next()?.parse().ok()?;
    let started: Vec<&str> = (&mut it).take(5).collect();
    if started.len() != 5 || !started[3].contains(':') {
        return None;
    }
    let command: Vec<&str> = it.collect();
    let command = command.join(" ");
    let name = command
        .rsplit('/')
        .next()
        .unwrap_or(&command)
        .trim()
        .to_owned();
    if name.is_empty() {
        return None;
    }
    Some(PsLine {
        zombie: stat.starts_with('Z'),
        ppid,
        started: started.join(" "),
        name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proc_stat_is_counted_after_the_last_paren() {
        let line = "1234 (node) S 1000 1234 1234 0 -1 4194560 100 0 0 0 5 3 0 0 20 0 11 0 987654 12345678 900 18446744073709551615";
        let s = parse_proc_stat(line).expect("parses");
        assert_eq!(
            s,
            ProcStat {
                state: 'S',
                ppid: 1000,
                start_ticks: 987_654
            }
        );
        // A hostile command name with spaces and parentheses cannot shift the fields.
        let evil = "7 (a) S 1 (b c) Z 999 1 1 0 -1 0 0 0 0 0 0 0 0 0 20 0 1 0 424242 1 1 1";
        let s = parse_proc_stat(evil).expect("parses");
        assert_eq!((s.state, s.ppid, s.start_ticks), ('Z', 999, 424_242));
        let zombie = "9 (sh) Z 1 9 9 0 -1 0 0 0 0 0 0 0 0 0 20 0 1 0 55 0 0 0";
        assert_eq!(parse_proc_stat(zombie).expect("zombie").state, 'Z');
    }

    #[test]
    fn broken_proc_stat_lines_are_refused() {
        for bad in [
            "",
            "1 (x)",
            "1 x S 2",
            "1 (x) S notanumber 0",
            "1 (x) S 2 3 4",
        ] {
            assert_eq!(parse_proc_stat(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn ps_lines_parse_with_spaces_in_the_command() {
        let p = parse_ps_line("S+      812 Fri Nov  7 10:00:00 2026 /usr/local/bin/claude")
            .expect("parses");
        assert_eq!(p.ppid, 812);
        assert!(!p.zombie);
        assert_eq!(p.started, "Fri Nov 7 10:00:00 2026");
        assert_eq!(p.name, "claude");
        let p = parse_ps_line(
            "Ss 1 Mon Jan  1 00:00:01 2026 /Applications/Visual Studio Code.app/Contents/MacOS/Electron",
        )
        .expect("parses");
        assert_eq!(p.name, "Electron");
        let z = parse_ps_line("Z 5 Mon Jan  1 00:00:01 2026 (node)").expect("zombie");
        assert!(z.zombie);
    }

    #[test]
    fn broken_ps_lines_are_refused() {
        for bad in [
            "",
            "S 12",
            "S x Fri Nov 7 10:00:00 2026 cmd",
            "S 12 Fri Nov 7 nottime 2026 cmd",
            "S 12 Fri Nov 7 10:00:00 2026",
        ] {
            assert_eq!(parse_ps_line(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn start_keys_are_valid_and_separate_their_parts() {
        let a = key_from(&[b"boot-1", b"100"]);
        let b = key_from(&[b"boot-2", b"100"]);
        let c = key_from(&[b"boot-1", b"101"]);
        assert!(crate::valid_start(a) && crate::valid_start(b) && crate::valid_start(c));
        assert!(a != b && a != c);
        assert_ne!(key_from(&[b"ab", b"c"]), key_from(&[b"a", b"bc"]));
        assert_eq!(a, key_from(&[b"boot-1", b"100"]), "deterministic");
    }
}
