//! S12-T03: the hook command line is escaped per shell. These tests run real shells (sh,
//! PowerShell 7 and Windows PowerShell) on many random, awkward texts and check that every
//! argument arrives unchanged and nothing runs. A shell that isn't installed is skipped, with a
//! note on stderr. Nothing here touches the real home directory.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use agent_adapters::hookfile::{Shell, command_line, quote_arg};

const NASTY: &[char] = &[
    '\'', '"', '`', '$', ';', '&', '|', '<', '>', '(', ')', '{', '}', '[', ']', '*', '?', '!', '#',
    '%', '^', '~', '=', '+', ',', '@', ' ', '\\', '/', ':', '-', '_', '.', '\u{2018}', '\u{2019}',
    '\u{201A}', '\u{201B}', '\u{201C}', '\u{201D}', '\u{FF07}', 'é', 'ß', '日', '本', '😀',
    '\u{0301}', '\u{200D}', '\u{00A0}', 'a', 'b', 'Z', '0', '9',
];

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn text(&mut self) -> String {
        let len = 1 + self.below(20);
        (0..len).map(|_| NASTY[self.below(NASTY.len())]).collect()
    }
    /// A text that is also a legal single file name on Windows and Unix.
    fn file_name(&mut self) -> String {
        let s: String = self
            .text()
            .chars()
            .filter(|c| !"\\/:*?\"<>|".contains(*c))
            .collect();
        let s = s.trim_matches([' ', '.']).to_owned();
        if s.is_empty() { "x".to_owned() } else { s }
    }
}

fn works(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn skip(what: &str) {
    eprintln!("skipped: {what} is not available");
}

fn codes(s: &str) -> String {
    s.encode_utf16()
        .map(|u| u.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn run_ps(ps: &str, script: &Path) -> String {
    let out = Command::new(ps)
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
        ])
        .arg("-File")
        .arg(script)
        .stdin(Stdio::null())
        .output()
        .expect("run powershell");
    assert!(
        out.status.success(),
        "{ps} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    if !err.trim().is_empty() {
        eprintln!("{ps} stderr: {err}");
    }
    String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n")
}

/// A `.ps1` file; the BOM makes Windows PowerShell read it as UTF-8.
fn write_ps1(path: &Path, body: &str) {
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(body.as_bytes());
    fs::write(path, bytes).expect("write ps1");
}

const PS_ECHO_CODES: &str =
    "[Console]::Out.WriteLine((($a.ToCharArray() | ForEach-Object { [int]$_ }) -join ','))";

#[test]
fn sh_gives_every_argument_back_unchanged() {
    if !works("sh", &["-c", "exit 0"]) {
        return skip("sh");
    }
    let dir = tempfile::tempdir().expect("tempdir");
    let mut rng = Rng(0x1234_5678_9ABC_DEF1);
    for round in 0..20 {
        let texts: Vec<String> = (0..100).map(|_| rng.text()).collect();
        let mut script = String::from("printf '%s\\n'");
        for t in &texts {
            script.push(' ');
            script.push_str(&quote_arg(t, Shell::Sh).expect("quotable"));
        }
        script.push('\n');
        let file = dir.path().join(format!("s{round}.sh"));
        fs::write(&file, script).expect("write");
        let out = Command::new("sh")
            .arg(&file)
            .current_dir(dir.path())
            .stdin(Stdio::null())
            .output()
            .expect("run sh");
        assert!(out.status.success());
        let got = String::from_utf8(out.stdout).expect("utf8");
        let got: Vec<&str> = got.lines().collect();
        assert_eq!(got, texts.iter().map(String::as_str).collect::<Vec<_>>());
    }
    // Nothing the texts contained ran: only the scripts are in the directory.
    assert_eq!(fs::read_dir(dir.path()).expect("ls").count(), 20);
}

#[test]
fn sh_injection_attempts_stay_text() {
    if !works("sh", &["-c", "exit 0"]) {
        return skip("sh");
    }
    let dir = tempfile::tempdir().expect("tempdir");
    let attacks = [
        "'; touch pwned; echo '",
        "$(touch pwned)",
        "`touch pwned`",
        "a' ; touch pwned #",
        "\\'; touch pwned; '",
        "x\u{2019}; touch pwned; \u{2019}",
    ];
    for (i, a) in attacks.iter().enumerate() {
        let script = format!("printf '%s\\n' {}\n", quote_arg(a, Shell::Sh).expect("q"));
        let file = dir.path().join(format!("a{i}.sh"));
        fs::write(&file, script).expect("write");
        let out = Command::new("sh")
            .arg(&file)
            .current_dir(dir.path())
            .output()
            .expect("run");
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim_end(), *a);
    }
    assert!(!dir.path().join("pwned").exists(), "an attack ran");
}

