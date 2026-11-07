//! Fixture tests for the log scan (ADR-024). Every fixture is made up.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use time::OffsetDateTime;

use super::*;

const CC_ID: &str = "11111111-2222-4333-8444-555555555555";
const CU_ID: &str = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";
const CX_ID: &str = "019db995-c01a-7b2e-9f3c-1234567890ab";

fn write(path: &Path, lines: &[&str], age: Duration) {
    fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    let mut body = lines.join("\n");
    body.push('\n');
    fs::write(path, body).expect("write");
    let f = fs::File::options().write(true).open(path).expect("open");
    f.set_modified(SystemTime::now() - age).expect("mtime");
}

fn now() -> OffsetDateTime {
    OffsetDateTime::now_utc()
}

struct Fixture {
    _dir: tempfile::TempDir,
    roots: Roots,
    base: PathBuf,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().expect("tmp");
    let base = dir.path().to_path_buf();
    let roots = Roots {
        claude_projects: Some(base.join("claude").join("projects")),
        cursor_projects: Some(base.join("cursor").join("projects")),
        codex_home: Some(base.join("codex")),
        opencode_db: Some(base.join("opencode").join("opencode.db")),
    };
    Fixture {
        _dir: dir,
        roots,
        base,
    }
}

fn claude_file(f: &Fixture, project: &str, id: &str) -> PathBuf {
    f.roots
        .claude_projects
        .as_ref()
        .expect("root")
        .join(project)
        .join(format!("{id}.jsonl"))
}

fn only(report: &Report, provider: &str) -> Vec<Found> {
    report
        .sessions
        .iter()
        .filter(|s| s.provider == provider)
        .cloned()
        .collect()
}

// ---- Claude Code ----------------------------------------------------------------------------

#[test]
fn claude_session_with_saved_title_and_ended_turn() {
    let f = fixture();
    write(
        &claude_file(&f, "D--work-shop-api", CC_ID),
        &[
            r#"{"type":"queue-operation","operation":"enqueue","sessionId":"x"}"#,
            r#"{"type":"user","cwd":"D:\\work\\shop\\api","sessionId":"x","message":{"role":"user","content":"Fix the login bug\nand add tests"}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash"}],"stop_reason":"tool_use"}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1"}]},"toolUseResult":{}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Done"}],"stop_reason":"end_turn"}}"#,
            r#"{"type":"custom-title","customTitle":"Old name","sessionId":"x"}"#,
            r#"{"type":"agent-name","agentName":"Login fix","sessionId":"x"}"#,
            r#"{"type":"custom-title","customTitle":"Login fix","sessionId":"x"}"#,
            r#"{"type":"system","subtype":"stop_hook_summary"}"#,
        ],
        Duration::from_secs(600),
    );
    let r = scan(&f.roots, &Limits::default(), now());
    let s = only(&r, "claude-code");
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].session_id, CC_ID);
    assert_eq!(s[0].title.as_deref(), Some("Login fix"));
    assert_eq!(s[0].project_path.as_deref(), Some(r"D:\work\shop\api"));
    assert_eq!(s[0].guess, Guess::Waiting);
    assert!(!r.incomplete);
    let state = r
        .agents
        .iter()
        .find(|a| a.provider == "claude-code")
        .expect("agent");
    assert_eq!((state.state, state.sessions), (LogsState::Read, 1));
}

#[test]
fn claude_title_falls_back_to_the_first_real_prompt() {
    let f = fixture();
    write(
        &claude_file(&f, "p", CC_ID),
        &[
            r#"{"type":"user","isMeta":true,"message":{"content":"meta text"}}"#,
            r#"{"type":"user","message":{"content":"<command-name>/clear</command-name>"}}"#,
            r#"{"type":"user","isSidechain":true,"message":{"content":"sub agent prompt"}}"#,
            r#"{"type":"user","message":{"content":[{"type":"image"},{"type":"text","text":"  \n Refactor\u202e the   parser \n second line"}]}}"#,
        ],
        Duration::from_secs(10),
    );
    let s = only(&scan(&f.roots, &Limits::default(), now()), "claude-code");
    assert_eq!(s[0].title.as_deref(), Some("Refactor the parser"));
    assert_eq!(s[0].project_path, None);
    assert_eq!(s[0].guess, Guess::Working, "recent and the turn not ended");
}

