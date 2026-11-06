//! Review fix M2: the generated command line is run through `pwsh -NoProfile -Command` and
//! `powershell -NoProfile -Command` (the way an agent host passes a command string) against a
//! real native executable that echoes its arguments. Folders have spaces and non-ASCII
//! characters. The executable is built from a tiny source with `rustc`; a missing `rustc` or
//! shell skips the test with a note on stderr. Nothing here touches the real home directory.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use agent_adapters::hookfile::{Shell, command_line};

const ECHO_SRC: &str = r#"
fn main() {
    for a in std::env::args().skip(1) {
        let codes: Vec<String> = a.encode_utf16().map(|u| u.to_string()).collect();
        println!("{}", codes.join(","));
    }
}
"#;

fn codes(s: &str) -> String {
    s.encode_utf16()
        .map(|u| u.to_string())
        .collect::<Vec<_>>()
        .join(",")
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

/// Builds the echo program at `out`. `None` if `rustc` isn't available.
fn build_echo(out: &Path, scratch: &Path) -> Option<()> {
    let src = scratch.join("echo_argv.rs");
    fs::write(&src, ECHO_SRC).ok()?;
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    Command::new(rustc)
        .args(["--edition", "2021", "-O", "-o"])
        .arg(out)
        .arg(&src)
        .stdin(Stdio::null())
        .output()
        .ok()?
        .status
        .success()
        .then_some(())
}

#[test]
fn powershell_command_line_round_trips_argv_through_a_native_program() {
    let shells: Vec<&str> = ["pwsh", "powershell"]
        .into_iter()
        .filter(|p| works(p, &["-NoProfile", "-NonInteractive", "-Command", "exit 0"]))
        .collect();
    if shells.is_empty() {
        eprintln!("skipped: PowerShell is not available");
        return;
    }
    let root = tempfile::tempdir().expect("tempdir");
    let built = root.path().join(if cfg!(windows) {
        "echo_argv.exe"
    } else {
        "echo_argv"
    });
    if build_echo(&built, root.path()).is_none() {
        eprintln!("skipped: rustc is not available to build the echo program");
        return;
    }
    // Spaces, an apostrophe, a curly quote look-alike and non-ASCII letters in both paths. None
    // of them is refused: their UTF-8 bytes avoid 0x82 and 0x91 to 0x94.
    let folders = [
        "Lull shift \u{fc}ber \u{65e5}\u{672c}",
        "it's \u{c9}t\u{e9} \u{2019}n",
        "plain",
    ];
    for ps in shells {
        for (i, f) in folders.iter().enumerate() {
            let exe_dir: PathBuf = root.path().join(format!("{f} app{i}"));
            fs::create_dir_all(&exe_dir).expect("mkdir");
            let exe = exe_dir.join(if cfg!(windows) {
                "lullshift-desktop.exe"
            } else {
                "lullshift-desktop"
            });
            fs::copy(&built, &exe).expect("copy");
            let data = root.path().join(format!("data {f} \u{e9}{i}"));
            let cmd = command_line(&exe, "cursor", &data, Shell::PowerShell).expect("quotable");
            let out = Command::new(ps)
                .args(["-NoProfile", "-NonInteractive", "-Command"])
                .arg(&cmd)
                .stdin(Stdio::null())
                .output()
                .expect("run powershell");
            assert!(
                out.status.success(),
                "{ps}: {cmd}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            let got = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
            let want: String = ["hook", "cursor", "--data-dir", data.to_str().expect("utf8")]
                .iter()
                .map(|t| format!("{}\n", codes(t)))
                .collect();
            assert_eq!(got, want, "{ps}: {cmd}");
        }
    }
}

/// The paths that are refused for PowerShell (legacy code page hazard) are refused up front.
#[test]
fn cyrillic_ve_in_a_powershell_path_is_refused_not_mangled() {
    let abs = std::env::temp_dir();
    let dir = abs.join("\u{412}\u{430}\u{43d}\u{44f}");
    let exe = abs.join("lullshift-desktop");
    assert!(command_line(&exe, "cursor", &dir, Shell::PowerShell).is_none());
    assert!(command_line(&exe, "cursor", &dir, Shell::Sh).is_some());
}
