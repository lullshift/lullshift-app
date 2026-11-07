//! Companion command tests: a local mock provider, an in-memory key store, the real store.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use ::companion::{ErrorKind, MemoryKeyStore};
use agent_core::parse_event;

use super::*;

const KEY: &str = "sk-test-DESKTOPSECRET-0123456789";
const NOTES: &str = "NOTES-SENTINEL remember the token expiry";

#[derive(Clone, Copy, PartialEq)]
enum Sc {
    Ok,
    Status(u16),
    Echo,
    Slow,
    /// An SSE body (OpenAI shape), then the connection closes.
    Sse(&'static str),
    /// The first part of an SSE body, a pause, then the second part.
    SseLate(&'static str, u64, &'static str),
}

/// One OpenAI-shaped chunk with text.
const OA_TEXT: &str = "data: {\"choices\":[{\"delta\":{\"content\":\"streamed reply\"}}]}\n\ndata: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: {\"choices\":[],\"usage\":{\"prompt_tokens\":40,\"completion_tokens\":7}}\n\ndata: [DONE]\n\n";

fn sse_head(s: &mut TcpStream) {
    let _ = s.write_all(
        b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n",
    );
}

/// An OpenAI-shaped stream with several tool calls in one reply.
fn oa_calls(calls: &[(String, &str, &str)]) -> String {
    let parts: Vec<String> = calls
        .iter()
        .enumerate()
        .map(|(i, (id, name, args))| {
            let args = args.replace('\\', "\\\\").replace('"', "\\\"");
            format!(
                "{{\"index\":{i},\"id\":\"{id}\",\"type\":\"function\",\"function\":{{\"name\":\"{name}\",\"arguments\":\"{args}\"}}}}"
            )
        })
        .collect();
    format!(
        "data: {{\"choices\":[{{\"delta\":{{\"tool_calls\":[{}]}}}}]}}\n\ndata: {{\"choices\":[{{\"delta\":{{}},\"finish_reason\":\"tool_calls\"}}]}}\n\ndata: {{\"choices\":[],\"usage\":{{\"prompt_tokens\":30,\"completion_tokens\":4}}}}\n\ndata: [DONE]\n\n",
        parts.join(",")
    )
}

/// An OpenAI-shaped tool call stream.
fn oa_call(id: &str, name: &str, args: &str) -> String {
    let args = args.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        "data: {{\"choices\":[{{\"delta\":{{\"tool_calls\":[{{\"index\":0,\"id\":\"{id}\",\"type\":\"function\",\"function\":{{\"name\":\"{name}\",\"arguments\":\"{args}\"}}}}]}}}}]}}\n\ndata: {{\"choices\":[{{\"delta\":{{}},\"finish_reason\":\"tool_calls\"}}]}}\n\ndata: {{\"choices\":[],\"usage\":{{\"prompt_tokens\":30,\"completion_tokens\":4}}}}\n\ndata: [DONE]\n\n"
    )
}

struct Mock {
    base: String,
    requests: Arc<Mutex<Vec<String>>>,
}

fn read_request(s: &mut TcpStream) -> Option<String> {
    s.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let n = s.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&buf[..pos]).to_ascii_lowercase();
            let len = head
                .lines()
                .find_map(|l| l.strip_prefix("content-length:"))
                .and_then(|v| v.trim().parse::<usize>().ok())
                .unwrap_or(0);
            while buf.len() < pos + 4 + len {
                let n = s.read(&mut chunk).ok()?;
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
            }
            return Some(String::from_utf8_lossy(&buf).into_owned());
        }
    }
}

fn respond(s: &mut TcpStream, status: &str, body: &str) {
    let head = format!(
        "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        body.len()
    );
    let _ = s.write_all(head.as_bytes());
    let _ = s.write_all(body.as_bytes());
}

/// Answers request n with `script[n]`; the last entry repeats. Owned bodies (built at run time)
/// are given through `mock_sse`.
fn mock(script: Vec<Sc>) -> Mock {
    mock_with(script, Vec::new())
}

/// Like [`mock`], but request n is answered with the SSE body `bodies[n]` when there is one.
fn mock_sse(bodies: Vec<String>) -> Mock {
    mock_with(vec![Sc::Ok], bodies)
}

fn mock_with(script: Vec<Sc>, bodies: Vec<String>) -> Mock {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&requests);
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(mut s) = conn else { continue };
            let Some(req) = read_request(&mut s) else {
                continue;
            };
            let n = {
                let mut l = log.lock().expect("log");
                l.push(req.clone());
                l.len() - 1
            };
            let sc = script[n.min(script.len() - 1)];
            let owned = bodies.get(n).cloned();
            std::thread::spawn(move || match sc {
                _ if owned.is_some() => {
                    sse_head(&mut s);
                    let _ = s.write_all(owned.unwrap_or_default().as_bytes());
                }
                Sc::Sse(b) => {
                    sse_head(&mut s);
                    let _ = s.write_all(b.as_bytes());
                }
                Sc::SseLate(a, ms, b) => {
                    sse_head(&mut s);
                    let _ = s.write_all(a.as_bytes());
                    let _ = s.flush();
                    std::thread::sleep(Duration::from_millis(ms));
                    let _ = s.write_all(b.as_bytes());
                }
                Sc::Ok => respond(
                    &mut s,
                    "200 OK",
                    r#"{"choices":[{"message":{"content":"a reply"}}],"usage":{"prompt_tokens":10,"completion_tokens":5}}"#,
                ),
                Sc::Status(c) => {
                    respond(&mut s, &format!("{c} X"), r#"{"error":"PROVIDER-BODY"}"#);
                }
                Sc::Echo => {
                    let echoed: String = req
                        .lines()
                        .filter(|l| l.to_ascii_lowercase().starts_with("authorization:"))
                        .collect();
                    respond(
                        &mut s,
                        "400 Bad Request",
                        &serde_json::json!({ "error": echoed }).to_string(),
                    );
                }
                Sc::Slow => {
                    std::thread::sleep(Duration::from_secs(4));
                    respond(&mut s, "200 OK", "{}");
                }
            });
        }
    });
    Mock {
        base: format!("http://127.0.0.1:{port}/v1"),
        requests,
    }
}

