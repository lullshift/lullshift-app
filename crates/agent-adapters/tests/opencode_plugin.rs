//! ADR-022: the OpenCode plugin file. Install, repair and uninstall against real files in temp
//! directories (nothing here touches the real OpenCode config), the exact generated content, and
//! the generated plugin run under node against a fake OpenCode context. Tests that need node are
//! skipped (with a note) when `node` is not on the PATH.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use agent_adapters::claude_code::install::{InstallError, Status, UninstallOutcome, sha256_hex};
use agent_adapters::opencode::install::{
    PLUGIN_FILE, PluginCommand, default_plugin_path, install, js_string, render, status, uninstall,
};
use serde_json::Value;

/// Current plugin template. A change must bump the version in its header; update both together.
const TEMPLATE_SHA256: &str = include_str!("opencode_plugin.sha256");

struct Env {
    dir: tempfile::TempDir,
    file: PathBuf,
    cmd: PluginCommand,
}

fn env() -> Env {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().to_path_buf();
    Env {
        file: root.join("config").join("plugins").join(PLUGIN_FILE),
        cmd: PluginCommand {
            exe: root.join("Lullshift App").join("lullshift-desktop.exe"),
            data_dir: root.join("data"),
        },
        dir,
    }
}

/// A directory name full of characters that break naive quoting.
fn weird_name() -> String {
    let mut s = String::from("it's \u{e9}\u{65e5}\u{1F600} [x] $y `z` ${q} ;&| %P% ");
    if !cfg!(windows) {
        // Not allowed in Windows file names.
        s.push_str("\"dq\" \\bs\\ ");
    }
    s.push_str("end");
    s
}