#[test]
fn claude_pending_tool_call_quiet_for_a_minute_may_need_you() {
    let f = fixture();
    let lines = [
        r#"{"type":"user","message":{"content":"Deploy it"}}"#,
        r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t9","name":"Bash"}],"stop_reason":"tool_use"}}"#,
    ];
    write(
        &claude_file(&f, "p", CC_ID),
        &lines,
        Duration::from_secs(60),
    );
    let s = only(&scan(&f.roots, &Limits::default(), now()), "claude-code");
    assert_eq!(s[0].guess, Guess::MayNeedYou);
    // Same log, written 10 seconds ago: still working.
    write(
        &claude_file(&f, "p", CC_ID),
        &lines,
        Duration::from_secs(10),
    );
    let s = only(&scan(&f.roots, &Limits::default(), now()), "claude-code");
    assert_eq!(s[0].guess, Guess::Working);
    // Abandoned for two days: unknown.
    write(
        &claude_file(&f, "p", CC_ID),
        &lines,
        Duration::from_secs(2 * 86_400),
    );
    let s = only(&scan(&f.roots, &Limits::default(), now()), "claude-code");
    assert_eq!(s[0].guess, Guess::Unknown);
}

#[test]
fn malformed_huge_and_truncated_lines_are_skipped() {
    let f = fixture();
    let huge = format!(
        r#"{{"type":"custom-title","customTitle":"{}"}}"#,
        "x".repeat(400 * 1024)
    );
    let path = claude_file(&f, "p", CC_ID);
    write(
        &path,
        &[
            "not json at all",
            r#"{"type":"user","message":{"content":"Real prompt"}}"#,
            "{\"type\":\"user\",",
            &huge,
            "[1,2,3]",
            r#"{"type":"assistant","message":{"content":[{"type":"text"}],"stop_reason":"end_turn"}}"#,
        ],
        Duration::from_secs(300),
    );
    // Cut the last line in half: a log being written.
    let mut body = fs::read(&path).expect("read");
    body.extend_from_slice(br#"{"type":"user","message":{"content":"half"#);
    fs::write(&path, body).expect("write");
    let s = only(&scan(&f.roots, &Limits::default(), now()), "claude-code");
    assert_eq!(s.len(), 1);
    assert_eq!(
        s[0].title.as_deref(),
        Some("Real prompt"),
        "the huge title line never fits a window"
    );
}

#[test]
fn only_jsonl_files_with_plain_ids_count() {
    let f = fixture();
    write(&claude_file(&f, "p", "bad id"), &["{}"], Duration::ZERO);
    write(&claude_file(&f, "p", ".hidden"), &["{}"], Duration::ZERO);
    let sub = f
        .roots
        .claude_projects
        .as_ref()
        .expect("root")
        .join("p")
        .join(CC_ID)
        .join("subagents")
        .join("agent-1.jsonl");
    write(&sub, &["{}"], Duration::ZERO);
    let r = scan(&f.roots, &Limits::default(), now());
    assert!(only(&r, "claude-code").is_empty());
}

#[cfg(unix)]
fn link_file(target: &Path, link: &Path) -> bool {
    std::os::unix::fs::symlink(target, link).is_ok()
}

#[cfg(windows)]
fn link_file(target: &Path, link: &Path) -> bool {
    // Needs Developer Mode or elevation; the test notes and skips when it can't make one.
    std::os::windows::fs::symlink_file(target, link).is_ok()
}

#[cfg(unix)]
fn link_dir(target: &Path, link: &Path) -> bool {
    std::os::unix::fs::symlink(target, link).is_ok()
}

#[cfg(windows)]
fn link_dir(target: &Path, link: &Path) -> bool {
    std::os::windows::fs::symlink_dir(target, link).is_ok()
        || std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .output()
            .is_ok_and(|o| o.status.success())
}

#[test]
fn symlinks_and_junctions_are_never_followed() {
    let f = fixture();
    let outside = f.base.join("outside");
    let real = outside.join(format!("{CC_ID}.jsonl"));
    write(
        &real,
        &[r#"{"type":"user","message":{"content":"secret elsewhere"}}"#],
        Duration::ZERO,
    );
    let root = f.roots.claude_projects.clone().expect("root");
    fs::create_dir_all(root.join("p")).expect("mkdir");
    let file_link = link_file(&real, &root.join("p").join(format!("{CC_ID}.jsonl")));
    let dir_link = link_dir(&outside, &root.join("linked"));
    if !file_link && !dir_link {
        eprintln!("note: could not create a symlink or junction here; test skipped");
        return;
    }
    let r = scan(&f.roots, &Limits::default(), now());
    assert!(
        only(&r, "claude-code").is_empty(),
        "nothing behind a link is read (file link: {file_link}, folder link: {dir_link})"
    );
}

#[test]
fn caps_keep_the_newest_and_say_the_list_is_incomplete() {
    let f = fixture();
    for (i, age) in [(1, 30), (2, 20), (3, 10)] {
        let id = format!("00000000-0000-4000-8000-00000000000{i}");
        write(
            &claude_file(&f, "p", &id),
            &[r#"{"type":"user","message":{"content":"x"}}"#],
            Duration::from_secs(age),
        );
    }
    let limits = Limits {
        max_sessions: 2,
        ..Limits::default()
    };
    let r = scan(&f.roots, &limits, now());
    let ids: Vec<_> = r.sessions.iter().map(|s| s.session_id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "00000000-0000-4000-8000-000000000003",
            "00000000-0000-4000-8000-000000000002"
        ]
    );
    assert!(r.incomplete);

    let few = Limits {
        max_entries: 2,
        ..Limits::default()
    };
    assert!(scan(&f.roots, &few, now()).incomplete);
    let late = Limits {
        deadline: Duration::ZERO,
        ..Limits::default()
    };
    let r = scan(&f.roots, &late, now());
    assert!(r.incomplete);
    assert!(r.sessions.is_empty());
}

#[test]
fn reads_only_bounded_windows_of_a_large_log() {
    let f = fixture();
    let path = claude_file(&f, "p", CC_ID);
    let filler = format!(
        r#"{{"type":"assistant","message":{{"content":[{{"type":"text","text":"{}"}}]}}}}"#,
        "y".repeat(1000)
    );
    let mut lines =
        vec![r#"{"type":"user","cwd":"/home/u/big","message":{"content":"Big task"}}"#.to_owned()];
    // ~4 MB in the middle, then a custom title only in the middle (never read) and an end.
    for _ in 0..2_000 {
        lines.push(filler.clone());
    }
    lines.push(r#"{"type":"custom-title","customTitle":"Only in the middle"}"#.to_owned());
    for _ in 0..2_000 {
        lines.push(filler.clone());
    }
    lines.push(r#"{"type":"system","subtype":"turn_duration"}"#.to_owned());
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    write(&path, &refs, Duration::from_secs(5));
    let s = only(&scan(&f.roots, &Limits::default(), now()), "claude-code");
    assert_eq!(
        s[0].title.as_deref(),
        Some("Big task"),
        "the middle is never read"
    );
    assert_eq!(s[0].project_path.as_deref(), Some("/home/u/big"));
    assert_eq!(s[0].guess, Guess::Waiting);
}

// ---- Cursor ---------------------------------------------------------------------------------

fn cursor_file(f: &Fixture, project: &str, id: &str) -> PathBuf {
    f.roots
        .cursor_projects
        .as_ref()
        .expect("root")
        .join(project)
        .join("agent-transcripts")
        .join(id)
        .join(format!("{id}.jsonl"))
}

#[test]
fn cursor_session_uses_the_user_query_and_turn_end() {
    let f = fixture();
    write(
        &cursor_file(&f, "d-work-shop", CU_ID),
        &[
            r#"{"role":"user","message":{"content":[{"type":"text","text":"<timestamp>Friday</timestamp>\n<user_query>\nAdd dark mode\n</user_query>"}]}}"#,
            r#"{"role":"assistant","message":{"content":[{"type":"tool_use"}]}}"#,
            r#"{"role":"assistant","message":{"content":[{"type":"text","text":"ok"}]}}"#,
            r#"{"type":"turn_ended","status":"success"}"#,
        ],
        Duration::from_secs(400),
    );
    let s = only(&scan(&f.roots, &Limits::default(), now()), "cursor");
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].session_id, CU_ID);
    assert_eq!(s[0].title.as_deref(), Some("Add dark mode"));
    assert_eq!(s[0].project_path.as_deref(), Some("d-work-shop"));
    assert_eq!(s[0].guess, Guess::Waiting);
}

#[test]
fn cursor_trailing_tool_call_is_never_may_need_you() {
    let f = fixture();
    write(
        &cursor_file(&f, "p", CU_ID),
        &[
            r#"{"role":"user","message":{"content":[{"type":"text","text":"Go"}]}}"#,
            r#"{"role":"assistant","message":{"content":[{"type":"tool_use"}]}}"#,
        ],
        Duration::from_secs(90),
    );
    let s = only(&scan(&f.roots, &Limits::default(), now()), "cursor");
    assert_eq!(s[0].guess, Guess::Working);
    write(
        &cursor_file(&f, "p", CU_ID),
        &[r#"{"role":"assistant","message":{"content":[{"type":"tool_use"}]}}"#],
        Duration::from_secs(600),
    );
    let s = only(&scan(&f.roots, &Limits::default(), now()), "cursor");
    assert_eq!(s[0].guess, Guess::Unknown);
}

#[test]
fn cursor_needs_the_file_named_like_its_folder() {
    let f = fixture();
    let wrong = f
        .roots
        .cursor_projects
        .as_ref()
        .expect("root")
        .join("p")
        .join("agent-transcripts")
        .join(CU_ID)
        .join("other.jsonl");
    write(&wrong, &["{}"], Duration::ZERO);
    assert!(only(&scan(&f.roots, &Limits::default(), now()), "cursor").is_empty());
}

// ---- Codex ----------------------------------------------------------------------------------

fn codex_file(f: &Fixture, id: &str) -> PathBuf {
    f.roots
        .codex_home
        .as_ref()
        .expect("root")
        .join("sessions")
        .join("2026")
        .join("11")
        .join("07")
        .join(format!("rollout-2026-11-07T09-00-00-{id}.jsonl"))
}

fn meta(id: &str) -> String {
    format!(
        r#"{{"timestamp":"2026-11-07T09:00:00Z","type":"session_meta","payload":{{"id":"{id}","session_id":"{id}","cwd":"/home/u/shop","base_instructions":{{"text":"long"}}}}}}"#
    )
}

#[test]
fn codex_session_with_thread_name_and_pending_call() {
    let f = fixture();
    let m = meta(CX_ID);
    write(
        &codex_file(&f, CX_ID),
        &[
            &m,
            r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<environment_context>cwd</environment_context>"}]}}"#,
            r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Speed up the build"}]}}"#,
            r#"{"type":"event_msg","payload":{"type":"task_started","turn_id":"1"}}"#,
            r#"{"type":"response_item","payload":{"type":"function_call","call_id":"c1","name":"shell"}}"#,
        ],
        Duration::from_secs(45),
    );
    let s = only(&scan(&f.roots, &Limits::default(), now()), "codex");
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].session_id, CX_ID);
    assert_eq!(s[0].title.as_deref(), Some("Speed up the build"));
    assert_eq!(s[0].project_path.as_deref(), Some("/home/u/shop"));
    assert_eq!(s[0].guess, Guess::MayNeedYou);

    let index = f
        .roots
        .codex_home
        .as_ref()
        .expect("home")
        .join("session_index.jsonl");
    write(
        &index,
        &[
            &format!(r#"{{"id":"{CX_ID}","thread_name":"First name","updated_at":"x"}}"#),
            "garbage",
            &format!(r#"{{"id":"{CX_ID}","thread_name":"Build speed","updated_at":"y"}}"#),
        ],
        Duration::ZERO,
    );
    let s = only(&scan(&f.roots, &Limits::default(), now()), "codex");
    assert_eq!(s[0].title.as_deref(), Some("Build speed"));
}

#[test]
fn codex_turn_complete_waits_and_a_mismatched_meta_is_dropped() {
    let f = fixture();
    let m = meta(CX_ID);
    write(
        &codex_file(&f, CX_ID),
        &[
            &m,
            r#"{"type":"response_item","payload":{"type":"function_call","call_id":"c1"}}"#,
            r#"{"type":"response_item","payload":{"type":"function_call_output","call_id":"c1"}}"#,
            r#"{"type":"event_msg","payload":{"type":"task_complete"}}"#,
        ],
        Duration::from_secs(900),
    );
    let s = only(&scan(&f.roots, &Limits::default(), now()), "codex");
    assert_eq!(s[0].guess, Guess::Waiting);

    let other = "019db995-c01a-7b2e-9f3c-000000000000";
    let m = meta(other);
    write(&codex_file(&f, CX_ID), &[&m], Duration::ZERO);
    assert!(only(&scan(&f.roots, &Limits::default(), now()), "codex").is_empty());
}

// ---- OpenCode -------------------------------------------------------------------------------

/// `(id, parent_id, title, time_updated ms, time_archived)`.
type OcRow<'a> = (&'a str, Option<&'a str>, &'a str, i64, Option<i64>);

fn opencode_db(f: &Fixture, rows: &[OcRow<'_>]) {
    let path = f.roots.opencode_db.clone().expect("db");
    fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    let conn = rusqlite::Connection::open(&path).expect("db");
    conn.execute_batch(
        "CREATE TABLE session (id text PRIMARY KEY, project_id text NOT NULL, parent_id text,
           slug text NOT NULL, directory text NOT NULL, title text NOT NULL, version text NOT NULL,
           time_created integer NOT NULL, time_updated integer NOT NULL, time_archived integer)",
    )
    .expect("schema");
    for (id, parent, title, updated, archived) in rows {
        conn.execute(
            "INSERT INTO session VALUES (?1, 'p', ?2, 's', '/home/u/oc', ?3, '2', 0, ?4, ?5)",
            rusqlite::params![id, parent, title, updated, archived],
        )
        .expect("row");
    }
}

#[test]
fn opencode_reads_top_level_sessions_read_only() {
    let f = fixture();
    let ms = |t: OffsetDateTime| i64::try_from(t.unix_timestamp_nanos() / 1_000_000).expect("ms");
    let n = now();
    opencode_db(
        &f,
        &[
            (
                "ses_recent",
                None,
                "Port the CLI",
                ms(n - time::Duration::seconds(30)),
                None,
            ),
            (
                "ses_old",
                None,
                "Old work",
                ms(n - time::Duration::hours(3)),
                None,
            ),
            ("ses_child", Some("ses_recent"), "helper", ms(n), None),
            ("ses_archived", None, "gone", ms(n), Some(1)),
            ("bad id!", None, "x", ms(n), None),
        ],
    );
    let r = scan(&f.roots, &Limits::default(), n);
    let s = only(&r, "opencode");
    let ids: Vec<_> = s.iter().map(|s| s.session_id.as_str()).collect();
    assert_eq!(ids, ["ses_recent", "ses_old"]);
    assert_eq!(s[0].title.as_deref(), Some("Port the CLI"));
    assert_eq!(s[0].project_path.as_deref(), Some("/home/u/oc"));
    assert_eq!(s[0].guess, Guess::Working);
    assert_eq!(s[1].guess, Guess::Unknown);
}

#[test]
fn opencode_with_an_unknown_schema_is_unreadable_not_an_error() {
    let f = fixture();
    let path = f.roots.opencode_db.clone().expect("db");
    fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    rusqlite::Connection::open(&path)
        .expect("db")
        .execute_batch("CREATE TABLE other (x)")
        .expect("schema");
    let r = scan(&f.roots, &Limits::default(), now());
    let oc = r
        .agents
        .iter()
        .find(|a| a.provider == "opencode")
        .expect("agent");
    assert_eq!(oc.state, LogsState::Unreadable);
}

// ---- shared ---------------------------------------------------------------------------------

#[test]
fn missing_folders_report_not_found() {
    let f = fixture();
    let r = scan(&f.roots, &Limits::default(), now());
    assert!(r.sessions.is_empty());
    assert_eq!(r.agents.len(), 4);
    assert!(r.agents.iter().all(|a| a.state == LogsState::NotFound));
    let none = scan(&Roots::default(), &Limits::default(), now());
    assert!(none.agents.iter().all(|a| a.state == LogsState::NotFound));
}

/// The id a scan reports is exactly the one each adapter maps from that agent's hook, so a
/// later hook event lands on the imported session.
#[test]
fn ids_match_what_the_adapters_map_from_hooks() {
    let claude = crate::claude_code::map_hook(
        format!(r#"{{"session_id":"{CC_ID}","hook_event_name":"Stop"}}"#).as_bytes(),
    )
    .expect("claude");
    assert_eq!(claude.provider_session_id, CC_ID);
    let cursor = crate::cursor::map_hook(
        format!(r#"{{"conversation_id":"{CU_ID}","hook_event_name":"stop"}}"#).as_bytes(),
    )
    .expect("cursor");
    assert_eq!(cursor.provider_session_id, CU_ID);
    let codex = crate::codex::map_hook(
        format!(r#"{{"session_id":"{CX_ID}","hook_event_name":"Stop"}}"#).as_bytes(),
    )
    .expect("codex");
    assert_eq!(codex.provider_session_id, CX_ID);
    let oc = crate::opencode::map_hook(br#"{"v":1,"event":"turn_ended","sessionId":"ses_recent"}"#)
        .expect("opencode");
    assert_eq!(oc.provider_session_id, "ses_recent");

    // And the scan reports those same strings.
    let f = fixture();
    write(
        &claude_file(&f, "p", CC_ID),
        &[r#"{"type":"user","message":{"content":"a"}}"#],
        Duration::ZERO,
    );
    write(
        &cursor_file(&f, "p", CU_ID),
        &[r#"{"role":"user","message":{"content":[{"type":"text","text":"a"}]}}"#],
        Duration::ZERO,
    );
    let m = meta(CX_ID);
    write(&codex_file(&f, CX_ID), &[&m], Duration::ZERO);
    let r = scan(&f.roots, &Limits::default(), now());
    for (p, id) in [("claude-code", CC_ID), ("cursor", CU_ID), ("codex", CX_ID)] {
        assert_eq!(only(&r, p)[0].session_id, id, "{p}");
    }
}

#[test]
fn titles_are_cleaned_and_cut() {
    assert_eq!(clean_title("a\u{200b}b\u{202e}c"), Some("abc".into()));
    assert_eq!(clean_title("  \n\t "), None);
    let long = "word ".repeat(40);
    let t = clean_title(&long).expect("title");
    assert_eq!(t.chars().count(), MAX_TITLE_CHARS);
    assert!(t.ends_with('…'));
    let secret = clean_title("use key sk-abcdef1234567890 now").expect("title");
    assert!(!secret.contains("sk-abcdef"), "secret-like text is hidden");
    assert_eq!(prompt_title("<command-name>x</command-name>"), None);
    assert_eq!(clean_path("C:\\a\u{202e}b\n"), Some("C:\\ab".into()));
}

#[test]
fn session_ids_are_plain() {
    for ok in [CC_ID, "ses_01ABC", "a.b-c_d"] {
        assert!(is_session_id(ok), "{ok}");
    }
    for bad in ["", ".x", "a/b", "a\\b", "a b", "é", &"x".repeat(129)] {
        assert!(!is_session_id(bad), "{bad}");
    }
}

#[test]
fn guess_rules() {
    let s = Duration::from_secs;
    assert_eq!(guess(true, true, s(5), true), Guess::Waiting);
    assert_eq!(guess(false, true, s(31), true), Guess::MayNeedYou);
    assert_eq!(guess(false, true, s(31), false), Guess::Working);
    assert_eq!(guess(false, true, s(20), true), Guess::Working);
    assert_eq!(guess(false, false, s(119), true), Guess::Working);
    assert_eq!(guess(false, false, s(121), true), Guess::Unknown);
    assert_eq!(guess(false, true, s(90_000), true), Guess::Unknown);
}

/// Manual: scans this machine's real agent logs (read-only) and prints only counts, agents and
/// the status spread; never ids, titles, paths or content. Run with
/// `cargo test -p agent-adapters --lib real_machine_scan -- --ignored --nocapture`.
#[test]
#[ignore = "reads the real logs of the machine it runs on"]
fn real_machine_scan() {
    let started = std::time::Instant::now();
    let r = scan(&Roots::from_env(), &Limits::default(), now());
    let took = started.elapsed();
    println!(
        "scan took {took:?}; sessions {}; incomplete {}",
        r.sessions.len(),
        r.incomplete
    );
    for a in &r.agents {
        let mut spread = std::collections::BTreeMap::new();
        for s in r.sessions.iter().filter(|s| s.provider == a.provider) {
            *spread.entry(s.guess.as_str()).or_insert(0) += 1;
        }
        let titled = r
            .sessions
            .iter()
            .filter(|s| s.provider == a.provider && s.title.is_some())
            .count();
        let with_project = r
            .sessions
            .iter()
            .filter(|s| s.provider == a.provider && s.project_path.is_some())
            .count();
        println!(
            "{}: {:?}, {} sessions, {titled} with a title, {with_project} with a project, {spread:?}",
            a.provider, a.state, a.sessions
        );
    }
}