fn core_with(dir: &std::path::Path, keys: Arc<dyn KeyStore>, timeout_ms: u64) -> Core {
    let mut core = Core::for_tests(dir);
    core.companion = CompanionState::with(
        keys,
        HttpConfig {
            timeout: Duration::from_millis(timeout_ms),
            first_byte_timeout: Duration::from_millis(timeout_ms),
            idle_timeout: Duration::from_millis(timeout_ms),
            total_timeout: Duration::from_millis(timeout_ms.max(10_000)),
            ..HttpConfig::default()
        },
    );
    core
}

fn memory() -> Arc<dyn KeyStore> {
    Arc::new(MemoryKeyStore::new())
}

fn input(kind: &str, url: Option<&str>, model: &str, budget: Option<i64>) -> ProfileInput {
    ProfileInput {
        id: None,
        kind: kind.into(),
        label: "L".into(),
        base_url: url.map(str::to_owned),
        model: model.into(),
        monthly_token_budget: budget,
        is_default: false,
    }
}

/// A compatible-server profile pointing at the mock, with the key saved.
fn profile(core: &Core, base: &str, budget: Option<i64>) -> String {
    let p = save_profile(
        core,
        input("openaiCompatible", Some(base), "mock-model", budget),
    )
    .expect("profile");
    set_key(core, &p.id, KEY.to_owned()).expect("key");
    p.id
}

fn seed_session(core: &Core) {
    let e = parse_event(
        r#"{"schemaVersion":1,"eventId":"00000000-0000-4000-8000-000000000001","provider":"claude-code","providerSessionId":"s1","eventType":"needs_attention","timestampUtc":"2026-11-05T10:00:00Z"}"#,
    )
    .expect("event");
    let now = OffsetDateTime::now_utc();
    with_store(core, |s| {
        s.ingest(&e, now)?;
        s.set_user_title("claude-code", "s1", "Fix login", now)?;
        s.set_user_intent("claude-code", "s1", "Make login work", now)?;
        s.set_notes("claude-code", "s1", NOTES, now)?;
        s.add_item("claude-code", "s1", "checklist", None, "ITEM-ONE", now)?;
        s.add_item("claude-code", "s1", "decision", None, "ITEM-TWO", now)?;
        Ok(())
    })
    .expect("seed");
}

fn item_ids(core: &Core) -> Vec<i64> {
    with_store(core, |s| s.items("claude-code", "s1"))
        .expect("items")
        .iter()
        .map(|i| i.id)
        .collect()
}

fn id_of(core: &Core, text: &str) -> i64 {
    with_store(core, |s| s.items("claude-code", "s1"))
        .expect("items")
        .iter()
        .find(|i| i.text == text)
        .expect("item")
        .id
}

fn summary() -> Scope {
    Scope::SessionSummary {
        provider: "claude-code".into(),
        session_id: "s1".into(),
    }
}

fn selected(notes: bool, item_ids: Vec<i64>) -> Scope {
    Scope::SelectedNotes {
        provider: "claude-code".into(),
        session_id: "s1".into(),
        notes,
        item_ids,
    }
}

fn thread(core: &Core) -> i64 {
    new_thread(core, None).expect("thread").id
}

fn kind(e: &CommandError) -> &'static str {
    e.kind
}

/// What the UI does: send with the hash of the preview it just showed (none for scope `none`).
fn send(core: &Core, t: i64, pid: &str, text: &str, scope: &Scope) -> Res<MessageDto> {
    send_events(core, t, pid, text, scope, &|_| {})
}

fn next_request_id() -> String {
    static N: AtomicU64 = AtomicU64::new(1);
    format!("test-req-{}", N.fetch_add(1, Ordering::SeqCst))
}

/// Like `send`, with a handler for the stream events.
fn send_events(
    core: &Core,
    t: i64,
    pid: &str,
    text: &str,
    scope: &Scope,
    emit: Emit<'_>,
) -> Res<MessageDto> {
    let sha = preview_context(core, scope.clone())
        .ok()
        .and_then(|p| p.sha256);
    super::send(
        core,
        t,
        pid,
        text,
        scope,
        sha.as_deref(),
        &next_request_id(),
        emit,
    )
}

fn raw_send(
    core: &Core,
    t: i64,
    pid: &str,
    text: &str,
    scope: &Scope,
    hash: Option<&str>,
) -> Res<MessageDto> {
    super::send(core, t, pid, text, scope, hash, &next_request_id(), &|_| {})
}

fn retry(core: &Core, id: i64) -> Res<MessageDto> {
    super::retry(core, id, &next_request_id(), &|_| {})
}

/// Points an existing compatible profile at another base URL.
fn repoint(core: &Core, id: &str, base: &str) {
    let mut p = input("openaiCompatible", Some(base), "mock-model", None);
    p.id = Some(id.to_owned());
    save_profile(core, p).expect("repoint");
}