fn node_available() -> bool {
    Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

// ---- install, repair, uninstall ------------------------------------------------------------

#[test]
fn install_status_and_uninstall_round_trip() {
    let e = env();
    assert_eq!(status(&e.file, &e.cmd), Status::NotInstalled);
    let record = install(&e.file, &e.cmd).expect("install");
    assert_eq!(record.settings_path, e.file);
    assert_eq!(status(&e.file, &e.cmd), Status::Installed);
    let bytes = fs::read(&e.file).expect("read");
    assert_eq!(bytes, render(&e.cmd).expect("render").into_bytes());
    assert_eq!(record.written_sha256, sha256_hex(&bytes));
    // The only thing created is the one file (and its directories).
    let siblings: Vec<_> = fs::read_dir(e.file.parent().expect("parent"))
        .expect("dir")
        .map(|d| d.expect("entry").file_name())
        .collect();
    assert_eq!(siblings, vec![std::ffi::OsString::from(PLUGIN_FILE)]);

    assert_eq!(
        uninstall(&record).expect("uninstall"),
        UninstallOutcome::Restored
    );
    assert!(!e.file.exists());
    assert_eq!(status(&e.file, &e.cmd), Status::NotInstalled);
    assert_eq!(
        uninstall(&record).expect("again"),
        UninstallOutcome::NothingToRemove
    );
}

#[test]
fn installing_twice_changes_nothing() {
    let e = env();
    let first = install(&e.file, &e.cmd).expect("install");
    let bytes = fs::read(&e.file).expect("read");
    let modified = fs::metadata(&e.file)
        .expect("meta")
        .modified()
        .expect("mtime");
    std::thread::sleep(std::time::Duration::from_millis(30));
    let second = install(&e.file, &e.cmd).expect("install again");
    assert_eq!(first, second);
    assert_eq!(fs::read(&e.file).expect("read"), bytes);
    assert_eq!(
        fs::metadata(&e.file)
            .expect("meta")
            .modified()
            .expect("mtime"),
        modified,
        "an up-to-date file is not rewritten"
    );
}

#[test]
fn moved_app_needs_repair_and_repair_rewrites_the_file() {
    let e = env();
    install(&e.file, &e.cmd).expect("install");
    let moved = PluginCommand {
        exe: e.dir.path().join("elsewhere").join("lullshift-desktop.exe"),
        ..e.cmd.clone()
    };
    assert_eq!(status(&e.file, &moved), Status::NeedsUpdate);
    let record = install(&e.file, &moved).expect("repair");
    assert_eq!(status(&e.file, &moved), Status::Installed);
    assert!(
        fs::read_to_string(&e.file)
            .expect("read")
            .contains(&js_string(moved.exe.to_str().expect("utf8")))
    );
    assert_eq!(
        uninstall(&record).expect("uninstall"),
        UninstallOutcome::Restored
    );
}

#[test]
fn a_file_the_user_edited_is_still_removed_but_reported_as_edited() {
    let e = env();
    let record = install(&e.file, &e.cmd).expect("install");
    let mut text = fs::read_to_string(&e.file).expect("read");
    text.push_str("// my note\n");
    fs::write(&e.file, text).expect("edit");
    assert_eq!(status(&e.file, &e.cmd), Status::NeedsUpdate);
    assert_eq!(
        uninstall(&record).expect("uninstall"),
        UninstallOutcome::Removed
    );
    assert!(!e.file.exists());
}

#[test]
fn a_foreign_file_with_our_name_is_never_replaced_or_removed() {
    for foreign in [
        "export default { id: 'mine', setup() {} }\n".as_bytes(),
        b"",
        b"\xEF\xBB\xBF// Lullshift-owned file: lullshift-observer",
        b"  // Lullshift-owned file: lullshift-observer",
    ] {
        let e = env();
        fs::create_dir_all(e.file.parent().expect("parent")).expect("mkdir");
        fs::write(&e.file, foreign).expect("write");
        assert!(matches!(
            install(&e.file, &e.cmd),
            Err(InstallError::SettingsInvalid)
        ));
        assert_eq!(fs::read(&e.file).expect("read"), foreign);
        assert_eq!(status(&e.file, &e.cmd), Status::SettingsInvalid);
        let record = agent_adapters::hookfile::InstallRecord {
            settings_path: e.file.clone(),
            backup_path: None,
            restore_exact: false,
            written_sha256: String::new(),
            created_events: Vec::new(),
            created_hooks_key: false,
            created_top_keys: Vec::new(),
        };
        assert_eq!(
            uninstall(&record).expect("uninstall"),
            UninstallOutcome::NothingToRemove
        );
        assert_eq!(fs::read(&e.file).expect("read"), foreign);
    }
}

#[test]
fn odd_targets_are_refused() {
    // A directory where the file should be.
    let e = env();
    fs::create_dir_all(&e.file).expect("mkdir");
    assert!(install(&e.file, &e.cmd).is_err());
    assert_eq!(status(&e.file, &e.cmd), Status::SettingsInvalid);
    // A read-only file of ours.
    let e = env();
    install(&e.file, &e.cmd).expect("install");
    let moved = PluginCommand {
        exe: e.dir.path().join("x").join("lullshift-desktop.exe"),
        ..e.cmd.clone()
    };
    let mut perm = fs::metadata(&e.file).expect("meta").permissions();
    perm.set_readonly(true);
    fs::set_permissions(&e.file, perm.clone()).expect("ro");
    assert!(matches!(
        install(&e.file, &moved),
        Err(InstallError::NotWritable)
    ));
    #[allow(clippy::permissions_set_readonly_false)]
    perm.set_readonly(false);
    fs::set_permissions(&e.file, perm).expect("rw");
    // Relative file or app paths.
    let e = env();
    assert!(matches!(
        install(Path::new("plugins/x.js"), &e.cmd),
        Err(InstallError::UnsupportedPath)
    ));
    let rel = PluginCommand {
        exe: PathBuf::from("lullshift-desktop"),
        ..e.cmd.clone()
    };
    assert!(matches!(
        install(&e.file, &rel),
        Err(InstallError::UnsupportedPath)
    ));
    assert!(!e.file.exists());
}

#[cfg(unix)]
#[test]
fn a_symlinked_plugin_file_is_refused() {
    let e = env();
    let real = e.dir.path().join("dotfiles").join("lullshift.js");
    fs::create_dir_all(real.parent().expect("parent")).expect("mkdir");
    install(&real, &e.cmd).expect("install real");
    fs::create_dir_all(e.file.parent().expect("parent")).expect("mkdir");
    std::os::unix::fs::symlink(&real, &e.file).expect("symlink");
    let moved = PluginCommand {
        exe: e.dir.path().join("x").join("lullshift-desktop"),
        ..e.cmd.clone()
    };
    assert!(matches!(
        install(&e.file, &moved),
        Err(InstallError::NotWritable)
    ));
}

#[test]
fn default_location_follows_opencodes_own_rules() {
    // Only the shape is checked here; the environment of the test process is not changed.
    if let Some(p) = default_plugin_path() {
        assert!(p.is_absolute());
        assert!(p.ends_with(Path::new("plugins").join(PLUGIN_FILE)));
    }
}

// ---- generated content ---------------------------------------------------------------------

#[test]
fn generated_content_is_pinned() {
    let e = env();
    let text = render(&e.cmd).expect("render");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines[0],
        "// Lullshift-owned file: lullshift-observer, plugin version 2 (experimental, OpenCode 2.x only)."
    );
    assert!(text.ends_with("};\n"));
    // Exactly the two embedded literals, each on its own line, ASCII only, nothing left over.
    let exe = format!(
        "const EXE = {};",
        js_string(e.cmd.exe.to_str().expect("utf8"))
    );
    let dir = format!(
        "const DATA_DIR = {};",
        js_string(e.cmd.data_dir.to_str().expect("utf8"))
    );
    assert_eq!(lines.iter().filter(|l| **l == exe).count(), 1);
    assert_eq!(lines.iter().filter(|l| **l == dir).count(), 1);
    assert!(text.is_ascii());
    assert!(!text.contains("__LULLSHIFT"));
    // The plugin never uses a shell, never exports hooks, and never awaits its child.
    for forbidden in [
        "exec(",
        "execSync",
        "shell:",
        "await spawn",
        "tool.execute",
        ".hook(",
    ] {
        assert!(!text.contains(forbidden), "{forbidden}");
    }
    assert!(text.contains(r#"spawn(EXE, ["hook", ADAPTER, "--data-dir", DATA_DIR]"#));
    // Pin the template so a change is deliberate: bump the plugin version and this hash together.
    let template = include_str!("../src/opencode/plugin.js");
    assert_eq!(
        sha256_hex(template.as_bytes()),
        TEMPLATE_SHA256.trim(),
        "plugin.js changed: bump the version in its header and update opencode_plugin.sha256"
    );
}

#[test]
fn embedded_paths_round_trip_through_node() {
    if !node_available() {
        eprintln!("skipped: node is not available");
        return;
    }
    let e = env();
    let cmd = PluginCommand {
        exe: e
            .dir
            .path()
            .join(weird_name())
            .join("lullshift-desktop.exe"),
        data_dir: e.dir.path().join(weird_name()).join("data"),
    };
    let text = render(&cmd).expect("render");
    let file = e.dir.path().join("plugin.mjs");
    fs::write(&file, &text).expect("write");
    // Read the two literals back out of the generated file and evaluate them as JS strings.
    let script = r#"
      import { readFileSync } from "node:fs";
      const src = readFileSync(process.argv[1], "utf8");
      const take = (name) => {
        const m = src.match(new RegExp("^const " + name + " = (.*);$", "m"));
        return (0, eval)("(" + m[1] + ")");
      };
      process.stdout.write(JSON.stringify({ exe: take("EXE"), dataDir: take("DATA_DIR") }));
    "#;
    let out = Command::new("node")
        .args(["--input-type=module", "-e", script])
        .arg(&file)
        .output()
        .expect("node");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let got: Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(got["exe"].as_str(), cmd.exe.to_str());
    assert_eq!(got["dataDir"].as_str(), cmd.data_dir.to_str());
    // And the whole file is valid JavaScript.
    let check = Command::new("node")
        .arg("--check")
        .arg(&file)
        .output()
        .expect("node");
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );
}

// ---- the plugin under node -----------------------------------------------------------------

fn harness() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("opencode_plugin_harness.mjs")
}