#[cfg(unix)]
#[test]
fn sh_command_line_runs_an_executable_in_an_awkward_folder() {
    use std::os::unix::fs::PermissionsExt;
    if !works("sh", &["-c", "exit 0"]) {
        return skip("sh");
    }
    let root = tempfile::tempdir().expect("tempdir");
    let mut rng = Rng(0xDEAD_BEEF_0BAD_F00D);
    for _ in 0..40 {
        let folder = root.path().join(rng.file_name());
        fs::create_dir_all(&folder).expect("mkdir");
        let exe = folder.join("lullshift-desktop");
        fs::write(&exe, "#!/bin/sh\nprintf '%s\\n' \"$@\"\n").expect("write");
        fs::set_permissions(&exe, fs::Permissions::from_mode(0o755)).expect("chmod");
        let data = root.path().join(rng.file_name());
        let cmd = command_line(&exe, "cursor", &data, Shell::Sh).expect("quotable");
        let out = Command::new("sh")
            .arg("-c")
            .arg(&cmd)
            .current_dir(root.path())
            .output()
            .expect("run");
        assert!(out.status.success(), "{cmd}");
        let got = String::from_utf8(out.stdout).expect("utf8");
        let want = format!(
            "hook\ncursor\n--data-dir\n{}\n",
            data.to_str().expect("utf8")
        );
        assert_eq!(got, want, "{cmd}");
    }
}

fn powershells() -> Vec<&'static str> {
    ["pwsh", "powershell"]
        .into_iter()
        .filter(|p| works(p, &["-NoProfile", "-NonInteractive", "-Command", "exit 0"]))
        .collect()
}

#[test]
fn powershell_gives_every_argument_back_unchanged() {
    let shells = powershells();
    if shells.is_empty() {
        return skip("PowerShell");
    }
    let dir = tempfile::tempdir().expect("tempdir");
    let mut rng = Rng(0x0F0F_1234_ABCD_9876);
    for ps in shells {
        for round in 0..6 {
            let texts: Vec<String> = (0..100).map(|_| rng.text()).collect();
            let items: Vec<String> = texts
                .iter()
                .map(|t| quote_arg(t, Shell::PowerShell).expect("quotable"))
                .collect();
            let body = format!(
                "foreach ($a in @({})) {{ {PS_ECHO_CODES} }}\n",
                items.join(",")
            );
            let file = dir.path().join(format!("p{round}.ps1"));
            write_ps1(&file, &body);
            let got = run_ps(ps, &file);
            let want: String = texts.iter().map(|t| format!("{}\n", codes(t))).collect();
            assert_eq!(got, want, "{ps} round {round}");
        }
    }
}

#[test]
fn powershell_injection_attempts_stay_text() {
    let shells = powershells();
    if shells.is_empty() {
        return skip("PowerShell");
    }
    let dir = tempfile::tempdir().expect("tempdir");
    let marker = dir.path().join("pwned");
    let m = marker.to_str().expect("utf8").replace('\'', "''");
    let attacks = [
        format!("'; New-Item '{m}'; '"),
        format!("\u{2019}; New-Item '{m}'; \u{2019}"),
        format!("\u{201B}; New-Item '{m}'; \u{201B}"),
        "$(New-Item pwned2)".to_owned(),
        "`n; calc".to_owned(),
        "a'b\u{2018}c\u{201A}d".to_owned(),
    ];
    for ps in shells {
        let items: Vec<String> = attacks
            .iter()
            .map(|a| quote_arg(a, Shell::PowerShell).expect("q"))
            .collect();
        let body = format!(
            "foreach ($a in @({})) {{ {PS_ECHO_CODES} }}\n",
            items.join(",")
        );
        let file = dir.path().join("attack.ps1");
        write_ps1(&file, &body);
        let got = run_ps(ps, &file);
        let want: String = attacks.iter().map(|t| format!("{}\n", codes(t))).collect();
        assert_eq!(got, want, "{ps}");
    }
    assert!(!marker.exists(), "an attack ran");
}

#[test]
fn powershell_command_line_runs_a_script_in_an_awkward_folder() {
    let shells = powershells();
    if shells.is_empty() {
        return skip("PowerShell");
    }
    let root = tempfile::tempdir().expect("tempdir");
    let mut rng = Rng(0x5555_AAAA_1357_2468);
    for ps in shells {
        for i in 0..15 {
            let folder: PathBuf = root.path().join(rng.file_name());
            fs::create_dir_all(&folder).expect("mkdir");
            let exe = folder.join(format!("lullshift-{i}.ps1"));
            write_ps1(
                &exe,
                &format!("foreach ($a in $args) {{ {PS_ECHO_CODES} }}\n"),
            );
            let data = root.path().join(rng.file_name());
            let cmd = command_line(&exe, "cursor", &data, Shell::PowerShell);
            if exe.to_str().is_some_and(|s| s.contains(['[', ']'])) {
                // PowerShell resolves the program path as a wildcard pattern: refused.
                assert!(cmd.is_none(), "{exe:?}");
                continue;
            }
            let cmd = cmd.expect("quotable");
            let wrapper = root.path().join("wrapper.ps1");
            write_ps1(&wrapper, &format!("{cmd}\n"));
            let got = run_ps(ps, &wrapper);
            let want: String = ["hook", "cursor", "--data-dir", data.to_str().expect("utf8")]
                .iter()
                .map(|t| format!("{}\n", codes(t)))
                .collect();
            assert_eq!(got, want, "{ps}: {cmd}");
        }
    }
}