fn used_tokens(core: &Core) -> i64 {
    usage(core)
        .expect("usage")
        .iter()
        .map(|u| u.input_tokens + u.output_tokens)
        .sum()
}

fn last_body(m: &Mock) -> serde_json::Value {
    let reqs = m.requests.lock().expect("log");
    let (_, body) = reqs
        .last()
        .expect("request")
        .split_once("\r\n\r\n")
        .expect("body");
    serde_json::from_str(body).expect("json")
}

// -- scopes

#[test]
fn none_attaches_nothing_and_preview_is_empty() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 2000);
    seed_session(&core);
    let p = preview_context(&core, Scope::None).expect("preview");
    assert_eq!((p.text.as_str(), p.chars), ("", 0));
    let m = mock(vec![Sc::Ok]);
    let pid = profile(&core, &m.base, None);
    let t = thread(&core);
    let reply = send(&core, t, &pid, "hi", &Scope::None).expect("send");
    assert_eq!(reply.role, "assistant");
    let msgs = messages(&core, t).expect("msgs");
    assert_eq!(msgs[0].attached_context, None);
    assert_eq!(msgs[0].scope_kind, "none");
    assert_eq!(last_body(&m)["messages"][1]["content"], "hi");
}

#[test]
fn preview_and_send_produce_identical_text_for_every_scope() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 2000);
    seed_session(&core);
    let ids = item_ids(&core);
    let m = mock(vec![Sc::Ok]);
    let pid = profile(&core, &m.base, None);
    let scopes = [
        summary(),
        selected(true, vec![]),
        selected(false, vec![id_of(&core, "ITEM-TWO")]),
        selected(true, ids.clone()),
    ];
    for scope in scopes {
        let preview = preview_context(&core, scope.clone()).expect("preview");
        assert!(!preview.text.is_empty());
        assert_eq!(preview.chars, preview.text.chars().count());
        let t = thread(&core);
        send(&core, t, &pid, "question", &scope).expect("send");
        let user = &messages(&core, t).expect("msgs")[0];
        assert_eq!(
            user.attached_context.as_deref(),
            Some(preview.text.as_str())
        );
        assert_eq!(user.scope_kind, scope.kind_str());
        // The provider received exactly that text, then the user's words.
        let sent = last_body(&m);
        assert_eq!(
            sent["messages"][1]["content"].as_str().expect("content"),
            format!("{}\n\n---\n\nquestion", preview.text)
        );
    }
}

#[test]
fn summary_shows_title_status_intent_and_nothing_else() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 2000);
    seed_session(&core);
    let t = preview_context(&core, summary()).expect("preview").text;
    assert!(t.contains("Title: Fix login") && t.contains("Intent: Make login work"));
    assert!(t.contains("needs attention") && t.contains("as last reported"));
    assert!(!t.contains("NOTES-SENTINEL") && !t.contains("ITEM-ONE"));
}

#[test]
fn selected_notes_and_items_only_when_picked() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 2000);
    seed_session(&core);
    let t = preview_context(&core, selected(false, vec![id_of(&core, "ITEM-ONE")]))
        .expect("preview")
        .text;
    assert!(t.contains("ITEM-ONE") && !t.contains("ITEM-TWO") && !t.contains("NOTES-SENTINEL"));
    let t = preview_context(&core, selected(true, vec![]))
        .expect("preview")
        .text;
    assert!(t.contains("NOTES-SENTINEL") && !t.contains("ITEM-ONE"));
}

#[test]
fn bad_scopes_are_refused() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 2000);
    seed_session(&core);
    let k = |s: Scope| kind(&preview_context(&core, s).unwrap_err());
    assert_eq!(k(selected(false, vec![])), "invalidInput");
    assert_eq!(k(selected(false, vec![999_999])), "invalidInput");
    assert_eq!(
        k(Scope::SessionSummary {
            provider: "claude-code".into(),
            session_id: "nope".into()
        }),
        "notFound"
    );
    assert_eq!(
        k(Scope::SessionSummary {
            provider: String::new(),
            session_id: "s1".into()
        }),
        "invalidInput"
    );
}