/// Writes the plugin for `cmd`, optionally with `node:child_process` replaced by the harness's
/// fake, runs the scenario and returns its JSON.
fn run_scenario(scenario: &str, fake_spawn: bool, cmd: &PluginCommand, dir: &Path) -> Value {
    let mut text = render(cmd).expect("render");
    if fake_spawn {
        let real = r#"import { spawn } from "node:child_process";"#;
        assert!(text.contains(real));
        text = text.replace(real, "const spawn = (...a) => globalThis.__spawn(...a);");
    }
    let file = dir.join("plugin.mjs");
    fs::write(&file, text).expect("write");
    let out = Command::new("node")
        .arg(harness())
        .arg(&file)
        .arg(scenario)
        .output()
        .expect("node");
    assert!(
        out.status.success(),
        "{scenario}: {}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("json")
}

fn bodies(v: &Value) -> Vec<Value> {
    v["calls"]
        .as_array()
        .expect("calls")
        .iter()
        .map(|c| serde_json::from_str(c["body"].as_str().expect("body")).expect("json body"))
        .collect()
}

fn summary(v: &Value) -> Vec<(String, String)> {
    bodies(v)
        .iter()
        .map(|b| {
            (
                b["event"].as_str().expect("event").to_owned(),
                b["sessionId"].as_str().expect("session").to_owned(),
            )
        })
        .collect()
}

fn pair(a: &str, b: &str) -> (String, String) {
    (a.to_owned(), b.to_owned())
}

#[test]
fn plugin_is_observe_only_and_spawns_the_forwarder_with_an_args_array() {
    if !node_available() {
        eprintln!("skipped: node is not available");
        return;
    }
    let e = env();
    let cmd = PluginCommand {
        exe: e
            .dir
            .path()
            .join(weird_name())
            .join("lullshift-desktop.exe"),
        data_dir: e.dir.path().join("data"),
    };
    let v = run_scenario("flow", true, &cmd, e.dir.path());

    // Shape: only `id` and `setup`; setup is synchronous and returns just a cleanup function
    // (never hooks, never a promise OpenCode would have to wait for), and the stream is abortable.
    assert_eq!(v["shape"]["keys"], serde_json::json!(["id", "setup"]));
    assert_eq!(v["shape"]["id"], "lullshift-observer");
    assert_eq!(v["shape"]["setupReturnedPromise"], false);
    assert_eq!(v["shape"]["cleanupIsFunction"], true);
    assert_eq!(v["shape"]["signalPassed"], true);
    assert_eq!(v["aborted"], true, "cleanup aborts the subscription");

    // Every spawn: the forwarder with an args array, stdin piped, no shell, detached.
    for call in v["calls"].as_array().expect("calls") {
        assert_eq!(call["exe"].as_str(), cmd.exe.to_str());
        assert_eq!(
            call["args"],
            serde_json::json!(["hook", "opencode", "--data-dir", cmd.data_dir.to_str()])
        );
        let opts = &call["opts"];
        assert_eq!(
            opts["stdio"],
            serde_json::json!(["pipe", "ignore", "ignore"])
        );
        assert_eq!(opts["detached"], true);
        assert!(opts.get("shell").is_none_or(|s| s.as_bool() == Some(false)));
    }

    // Burst of 10 activity events inside a second is one message.
    assert_eq!(
        v["afterBurst"], 2,
        "session.created plus one coalesced working"
    );

    // Mapping and de-duplication, in order.
    assert_eq!(
        summary(&v),
        vec![
            pair("session.created", "ses_a"),
            pair("session.tool.called", "ses_a"),
            pair("session.execution.started", "ses_a"),
            pair("permission.asked", "ses_a"),
            pair("permission.replied", "ses_a"),
            pair("turn_ended", "ses_a"),
            pair("session.execution.started", "ses_b"),
            pair("failed", "ses_b"),
            pair("session.deleted", "ses_a"),
            pair("session.execution.started", "ses_after_junk"),
        ]
    );

    // Allow-listed fields only, and nothing from the events' payloads.
    for body in bodies(&v) {
        let keys: Vec<&str> = body
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect();
        assert!(
            keys.iter()
                .all(|k| ["v", "event", "sessionId", "hint"].contains(k)),
            "{body}"
        );
        assert_eq!(body["v"], 1);
    }
    let all = serde_json::to_string(&v["calls"]).expect("json");
    for secret in ["SECRET", "/SECRET", "title", "input", "cmd"] {
        // `title` and `input` appear only as the harness's own words, never in a body.
        assert!(
            !bodies(&v).iter().any(|b| b.to_string().contains(secret)),
            "{secret} leaked: {all}"
        );
    }
    let hints: Vec<_> = bodies(&v)
        .iter()
        .filter_map(|b| b["hint"].as_str().map(str::to_owned))
        .collect();
    assert_eq!(hints, vec!["always"]);
}

#[test]
fn prompt_is_sent_only_with_the_consent_marker_and_only_for_user_messages() {
    if !node_available() {
        eprintln!("skipped: node is not available");
        return;
    }
    let e = env();
    // Off: no marker file.
    let off = run_scenario("prompt", true, &e.cmd, e.dir.path());
    for b in bodies(&off) {
        assert!(b.get("prompt").is_none(), "{b}");
    }
    assert!(!off["calls"].to_string().contains("FIRST PROMPT"));
    // On: the marker exists in the data directory.
    let marker = e.cmd.data_dir.join("context-capture");
    fs::create_dir_all(&marker).expect("mkdir");
    fs::write(marker.join("opencode"), b"").expect("marker");
    let on = run_scenario("prompt", true, &e.cmd, e.dir.path());
    let sent = bodies(&on);
    let prompts: Vec<(String, String)> = sent
        .iter()
        .filter_map(|b| {
            Some((
                b["sessionId"].as_str()?.to_owned(),
                b["prompt"].as_str()?.to_owned(),
            ))
        })
        .collect();
    // The synthetic item is ignored; only the first prompt of each session is sent.
    assert_eq!(prompts.len(), 4, "{prompts:?}");
    assert_eq!(prompts[0], pair("ses_p", "FIRST PROMPT"));
    assert!(!on["calls"].to_string().contains("SECOND PROMPT"));
    assert_eq!(sent[0]["event"], "session.inbox.enqueued");
    let cp = |i: usize| prompts[i].1.chars().count();
    assert_eq!(
        cp(1),
        4000,
        "long prompts are cut before they leave OpenCode"
    );
    // 3999 letters + emoji + "tail": the cut at 4000 code points keeps the whole emoji.
    assert_eq!(cp(2), 4000);
    assert!(prompts[2].1.ends_with('\u{1F600}'));
    // 3998 letters + two emoji + "tail": exactly 4000 whole characters, no split pair.
    assert_eq!(cp(3), 4000);
    assert!(prompts[3].1.ends_with("\u{1F600}\u{1F600}"));
    assert!(!on["calls"].to_string().contains("/SECRET/file"));
}

#[test]
fn plugin_limits_how_many_forwarders_run_at_once() {
    if !node_available() {
        eprintln!("skipped: node is not available");
        return;
    }
    let e = env();
    let v = run_scenario("flood", true, &e.cmd, e.dir.path());
    assert_eq!(v["calls"].as_array().expect("calls").len(), 8);
}

#[test]
fn dropped_messages_are_not_remembered_and_are_retried() {
    if !node_available() {
        eprintln!("skipped: node is not available");
        return;
    }
    let e = env();
    let marker = e.cmd.data_dir.join("context-capture");
    fs::create_dir_all(&marker).expect("mkdir");
    fs::write(marker.join("opencode"), b"").expect("marker");
    let v = run_scenario("dropped", true, &e.cmd, e.dir.path());
    assert_eq!(v["whileFull"], 8, "nothing more starts while 8 are running");
    let sent = bodies(&v);
    assert_eq!(sent.len(), 12, "{sent:?}");
    let after = summary(&v)[8..].to_vec();
    assert_eq!(
        after,
        vec![
            pair("turn_ended", "ses_x"),
            pair("session.execution.started", "ses_w"),
            pair("permission.asked", "ses_y"),
            pair("session.inbox.enqueued", "ses_z"),
        ],
        "each dropped event is sent when it comes again"
    );
    assert_eq!(sent[11]["prompt"], "PROMPT Z AGAIN");
    assert!(!v["calls"].to_string().contains("PROMPT Z THIRD"));
    assert!(!v["calls"].to_string().contains("\"PROMPT Z\""));
}

#[test]
fn plugin_never_throws_whatever_happens() {
    if !node_available() {
        eprintln!("skipped: node is not available");
        return;
    }
    let e = env();
    let v = run_scenario("spawn_throws", true, &e.cmd, e.dir.path());
    assert_eq!(v["survived"], true);
    let v = run_scenario("no_event_api", true, &e.cmd, e.dir.path());
    assert_eq!(v["noEventApi"], true);
    // The unmodified plugin (real child_process) with an executable that does not exist, and with
    // one that exits at once: no error escapes, node exits 0.
    let missing = PluginCommand {
        exe: e
            .dir
            .path()
            .join("does-not-exist")
            .join("lullshift-desktop.exe"),
        ..e.cmd.clone()
    };
    assert_eq!(
        run_scenario("real", false, &missing, e.dir.path())["realOk"],
        true
    );
    let wrong = PluginCommand {
        exe: PathBuf::from(std::env::var_os("NODE_EXE").unwrap_or_else(node_path)),
        ..e.cmd.clone()
    };
    if wrong.exe.is_absolute() {
        assert_eq!(
            run_scenario("real", false, &wrong, e.dir.path())["realOk"],
            true
        );
    }
}

fn node_path() -> std::ffi::OsString {
    let probe = "process.stdout.write(process.execPath)";
    Command::new("node")
        .args(["-e", probe])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned().into())
        .unwrap_or_default()
}