#[test]
fn scope_json_matches_the_contract() {
    let s: Scope = serde_json::from_str(
        r#"{"kind":"selectedNotes","provider":"p","sessionId":"s","notes":true,"itemIds":[1,2]}"#,
    )
    .expect("parse");
    assert_eq!(s.kind_str(), "selectedNotes");
    let v = serde_json::to_value(summary()).expect("json");
    assert_eq!(v["kind"], "sessionSummary");
    assert_eq!(v["sessionId"], "s1");
    let n: Scope = serde_json::from_str(r#"{"kind":"none"}"#).expect("none");
    assert_eq!(n, Scope::None);
    assert!(serde_json::from_str::<Scope>(r#"{"kind":"everything"}"#).is_err());
}

// -- key handling

#[test]
fn key_never_reaches_sqlite_results_or_errors_across_a_full_flow() {
    let dir = tempfile::tempdir().expect("dir");
    let db = dir.path().join("l.db");
    let core = core_with(dir.path(), memory(), 2000);
    {
        let (store, _) = Store::open(&db).expect("open");
        *core.store.lock().expect("lock") = Some(store);
    }
    seed_session(&core);
    let ok = mock(vec![
        Sc::Ok,
        Sc::Ok,
        Sc::Echo,
        Sc::Status(500),
        Sc::Status(401),
        Sc::Status(429),
    ]);
    let pid = profile(&core, &ok.base, Some(1_000_000));
    let t = thread(&core);
    let mut shown = String::new();
    let mut note = |label: &str, v: String| shown.push_str(&format!("{label}={v}\n"));

    note(
        "profiles",
        serde_json::to_string(&profiles(&core).expect("p")).expect("j"),
    );
    note(
        "test",
        serde_json::to_string(&test_connection(&core, &pid).expect("t")).expect("j"),
    );
    let first = send(&core, t, &pid, "one", &summary()).expect("s");
    note("send", serde_json::to_string(&first).expect("j"));
    for text in ["two", "three", "four", "five"] {
        let e = send(&core, t, &pid, text, &summary()).unwrap_err();
        note("err", serde_json::to_string(&e).expect("j"));
    }
    let ms = messages(&core, t).expect("m");
    note("messages", serde_json::to_string(&ms).expect("j"));
    note(
        "threads",
        serde_json::to_string(&threads(&core).expect("t")).expect("j"),
    );
    note(
        "usage",
        serde_json::to_string(&usage(&core).expect("u")).expect("j"),
    );
    note(
        "preview",
        serde_json::to_string(&preview_context(&core, summary()).expect("p")).expect("j"),
    );
    // Bad key input must not be echoed either.
    let e = set_key(&core, &pid, format!("{KEY} with spaces")).unwrap_err();
    note("badkey", serde_json::to_string(&e).expect("j"));
    assert!(ms.iter().any(|m| m.status == "failed" && m.error.is_some()));
    assert!(ms.iter().any(|m| m.status == "sent"));

    // The mock did get the key, so the checks below mean something.
    assert!(ok.requests.lock().expect("log")[0].contains("DESKTOPSECRET"));
    assert!(!shown.contains("DESKTOPSECRET"), "results and errors");
    assert!(!shown.contains("PROVIDER-BODY"));

    // Close the store (checkpointing the WAL), then look at every file in the data folder.
    *core.store.lock().expect("lock") = None;
    let mut checked = Vec::new();
    for entry in std::fs::read_dir(dir.path()).expect("ls") {
        let path = entry.expect("entry").path();
        if path.is_file() {
            let bytes = std::fs::read(&path).expect("read");
            assert!(
                !bytes.windows(KEY.len()).any(|w| w == KEY.as_bytes()),
                "key found in {}",
                path.display()
            );
            assert!(
                !bytes.windows(10).any(|w| w == b"DESKTOPSEC"),
                "key fragment found in {}",
                path.display()
            );
            checked.push(
                path.file_name()
                    .expect("name")
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
    assert!(
        checked.iter().any(|n| n == "l.db"),
        "database checked: {checked:?}"
    );
}

#[test]
fn key_is_also_absent_from_the_database_while_the_wal_is_live() {
    let dir = tempfile::tempdir().expect("dir");
    let db = dir.path().join("l.db");
    let core = core_with(dir.path(), memory(), 2000);
    {
        let (store, _) = Store::open(&db).expect("open");
        *core.store.lock().expect("lock") = Some(store);
    }
    let m = mock(vec![Sc::Ok]);
    let pid = profile(&core, &m.base, None);
    let t = thread(&core);
    send(&core, t, &pid, "hi", &Scope::None).expect("send");
    // Store still open: the -wal file holds recent writes. Check it and the main file.
    for name in ["l.db", "l.db-wal", "l.db-shm"] {
        if let Ok(bytes) = std::fs::read(dir.path().join(name)) {
            assert!(
                !bytes.windows(10).any(|w| w == b"DESKTOPSEC"),
                "key fragment in {name}"
            );
        }
    }
    assert!(
        dir.path().join("l.db-wal").exists(),
        "the WAL was really in play"
    );
}

#[test]
fn nothing_in_this_module_logs() {
    let src = include_str!("../companion_commands.rs");
    let prod = src.split("#[cfg(test)]").next().expect("prod");
    for banned in [
        "println!",
        "eprintln!",
        "dbg!",
        "log::",
        "tracing::",
        "print!(",
    ] {
        assert!(!prod.contains(banned), "{banned}");
    }
}

#[test]
fn keychain_unavailable_is_reported_and_never_falls_back() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), Arc::new(MemoryKeyStore::unavailable()), 2000);
    let p = save_profile(&core, input("anthropic", None, "claude-x", None)).expect("profile");
    assert!(!p.has_key && !p.keychain_ok);
    assert_eq!(
        kind(&set_key(&core, &p.id, KEY.into()).unwrap_err()),
        "keychainUnavailable"
    );
    let t = thread(&core);
    assert_eq!(
        kind(&send(&core, t, &p.id, "hi", &Scope::None).unwrap_err()),
        "keychainUnavailable"
    );
    assert_eq!(
        kind(&delete_profile(&core, &p.id).unwrap_err()),
        "keychainUnavailable"
    );
    assert!(messages(&core, t).expect("m").is_empty());
}

#[test]
fn no_key_blocks_sending_without_storing() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 2000);
    let m = mock(vec![Sc::Ok]);
    let pid = profile(&core, &m.base, None);
    clear_key(&core, &pid).expect("clear");
    assert!(!profiles(&core).expect("p")[0].has_key);
    let t = thread(&core);
    assert_eq!(
        kind(&send(&core, t, &pid, "hi", &Scope::None).unwrap_err()),
        "noKey"
    );
    assert_eq!(kind(&test_connection(&core, &pid).unwrap_err()), "noKey");
    assert!(messages(&core, t).expect("m").is_empty());
    assert!(m.requests.lock().expect("log").is_empty(), "no network use");
}

#[test]
fn listing_and_reading_use_no_network() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 2000);
    let m = mock(vec![Sc::Ok]);
    profile(&core, &m.base, None);
    let p = profiles(&core).expect("p");
    assert!(p[0].has_key && p[0].keychain_ok && p[0].is_default);
    threads(&core).expect("t");
    usage(&core).expect("u");
    assert!(m.requests.lock().expect("log").is_empty());
}

#[test]
fn deleting_a_profile_deletes_its_key_and_usage_but_keeps_conversations() {
    let dir = tempfile::tempdir().expect("dir");
    let keys = Arc::new(MemoryKeyStore::new());
    let core = core_with(dir.path(), keys.clone(), 2000);
    let m = mock(vec![Sc::Ok]);
    let pid = profile(&core, &m.base, None);
    let t = thread(&core);
    send(&core, t, &pid, "hi", &Scope::None).expect("send");
    assert!(keys.has(&pid).expect("has"));
    delete_profile(&core, &pid).expect("delete");
    assert!(!keys.has(&pid).expect("has"));
    assert!(profiles(&core).expect("p").is_empty());
    assert_eq!(messages(&core, t).expect("m").len(), 2);
    assert!(usage(&core).expect("u").is_empty());
}

// -- profiles

#[test]
fn profile_validation() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 2000);
    for bad in [
        input("openaiCompatible", Some("http://example.com/v1"), "m", None),
        input(
            "openaiCompatible",
            Some("https://u:p@example.com"),
            "m",
            None,
        ),
        input("openaiCompatible", None, "m", None),
        input("openai", None, "bad model!", None),
        input("openai", None, "m", Some(0)),
        input("nonsense", None, "m", None),
    ] {
        assert_eq!(kind(&save_profile(&core, bad).unwrap_err()), "invalidInput");
    }
    // Fixed providers ignore a supplied URL.
    let p = save_profile(
        &core,
        input("anthropic", Some("https://evil.example"), "claude-x", None),
    )
    .expect("ok");
    assert_eq!(p.base_url, "https://api.anthropic.com");
    assert_eq!(p.host, "api.anthropic.com");
    assert!(p.is_default, "the first profile is the default");
    let local = save_profile(
        &core,
        input(
            "openaiCompatible",
            Some("http://localhost:11434/v1/"),
            "llama3:8b",
            Some(500),
        ),
    )
    .expect("ok");
    assert_eq!(local.base_url, "http://localhost:11434/v1");
    assert_eq!(local.host, "localhost:11434");
    assert!(!local.is_default);
    let mut upd = input("openai", None, "gpt-x", None);
    upd.id = Some("p-unknown".into());
    assert_eq!(kind(&save_profile(&core, upd).unwrap_err()), "notFound");
}

#[test]
fn only_one_default_profile() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 2000);
    let m = mock(vec![Sc::Ok]);
    let a = profile(&core, &m.base, None);
    let mut second = input("openai", None, "gpt-x", None);
    second.is_default = true;
    let b = save_profile(&core, second).expect("b");
    let list = profiles(&core).expect("p");
    assert_eq!(list.iter().filter(|p| p.is_default).count(), 1);
    assert!(list.iter().find(|p| p.id == b.id).expect("b").is_default);
    delete_profile(&core, &b.id).expect("delete default");
    assert!(
        profiles(&core)
            .expect("p")
            .iter()
            .find(|p| p.id == a)
            .expect("a")
            .is_default
    );
}

// -- budget, failures, retry

#[test]
fn budget_blocks_before_sending_and_is_per_profile() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 2000);
    let m = mock(vec![Sc::Ok]);
    let limited = profile(&core, &m.base, Some(20));
    let free = profile(&core, &m.base, None);
    let t = thread(&core);
    send(&core, t, &limited, "one", &Scope::None).expect("15 tokens used");
    send(&core, t, &limited, "two", &Scope::None).expect("starts under the limit");
    let sent_before = m.requests.lock().expect("log").len();
    let e = send(&core, t, &limited, "three", &Scope::None).unwrap_err();
    assert_eq!(kind(&e), "budgetReached");
    assert_eq!(
        m.requests.lock().expect("log").len(),
        sent_before,
        "nothing sent"
    );
    assert_eq!(
        messages(&core, t).expect("m").len(),
        4,
        "nothing stored either"
    );
    assert_eq!(
        kind(&test_connection(&core, &limited).unwrap_err()),
        "budgetReached"
    );
    send(&core, t, &free, "four", &Scope::None).expect("other profile unaffected");
    let u = usage(&core).expect("u");
    let l = u.iter().find(|u| u.profile_id == limited).expect("limited");
    assert_eq!(
        (l.input_tokens, l.output_tokens, l.budget),
        (20, 10, Some(20))
    );
    assert_eq!(l.month, month_of(OffsetDateTime::now_utc()));
}

#[test]
fn budget_counts_only_this_utc_month() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 2000);
    let m = mock(vec![Sc::Ok]);
    let pid = profile(&core, &m.base, Some(10));
    with_store(&core, |s| {
        s.companion_record_usage(
            &pid,
            OffsetDateTime::now_utc() - time::Duration::days(40),
            1000,
            1000,
        )
    })
    .expect("old usage");
    let t = thread(&core);
    send(&core, t, &pid, "hi", &Scope::None).expect("allowed");
    assert_eq!(
        kind(&send(&core, t, &pid, "again", &Scope::None).unwrap_err()),
        "budgetReached"
    );
}

#[test]
fn failed_messages_are_kept_and_retry_resends_the_same_context() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 2000);
    seed_session(&core);
    let m = mock(vec![Sc::Status(500), Sc::Ok]);
    let pid = profile(&core, &m.base, None);
    let t = thread(&core);
    let e = send(&core, t, &pid, "please help", &summary()).unwrap_err();
    assert_eq!(kind(&e), "providerError");
    let ms = messages(&core, t).expect("m");
    assert_eq!(ms.len(), 1);
    assert_eq!(ms[0].status, "failed");
    assert_eq!(ms[0].error.as_ref().expect("err").kind, "providerError");
    let ctx = ms[0].attached_context.clone().expect("context kept");
    let r = retry(&core, ms[0].id).expect("retry");
    assert_eq!(r.role, "assistant");
    let ms = messages(&core, t).expect("m");
    assert_eq!(
        (ms[0].status.as_str(), ms[0].error.is_none()),
        ("sent", true)
    );
    assert_eq!(ms.len(), 2);
    let reqs = m.requests.lock().expect("log").clone();
    assert_eq!(reqs.len(), 2);
    for r in &reqs {
        let (_, body) = r.split_once("\r\n\r\n").expect("body");
        let v: serde_json::Value = serde_json::from_str(body).expect("json");
        assert_eq!(
            v["messages"][1]["content"],
            format!("{ctx}\n\n---\n\nplease help")
        );
    }
    assert_eq!(kind(&retry(&core, ms[0].id).unwrap_err()), "invalidInput");
}

#[test]
fn only_the_latest_failed_message_can_be_retried() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 2000);
    let m = mock(vec![Sc::Status(500), Sc::Status(500), Sc::Ok]);
    let pid = profile(&core, &m.base, None);
    let t = thread(&core);
    send(&core, t, &pid, "first", &Scope::None).unwrap_err();
    send(&core, t, &pid, "second", &Scope::None).unwrap_err();
    let ms = messages(&core, t).expect("m");
    assert_eq!(kind(&retry(&core, ms[0].id).unwrap_err()), "invalidInput");
    retry(&core, ms[1].id).expect("latest retries");
}

#[test]
fn history_sends_delivered_text_only_and_never_old_context() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 2000);
    seed_session(&core);
    let m = mock(vec![Sc::Ok, Sc::Status(500), Sc::Ok]);
    let pid = profile(&core, &m.base, None);
    let t = thread(&core);
    send(&core, t, &pid, "first", &summary()).expect("1");
    send(&core, t, &pid, "failed one", &Scope::None).unwrap_err();
    send(&core, t, &pid, "third", &Scope::None).expect("3");
    let v = last_body(&m);
    let texts: Vec<String> = v["messages"]
        .as_array()
        .expect("arr")
        .iter()
        .map(|m| {
            format!(
                "{}:{}",
                m["role"].as_str().expect("r"),
                m["content"].as_str().expect("c")
            )
        })
        .collect();
    assert_eq!(texts[1], "user:first");
    assert_eq!(texts[2], "assistant:a reply");
    assert_eq!(texts[3], "user:third");
    assert_eq!(texts.len(), 4);
    assert!(
        !v.to_string().contains("Fix login"),
        "old context is not repeated"
    );
}

#[test]
fn errors_are_stored_with_fixed_messages_only() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 2000);
    let m = mock(vec![Sc::Echo]);
    let pid = profile(&core, &m.base, None);
    let t = thread(&core);
    let e = send(&core, t, &pid, "hi", &Scope::None).unwrap_err();
    assert!(
        !serde_json::to_string(&e)
            .expect("j")
            .contains("DESKTOPSECRET")
    );
    let ms = messages(&core, t).expect("m");
    let err = ms[0].error.as_ref().expect("error");
    assert!(!err.message.contains("DESKTOPSECRET") && !err.message.contains("Bearer"));
}

#[test]
fn timeout_marks_the_message_failed() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 300);
    let m = mock(vec![Sc::Slow]);
    let pid = profile(&core, &m.base, None);
    let t = thread(&core);
    assert_eq!(
        kind(&send(&core, t, &pid, "hi", &Scope::None).unwrap_err()),
        "timeout"
    );
    assert_eq!(messages(&core, t).expect("m")[0].status, "failed");
}

#[test]
fn busy_while_sending_and_cancel_stops_it() {
    let dir = tempfile::tempdir().expect("dir");
    let core = Arc::new(core_with(dir.path(), memory(), 20_000));
    let m = mock(vec![Sc::Slow]);
    let pid = profile(&core, &m.base, None);
    let t = thread(&core);
    let c2 = Arc::clone(&core);
    let pid2 = pid.clone();
    let first = std::thread::spawn(move || send(&c2, t, &pid2, "slow one", &Scope::None));
    let start = std::time::Instant::now();
    while m.requests.lock().expect("log").is_empty() {
        assert!(start.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(20));
    }
    let e = send(&core, t, &pid, "second", &Scope::None).unwrap_err();
    assert_eq!(kind(&e), "busy");
    let first_id = messages(&core, t).expect("m")[0].id;
    assert_eq!(kind(&retry(&core, first_id).unwrap_err()), "busy");
    assert_eq!(kind(&delete_thread(&core, t).unwrap_err()), "busy");
    // Another thread is not blocked by this one.
    let other = thread(&core);
    assert!(core.companion.begin(other, "test-request-id").is_ok());
    cancel(&core, t);
    let r = first.join().expect("join");
    assert_eq!(kind(&r.unwrap_err()), "cancelled");
    let ms = messages(&core, t).expect("m");
    assert_eq!(ms.len(), 1);
    assert_eq!(ms[0].status, "failed");
    assert_eq!(ms[0].error.as_ref().expect("e").kind, "cancelled");
}

#[test]
fn stale_pending_messages_become_retryable_after_a_restart() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 2000);
    let m = mock(vec![Sc::Ok]);
    let pid = profile(&core, &m.base, None);
    let t = thread(&core);
    with_store(&core, |s| {
        s.companion_add_user_message(
            &NewUserMessage {
                thread_id: t,
                text: "interrupted",
                attached_context: None,
                scope_kind: "none",
                profile_id: &pid,
                model: "mock-model",
                host: &m.base.replace("http://", "").replace("/v1", ""),
            },
            OffsetDateTime::now_utc(),
        )
    })
    .expect("pending");
    assert_eq!(messages(&core, t).expect("m")[0].status, "pending");
    recover_pending(&core);
    let ms = messages(&core, t).expect("m");
    assert_eq!(ms[0].status, "failed");
    retry(&core, ms[0].id).expect("retry works");
}

#[test]
fn threads_can_be_renamed_and_deleted_with_their_messages() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 2000);
    let m = mock(vec![Sc::Ok]);
    let pid = profile(&core, &m.base, None);
    let t = thread(&core);
    send(&core, t, &pid, "hi", &Scope::None).expect("send");
    rename_thread(&core, t, "  Plans  ").expect("rename");
    assert_eq!(threads(&core).expect("t")[0].title, "Plans");
    assert_eq!(
        kind(&rename_thread(&core, t, "  ").unwrap_err()),
        "invalidInput"
    );
    assert_eq!(
        threads(&core).expect("t")[0].profile_id.as_deref(),
        Some(pid.as_str())
    );
    delete_thread(&core, t).expect("delete");
    assert!(threads(&core).expect("t").is_empty());
    assert_eq!(kind(&messages(&core, t).unwrap_err()), "notFound");
    assert_eq!(kind(&delete_thread(&core, t).unwrap_err()), "notFound");
}

#[test]
fn dtos_match_the_contract_field_names() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 2000);
    let m = mock(vec![Sc::Ok]);
    let pid = profile(&core, &m.base, Some(100));
    let t = thread(&core);
    let reply = send(&core, t, &pid, "hi", &Scope::None).expect("send");
    let has = |v: &serde_json::Value, fields: &[&str]| {
        for f in fields {
            assert!(v.get(*f).is_some(), "{f}");
        }
    };
    has(
        &serde_json::to_value(&reply).expect("json"),
        &[
            "id",
            "threadId",
            "role",
            "text",
            "attachedContext",
            "scopeKind",
            "host",
            "status",
            "error",
            "inputTokens",
            "outputTokens",
            "createdAt",
        ],
    );
    has(
        &serde_json::to_value(&profiles(&core).expect("p")[0]).expect("json"),
        &[
            "id",
            "kind",
            "label",
            "baseUrl",
            "host",
            "model",
            "monthlyTokenBudget",
            "isDefault",
            "hasKey",
            "keychainOk",
        ],
    );
    has(
        &serde_json::to_value(&threads(&core).expect("t")[0]).expect("json"),
        &[
            "id",
            "title",
            "profileId",
            "model",
            "createdAt",
            "updatedAt",
        ],
    );
    has(
        &serde_json::to_value(&usage(&core).expect("u")[0]).expect("json"),
        &[
            "profileId",
            "month",
            "inputTokens",
            "outputTokens",
            "budget",
        ],
    );
    let e = serde_json::to_value(CommandError::from(CompanionError::busy())).expect("j");
    assert_eq!(e["kind"], "busy");
    assert_eq!(ErrorKind::BudgetReached.as_str(), "budgetReached");
}

#[test]
fn test_connection_records_usage_and_reports_the_model() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 2000);
    let m = mock(vec![Sc::Ok, Sc::Status(401)]);
    let pid = profile(&core, &m.base, None);
    let r = test_connection(&core, &pid).expect("ok");
    assert!(r.ok);
    assert_eq!(r.model, "mock-model");
    assert_eq!(usage(&core).expect("u")[0].input_tokens, 10);
    assert_eq!(
        kind(&test_connection(&core, &pid).unwrap_err()),
        "keyRejected"
    );
}

// -- security review fixes (M1, L1, L3, I2)

#[test]
fn cancel_frees_the_thread_at_once_and_the_late_call_is_still_counted() {
    let dir = tempfile::tempdir().expect("dir");
    let core = Arc::new(core_with(dir.path(), memory(), 20_000));
    // The first request is slow (and ends with an unusable body); the second answers at once.
    let m = mock(vec![Sc::Slow, Sc::Ok]);
    let pid = profile(&core, &m.base, None);
    let t = thread(&core);
    let (c2, pid2) = (Arc::clone(&core), pid.clone());
    let first = std::thread::spawn(move || send(&c2, t, &pid2, "slow one", &Scope::None));
    let start = std::time::Instant::now();
    while m.requests.lock().expect("log").is_empty() {
        assert!(start.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(20));
    }
    cancel(&core, t);
    let e = first.join().expect("join").unwrap_err();
    assert_eq!(kind(&e), "cancelled");
    assert!(e.message.contains("may already have reached the provider"));
    // The mock is still sleeping, yet the thread is free: a new send goes straight through, and
    // so does a delete-protected operation on another thread.
    let again = send(&core, t, &pid, "again", &Scope::None).expect("free at once");
    assert_eq!(again.text, "a reply");
    let ms = messages(&core, t).expect("m");
    assert_eq!(ms.len(), 3);
    assert_eq!(ms[0].status, "failed");
    assert_eq!(ms[0].error.as_ref().expect("e").kind, "cancelled");
    assert_eq!(
        (ms[1].status.as_str(), ms[2].role.as_str()),
        ("sent", "assistant")
    );
    // When the slow worker ends, the cancelled call's cost is on the books (an estimate: the mock
    // sends no usable reply). The reply of the second send is 10 + 5 tokens.
    let start = std::time::Instant::now();
    while used_tokens(&core) <= 15 {
        assert!(
            start.elapsed() < Duration::from_secs(15),
            "cancelled call was not counted"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn stale_context_hash_is_refused_before_anything_is_stored_or_sent() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 2000);
    seed_session(&core);
    let m = mock(vec![Sc::Ok]);
    let pid = profile(&core, &m.base, None);
    let t = thread(&core);
    let scope = selected(true, vec![]);
    let seen = preview_context(&core, scope.clone()).expect("preview");
    let hash = seen.sha256.clone().expect("hash");
    assert_eq!(hash.len(), 64);
    assert!(
        hash.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
    assert_eq!(hash, context_sha256(&seen.text));
    // The notes change after the preview.
    with_store(&core, |s| {
        s.set_notes(
            "claude-code",
            "s1",
            "CHANGED NOTES",
            OffsetDateTime::now_utc(),
        )
    })
    .expect("change");
    let e = raw_send(&core, t, &pid, "hi", &scope, Some(&hash)).unwrap_err();
    assert_eq!(kind(&e), "invalidInput");
    assert_eq!(
        e.message,
        "The context changed since you previewed it. Review it again."
    );
    assert!(messages(&core, t).expect("m").is_empty(), "nothing stored");
    assert!(m.requests.lock().expect("log").is_empty(), "nothing sent");
    // A missing hash for a scope that attaches text is also a mismatch; a hash for scope none is.
    assert_eq!(
        kind(&raw_send(&core, t, &pid, "hi", &scope, None).unwrap_err()),
        "invalidInput"
    );
    assert_eq!(
        kind(&raw_send(&core, t, &pid, "hi", &Scope::None, Some(&hash)).unwrap_err()),
        "invalidInput"
    );
    assert!(m.requests.lock().expect("log").is_empty());
    // The fresh preview works, and scope none needs no hash.
    let fresh = preview_context(&core, scope.clone()).expect("p").sha256;
    raw_send(&core, t, &pid, "hi", &scope, fresh.as_deref()).expect("fresh hash");
    raw_send(&core, t, &pid, "plain", &Scope::None, None).expect("none");
    assert!(
        preview_context(&core, Scope::None)
            .expect("p")
            .sha256
            .is_none()
    );
}

#[test]
fn history_is_only_sent_to_the_host_it_was_sent_to() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 2000);
    let a = mock(vec![Sc::Ok]);
    let b = mock(vec![Sc::Ok]);
    let pid = profile(&core, &a.base, None);
    let t = thread(&core);
    send(&core, t, &pid, "to host A", &Scope::None).expect("a1");
    let ms = messages(&core, t).expect("m");
    assert!(ms.iter().all(|m| {
        m.host
            .as_deref()
            .is_some_and(|h| h.starts_with("127.0.0.1:"))
    }));
    // The profile now points at another host: the earlier messages do not follow.
    repoint(&core, &pid, &b.base);
    send(&core, t, &pid, "to host B", &Scope::None).expect("b1");
    let v = last_body(&b);
    let roles: Vec<String> = v["messages"]
        .as_array()
        .expect("arr")
        .iter()
        .map(|m| m["content"].as_str().expect("c").to_owned())
        .collect();
    assert_eq!(roles.len(), 2, "system + the new message only");
    assert_eq!(roles[1], "to host B");
    assert!(!v.to_string().contains("to host A"));
    // Back to A: A's own messages return, B's do not.
    repoint(&core, &pid, &a.base);
    send(&core, t, &pid, "to host A again", &Scope::None).expect("a2");
    let v = last_body(&a);
    let text = v.to_string();
    assert!(text.contains("to host A") && text.contains("to host A again"));
    assert!(!text.contains("to host B"));
}

#[test]
fn retry_after_the_address_changed_to_another_host_is_refused() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core_with(dir.path(), memory(), 2000);
    let a = mock(vec![Sc::Status(500)]);
    let b = mock(vec![Sc::Ok]);
    let pid = profile(&core, &a.base, None);
    let t = thread(&core);
    send(&core, t, &pid, "private question", &Scope::None).unwrap_err();
    let failed = messages(&core, t).expect("m")[0].id;
    repoint(&core, &pid, &b.base);
    let e = retry(&core, failed).unwrap_err();
    assert_eq!(kind(&e), "invalidInput");
    assert_eq!(
        e.message,
        "This provider's address changed. Send the message again to review it."
    );
    assert!(
        b.requests.lock().expect("log").is_empty(),
        "nothing reached host B"
    );
    assert_eq!(messages(&core, t).expect("m")[0].status, "failed");
    // Same host again: the retry goes ahead.
    repoint(&core, &pid, &a.base);
    assert_eq!(kind(&retry(&core, failed).unwrap_err()), "providerError");
}

mod catalog;
mod streaming;
