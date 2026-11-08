//! Model-written sets, command level (S13-T06, S13-T07): the switches, refusals that send
//! nothing, a full success against a loopback provider, failures that store nothing, budget,
//! cancel, the request's contents (S13-A04), the avoid list, skips and the denial of set ids to
//! suggestions.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ::companion::{HttpConfig, MemoryKeyStore};
use serde_json::{Value, json};

use super::*;
use crate::companion_commands::{
    CompanionState, ProfileInput, save_prefs, save_profile, set_key, set_tools_enabled,
};

const KEY: &str = "sk-test-GENSECRET-0123456789";

// ---- a loopback provider -----------------------------------------------------------------

#[derive(Clone)]
pub(crate) enum Reply {
    /// An OpenAI-style stream whose text is this.
    Text(String),
    /// Waits, then answers (for cancel).
    Slow(String, u64),
}

pub(crate) struct Mock {
    pub(crate) base: String,
    pub(crate) requests: Arc<Mutex<Vec<String>>>,
}

impl Mock {
    pub(crate) fn bodies(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }

    fn count(&self) -> usize {
        self.requests.lock().unwrap().len()
    }
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

fn sse(text: &str) -> String {
    let t = serde_json::to_string(text).unwrap();
    format!(
        "data: {{\"choices\":[{{\"delta\":{{\"content\":{t}}}}}]}}\n\ndata: {{\"choices\":[{{\"delta\":{{}},\"finish_reason\":\"stop\"}}]}}\n\ndata: {{\"choices\":[],\"usage\":{{\"prompt_tokens\":100,\"completion_tokens\":50}}}}\n\ndata: [DONE]\n\n"
    )
}

/// Request n gets `script[n]`; the last entry repeats.
pub(crate) fn mock(script: Vec<Reply>) -> Mock {
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
                let mut l = log.lock().unwrap();
                l.push(req);
                l.len() - 1
            };
            let reply = script[n.min(script.len() - 1)].clone();
            std::thread::spawn(move || {
                let (text, wait) = match reply {
                    Reply::Text(t) => (t, 0),
                    Reply::Slow(t, ms) => (t, ms),
                };
                let _ = s.write_all(
                    b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n",
                );
                let _ = s.flush();
                std::thread::sleep(Duration::from_millis(wait));
                let _ = s.write_all(sse(&text).as_bytes());
            });
        }
    });
    Mock {
        base: format!("http://127.0.0.1:{port}/v1"),
        requests,
    }
}

// ---- fixtures ----------------------------------------------------------------------------

pub(crate) fn quiz_reply(title: &str, n: usize) -> String {
    let qs: Vec<Value> = (0..n)
        .map(|i| {
            json!({
                "prompt": format!("Question number {i}?"),
                "choices": ["Alpha", "Beta", "Gamma"],
                "answer": 1,
                "explanation": format!("Because {i}.")
            })
        })
        .collect();
    json!({ "title": title, "intro": "A short quiz.", "questions": qs }).to_string()
}

pub(crate) fn cards_reply(title: &str, n: usize) -> String {
    let cards: Vec<Value> = (0..n)
        .map(|i| json!({ "front": format!("Card {i}"), "back": format!("Back {i}") }))
        .collect();
    json!({ "title": title, "cards": cards }).to_string()
}

pub(crate) fn core_in(dir: &std::path::Path) -> Core {
    let mut core = Core::for_tests(dir);
    core.companion = CompanionState::with(
        Arc::new(MemoryKeyStore::new()),
        HttpConfig {
            timeout: Duration::from_secs(10),
            first_byte_timeout: Duration::from_secs(10),
            idle_timeout: Duration::from_secs(10),
            total_timeout: Duration::from_secs(20),
            ..HttpConfig::default()
        },
    );
    core
}

pub(crate) fn add_profile(core: &Core, base: &str, budget: Option<i64>) -> String {
    let p = save_profile(
        core,
        ProfileInput {
            id: None,
            kind: "openaiCompatible".into(),
            label: "Local".into(),
            base_url: Some(base.to_owned()),
            model: "mock-model".into(),
            monthly_token_budget: budget,
            is_default: true,
        },
    )
    .expect("profile");
    set_key(core, &p.id, KEY.to_owned()).expect("key");
    p.id
}

pub(crate) fn turn_on(core: &Core, body: bool) {
    set_settings(core, true, body).expect("switches");
}

pub(crate) fn dto(engine: &str, topic: &str) -> GenerateRequestDto {
    GenerateRequestDto {
        engine: engine.into(),
        topic: topic.into(),
        difficulty: "medium".into(),
        size: 5,
        breathing: None,
        avoid: false,
        origin_pack: None,
        profile_id: None,
        model: None,
    }
}

type Events = Arc<Mutex<Vec<GenerationEvent>>>;

fn collect() -> (Events, impl Fn(GenerationEvent) + Send + Sync) {
    let events: Events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    (events, move |e| sink.lock().unwrap().push(e))
}

fn gen_set(core: &Core, req: &GenerateRequestDto) -> Res<ActivityInfo> {
    let (_, emit) = collect();
    write_set(core, "req-00000001", req, &emit)
}

fn err_kind<T: std::fmt::Debug>(r: Res<T>) -> &'static str {
    r.expect_err("an error").kind
}

fn stored(core: &Core) -> usize {
    list(core, None).expect("list").len()
}

fn used_tokens(core: &Core) -> i64 {
    let month = local_store::month_of(OffsetDateTime::now_utc());
    with_store(core, |s| s.companion_usage(&month))
        .expect("usage")
        .iter()
        .map(|u| u.input_tokens + u.output_tokens)
        .sum()
}

// ---- switches ----------------------------------------------------------------------------

#[test]
fn both_switches_default_off_and_body_follows_the_main_switch() {
    let dir = tempfile::tempdir().unwrap();
    let core = core_in(dir.path());
    assert_eq!(
        crate::companion_commands::prefs(&core).content_generation,
        ContentGenerationPrefs::default()
    );
    // No provider yet: the main switch cannot be turned on.
    assert_eq!(err_kind(set_settings(&core, true, false)), "invalidInput");
    add_profile(&core, "http://127.0.0.1:9/v1", None);
    // Body cannot be on without the main switch.
    assert_eq!(err_kind(set_settings(&core, false, true)), "invalidInput");
    let on = set_settings(&core, true, true).expect("both");
    assert!(on.enabled && on.body_enabled);
    assert_eq!(
        crate::companion_commands::prefs(&core).content_generation,
        on
    );
    // Turning the main switch off turns the body switch off with it.
    let off = set_settings(&core, false, true);
    assert!(off.is_err(), "body without main is refused");
    let off = set_settings(&core, false, false).expect("off");
    assert!(!off.enabled && !off.body_enabled);
    // A stored record with body on and main off reads as both off.
    let mut p = crate::companion_commands::prefs(&core);
    p.content_generation = ContentGenerationPrefs {
        enabled: false,
        body_enabled: true,
    };
    save_prefs(&core, &p).unwrap();
    assert_eq!(settings(&core), ContentGenerationPrefs::default());
}

// ---- refusals send nothing ---------------------------------------------------------------

#[test]
fn with_the_switch_off_the_command_refuses_and_nothing_is_sent() {
    let dir = tempfile::tempdir().unwrap();
    let core = core_in(dir.path());
    let m = mock(vec![Reply::Text(quiz_reply("T", 5))]);
    add_profile(&core, &m.base, None);
    let (events, emit) = collect();
    let e = write_set(&core, "req-00000001", &dto("quiz", "Rust"), &emit).expect_err("refused");
    assert_eq!(e.kind, "generationOff");
    assert_eq!(m.count(), 0);
    assert_eq!(stored(&core), 0);
    // An unknown engine is refused too (with the switch on), also without sending.
    turn_on(&core, false);
    assert_eq!(err_kind(gen_set(&core, &dto("away", "x"))), "invalidInput");
    assert_eq!(m.count(), 0);
    assert!(events.lock().unwrap().is_empty());
}

#[test]
fn a_body_engine_needs_the_body_switch_and_sends_nothing_without_it() {
    let dir = tempfile::tempdir().unwrap();
    let core = core_in(dir.path());
    let m = mock(vec![Reply::Text("{}".into())]);
    add_profile(&core, &m.base, None);
    turn_on(&core, false);
    for engine in ["movement", "breathing", "pause"] {
        let mut d = dto(engine, "Desk break");
        if engine == "breathing" {
            d.breathing = Some(BreathingDto {
                pattern: "even".into(),
                rounds: 4,
            });
            d.size = 1;
        }
        assert_eq!(err_kind(gen_set(&core, &d)), "generationOff", "{engine}");
    }
    assert_eq!(m.count(), 0);
    assert_eq!(stored(&core), 0);
}

#[test]
fn preview_works_before_consent_and_matches_the_body_sent() {
    let dir = tempfile::tempdir().unwrap();
    let core = core_in(dir.path());
    let m = mock(vec![Reply::Text(quiz_reply("Sea life", 5))]);
    add_profile(&core, &m.base, None);
    // Switches are off: the preview is what the consent dialog shows.
    let mut req = dto("quiz", "Sea creatures");
    let shown = preview(&core, &req).expect("preview");
    assert_eq!(m.count(), 0, "a preview sends nothing");
    assert!(shown.contains("Sea creatures"));
    assert!(!shown.contains(KEY));
    turn_on(&core, false);
    req.topic = "Sea creatures".into();
    gen_set(&core, &req).expect("set");
    let sent = m.bodies();
    let body = sent[0].split("\r\n\r\n").nth(1).unwrap();
    // The local-server preview and the real body differ only in the endpoint's host, which is
    // not part of the body.
    assert_eq!(body, shown);
}

// ---- success -----------------------------------------------------------------------------

#[test]
fn a_full_success_stores_lists_and_launches_the_set() {
    let dir = tempfile::tempdir().unwrap();
    let core = core_in(dir.path());
    let m = mock(vec![Reply::Text(quiz_reply("Ocean quiz", 5))]);
    add_profile(&core, &m.base, None);
    turn_on(&core, false);
    let (events, emit) = collect();
    let info = write_set(&core, "req-00000001", &dto("quiz", "The ocean"), &emit).expect("set");
    assert!(info.id.starts_with("lullshift.generated.quiz."));
    assert_eq!(info.name, "Ocean quiz");
    let g = info.generated.as_ref().expect("generated info");
    assert_eq!(g.model.as_deref(), Some("mock-model"));
    assert_eq!(g.topic, "The ocean");
    assert_eq!(m.count(), 1);
    // Listed, filtered by engine, and launchable with checked data.
    let listed = list(&core, Some("quiz")).expect("list");
    assert_eq!(listed.len(), 1);
    assert!(list(&core, Some("teaser")).expect("list").is_empty());
    let content = crate::activities::content_for_core(&core, &info.id).expect("content");
    assert_eq!(content["title"], "Ocean quiz");
    assert_eq!(content["questions"].as_array().unwrap().len(), 5);
    assert_eq!(content["shuffle"], true, "the host sets shuffle");
    // The phases and the end, all with the request id.
    let ev = events.lock().unwrap().clone();
    let phases: Vec<&str> = ev
        .iter()
        .filter_map(|e| match e {
            GenerationEvent::Phase { phase, .. } => Some(*phase),
            _ => None,
        })
        .collect();
    assert_eq!(phases, ["waiting", "checking"]);
    assert!(matches!(
        ev.last(),
        Some(GenerationEvent::Done { request_id, set_id }) if request_id == "req-00000001" && *set_id == info.id
    ));
    // The cost of the call is booked.
    assert_eq!(used_tokens(&core), 150);
    // Deleting works and leaves nothing.
    assert!(delete_one(&core, &info.id).expect("delete"));
    assert_eq!(stored(&core), 0);
}

#[test]
fn a_first_reply_that_fails_is_retried_once_and_the_retry_may_succeed() {
    let dir = tempfile::tempdir().unwrap();
    let core = core_in(dir.path());
    let m = mock(vec![
        Reply::Text("not json at all".into()),
        Reply::Text(quiz_reply("Second try", 5)),
    ]);
    add_profile(&core, &m.base, None);
    turn_on(&core, false);
    let (events, emit) = collect();
    let info = write_set(&core, "req-00000001", &dto("quiz", "Anything"), &emit).expect("set");
    assert_eq!(info.name, "Second try");
    assert_eq!(m.count(), 2);
    let phases: Vec<&str> = events
        .lock()
        .unwrap()
        .iter()
        .filter_map(|e| match e {
            GenerationEvent::Phase { phase, .. } => Some(*phase),
            _ => None,
        })
        .collect();
    assert!(phases.contains(&"retrying"), "{phases:?}");
    assert_eq!(used_tokens(&core), 300, "both calls are booked");
}

#[test]
fn a_reply_failing_twice_stores_nothing_and_says_it_did_not_fit() {
    let dir = tempfile::tempdir().unwrap();
    let core = core_in(dir.path());
    let m = mock(vec![Reply::Text("still not json".into())]);
    add_profile(&core, &m.base, None);
    turn_on(&core, false);
    let (events, emit) = collect();
    let e = write_set(&core, "req-00000001", &dto("quiz", "Anything"), &emit).expect_err("fails");
    assert_eq!(e.kind, "didNotFit");
    assert!(e.message.contains("Nothing was lost"));
    assert_eq!(m.count(), 2, "one retry at most");
    assert_eq!(stored(&core), 0);
    assert_eq!(used_tokens(&core), 300, "failed calls still count");
    assert!(matches!(
        events.lock().unwrap().last(),
        Some(GenerationEvent::Failed {
            kind: "didNotFit",
            ..
        })
    ));
    // The reply text is nowhere in the error.
    assert!(!format!("{e:?}").contains("still not json"));
}

#[test]
fn a_reply_that_fails_the_engine_gate_is_not_stored() {
    let dir = tempfile::tempdir().unwrap();
    let core = core_in(dir.path());
    // Valid shape, but every answer points outside its choices.
    let bad = quiz_reply("Fine title", 5).replace("\"answer\":1", "\"answer\":7");
    let m = mock(vec![Reply::Text(bad)]);
    add_profile(&core, &m.base, None);
    turn_on(&core, false);
    assert_eq!(err_kind(gen_set(&core, &dto("quiz", "x"))), "didNotFit");
    assert_eq!(stored(&core), 0);
}

// ---- budget and cancel -------------------------------------------------------------------

#[test]
fn a_spent_budget_refuses_before_anything_is_sent() {
    let dir = tempfile::tempdir().unwrap();
    let core = core_in(dir.path());
    let m = mock(vec![Reply::Text(quiz_reply("T", 5))]);
    let id = add_profile(&core, &m.base, Some(100));
    turn_on(&core, false);
    book(&core, &id, 80, 40);
    let e = gen_set(&core, &dto("quiz", "x")).expect_err("budget");
    assert_eq!(e.kind, "budgetReached");
    assert_eq!(m.count(), 0);
    assert_eq!(stored(&core), 0);
}

#[test]
fn a_budget_reached_by_the_first_call_stops_the_retry() {
    let dir = tempfile::tempdir().unwrap();
    let core = core_in(dir.path());
    // The first reply does not fit and costs 150 tokens, which uses up the budget of 200? No:
    // 100 + 50 = 150 of 120 allowed, so the retry is refused by the budget check.
    let m = mock(vec![Reply::Text("nope".into())]);
    add_profile(&core, &m.base, Some(120));
    turn_on(&core, false);
    let e = gen_set(&core, &dto("quiz", "x")).expect_err("budget");
    assert_eq!(e.kind, "budgetRefused");
    assert_eq!(m.count(), 1);
    assert_eq!(stored(&core), 0);
    assert_eq!(used_tokens(&core), 150);
}

#[test]
fn cancel_stops_the_request_and_stores_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let core = Arc::new(core_in(dir.path()));
    let m = mock(vec![Reply::Slow(quiz_reply("Late", 5), 3000)]);
    add_profile(&core, &m.base, None);
    turn_on(&core, false);
    let worker = {
        let core = Arc::clone(&core);
        std::thread::spawn(move || {
            let (_, emit) = collect();
            write_set(&core, "req-00000001", &dto("quiz", "x"), &emit)
        })
    };
    // Wait until the request is on the wire, then cancel it.
    let start = std::time::Instant::now();
    while m.count() == 0 && start.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(20));
    }
    cancel(&core, "req-00000001");
    let e = worker.join().unwrap().expect_err("cancelled");
    assert_eq!(e.kind, "cancelled");
    assert_eq!(stored(&core), 0);
    // The one-at-a-time mark is gone, so the next press works.
    assert!(core.companion.generations.lock().unwrap().is_empty());
}

#[test]
fn only_one_set_is_written_at_a_time() {
    let dir = tempfile::tempdir().unwrap();
    let core = core_in(dir.path());
    add_profile(&core, "http://127.0.0.1:9/v1", None);
    turn_on(&core, false);
    let _first = Running::begin(&core, "req-00000001").expect("first");
    assert_eq!(err_kind(gen_set(&core, &dto("quiz", "x"))), "busy");
}

// ---- the request's contents --------------------------------------------------------------

#[test]
fn the_request_holds_only_the_topic_choices_and_the_schema() {
    let dir = tempfile::tempdir().unwrap();
    let core = core_in(dir.path());
    let m = mock(vec![Reply::Text(quiz_reply("T", 5))]);
    add_profile(&core, &m.base, None);
    turn_on(&core, false);
    // Things the app holds that must never reach the provider.
    let canary = "CANARY-SESSION-TASK-NOTE";
    with_store(&core, |s| {
        s.set_preference("canary", &format!("\"{canary}\""))
    })
    .unwrap();
    let mut d = dto("quiz", "Rust ownership");
    // An origin pack that is not bundled (an installed third-party pack's id): its text is never
    // read, whatever it is called.
    d.origin_pack = Some("thirdparty.evil-pack".into());
    d.avoid = true;
    gen_set(&core, &d).expect("set");
    let sent = m.bodies();
    assert_eq!(sent.len(), 1);
    // Only the body: the headers carry the key by design.
    let lower = sent[0].split("\r\n\r\n").nth(1).unwrap().to_lowercase();
    for banned in [
        canary,
        "session",
        "thread",
        "note",
        "evil-pack",
        "thirdparty",
        KEY,
    ] {
        assert!(
            !lower.contains(&banned.to_lowercase()),
            "{banned} reached the provider"
        );
    }
    assert!(sent[0].contains("Rust ownership"));
}

#[test]
fn a_bundled_origin_packs_items_go_in_the_avoid_list_and_an_unknown_ones_do_not() {
    let dir = tempfile::tempdir().unwrap();
    let core = core_in(dir.path());
    let m = mock(vec![Reply::Text(quiz_reply("T", 5))]);
    add_profile(&core, &m.base, None);
    turn_on(&core, false);
    // A bundled quiz pack.
    let pack = crate::activities::list_activities(&core)
        .unwrap()
        .into_iter()
        .find(|a| a.engine.as_deref() == Some("lullshift.quiz@1"))
        .expect("a bundled quiz pack");
    let items = sets::builtin_pack_items(pack.id(), "quiz");
    assert!(!items.is_empty());
    let mut d = dto("quiz", "Anything");
    d.avoid = true;
    d.origin_pack = Some(pack.id().to_owned());
    gen_set(&core, &d).expect("set");
    let body = m.bodies().remove(0);
    // The newest of the pack's items are in the request (at most 20, JSON-escaped in the body).
    let last = items.last().unwrap();
    let escaped = serde_json::to_string(last).unwrap();
    let escaped = escaped
        .trim_matches('"')
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    assert!(
        body.contains(&escaped),
        "the pack's last item is in the avoid list"
    );
    // With the switch off, none of it is sent.
    let mut d2 = d.clone();
    d2.avoid = false;
    gen_set(&core, &d2).expect("set");
    let body2 = m.bodies().remove(1);
    assert!(!body2.contains(&escaped));
}

#[test]
fn earlier_sets_of_the_same_topic_are_avoided_at_most_twenty_newest_first() {
    let dir = tempfile::tempdir().unwrap();
    let core = core_in(dir.path());
    add_profile(&core, "http://127.0.0.1:9/v1", None);
    let meta = |topic: &str| SetMeta {
        topic: topic.to_owned(),
        difficulty: "medium".into(),
        provider_host: "h".into(),
        model: Some("m".into()),
        origin_pack: None,
    };
    let data = |tag: &str, n: usize| {
        let qs: Vec<Value> = (0..n)
            .map(|i| {
                json!({"prompt": format!("{tag} q{i}"), "choices": ["a", "b"], "answer": 0,
                "explanation": "e"})
            })
            .collect();
        json!({"title": tag, "questions": qs})
    };
    // Three sets of 12 on one topic (oldest first), one on another.
    for tag in ["old", "mid", "new"] {
        sets::accept_generated_set(&core, "quiz", data(tag, 12), &meta("Space")).unwrap();
        std::thread::sleep(Duration::from_millis(1100));
    }
    sets::accept_generated_set(&core, "quiz", data("other", 3), &meta("Cooking")).unwrap();
    let req = base_request(&{
        let mut d = dto("quiz", " space ");
        d.avoid = true;
        d
    })
    .unwrap();
    let avoid = avoid_list(&core, &req, None).unwrap();
    assert_eq!(avoid.len(), 20);
    assert!(avoid.iter().all(|a| !a.starts_with("other")));
    // The newest set is fully in; the oldest is out.
    assert!(avoid.iter().filter(|a| a.starts_with("new")).count() == 12);
    assert!(avoid.iter().all(|a| !a.starts_with("old")));
}

// ---- This looks wrong --------------------------------------------------------------------

#[test]
fn skipped_items_are_left_out_the_next_time_the_set_starts() {
    let dir = tempfile::tempdir().unwrap();
    let core = core_in(dir.path());
    let m = mock(vec![Reply::Text(quiz_reply("Skippable", 5))]);
    add_profile(&core, &m.base, None);
    turn_on(&core, false);
    let info = gen_set(&core, &dto("quiz", "x")).expect("set");
    let id = info.id().to_owned();
    let listed = items(&core, &id).expect("items");
    assert_eq!(listed.len(), 5);
    assert_eq!(listed[1], "Question number 1?");
    skip(&core, &id, 1).expect("skip");
    skip(&core, &id, 1).expect("a repeat is a no-op");
    let content = crate::activities::content_for_core(&core, &id).expect("content");
    let prompts: Vec<&str> = content["questions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|q| q["prompt"].as_str().unwrap())
        .collect();
    assert_eq!(prompts.len(), 4);
    assert!(!prompts.contains(&"Question number 1?"));
    // An item that is not in the set is refused.
    assert_eq!(err_kind(skip(&core, &id, 99)), "invalidInput");
    // Marking every item leaves the set refusing to start, calmly.
    for i in [0, 2, 3, 4] {
        skip(&core, &id, i).expect("skip");
    }
    let e = crate::activities::content_for_core(&core, &id).expect_err("all skipped");
    assert_eq!(e.code(), "generated_all_skipped");
    // Only quiz and teaser sets have items to mark.
    let deck = sets::accept_generated_set(
        &core,
        "cards",
        json!({"title": "Calm", "cards": [{"front": "Hi"}]}),
        &SetMeta {
            topic: "t".into(),
            difficulty: "gentle".into(),
            provider_host: "h".into(),
            model: None,
            origin_pack: None,
        },
    )
    .unwrap();
    assert!(items(&core, &deck).expect("items").is_empty());
    assert_eq!(err_kind(skip(&core, &deck, 0)), "invalidInput");
}

// ---- deletion ----------------------------------------------------------------------------

#[test]
fn delete_all_removes_every_set() {
    let dir = tempfile::tempdir().unwrap();
    let core = core_in(dir.path());
    let meta = SetMeta {
        topic: "t".into(),
        difficulty: "gentle".into(),
        provider_host: "h".into(),
        model: None,
        origin_pack: None,
    };
    for _ in 0..3 {
        sets::accept_generated_set(
            &core,
            "cards",
            json!({"title": "Calm", "cards": [{"front": "Hi"}]}),
            &meta,
        )
        .unwrap();
    }
    assert_eq!(stored(&core), 3);
    assert_eq!(delete_all(&core).unwrap(), 3);
    assert_eq!(stored(&core), 0);
    // Deleting an id outside the namespace is refused.
    assert!(delete_one(&core, "lullshift.quiz-basics").is_err());
}

// ---- never suggested ---------------------------------------------------------------------

#[test]
fn sets_are_never_offered_by_suggestions_or_the_companion_lookups() {
    let dir = tempfile::tempdir().unwrap();
    let core = core_in(dir.path());
    let id = sets::accept_generated_set(
        &core,
        "movement",
        json!({"title": "Desk stretch", "wayOut": "Stop at any time.", "moves": [{"name": "Shrug", "steps": ["Lift and drop."], "reps": 3}]}),
        &SetMeta {
            topic: "t".into(),
            difficulty: "gentle".into(),
            provider_host: "h".into(),
            model: None,
            origin_pack: None,
        },
    )
    .unwrap();
    // The list Home and the lookups are built from has no set.
    let ids = crate::activities::list_ids(&core).unwrap();
    assert!(!ids.iter().any(|i| i.starts_with("lullshift.generated.")));
    assert!(!ids.contains(&id));
    // "Suggest a break" and "activity help" read that list: no set card, no help for a set id.
    let acts = crate::companion_commands::lookups::installed(&core).unwrap();
    assert!(
        acts.iter()
            .all(|a| !a.id.starts_with("lullshift.generated."))
    );
    for minutes in [None, Some(1), Some(5), Some(60)] {
        let s = crate::companion_commands::lookups::suggest_break(&acts, minutes);
        assert!(
            s.card
                .as_ref()
                .is_none_or(|c| !c.activity_id.starts_with("lullshift.generated.")),
            "{minutes:?}"
        );
        assert!(!s.text.contains(&id));
    }
    let help = crate::companion_commands::lookups::activity_help(&acts, &id);
    assert_eq!(help.text, "No installed activity has that id.");
}

// ---- switches turned off while a request runs; concurrent preference writers; avoid rows ----

fn movement_reply() -> String {
    json!({"title": "Desk moves", "moves": [{"name": "Shrug", "steps": ["Lift and drop your shoulders."], "reps": 5}]})
        .to_string()
}

fn wait_for_request(m: &Mock) {
    let start = std::time::Instant::now();
    while m.count() == 0 && start.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn turning_a_switch_off_while_the_provider_answers_stores_nothing() {
    for body in [false, true] {
        for via_set_settings in [true, false] {
            let dir = tempfile::tempdir().unwrap();
            let core = Arc::new(core_in(dir.path()));
            let (engine, reply) = if body {
                ("movement", movement_reply())
            } else {
                ("quiz", quiz_reply("Late", 5))
            };
            let m = mock(vec![Reply::Slow(reply, 1500)]);
            add_profile(&core, &m.base, None);
            turn_on(&core, body);
            let worker = {
                let core = Arc::clone(&core);
                std::thread::spawn(move || {
                    let (_, emit) = collect();
                    write_set(&core, "req-00000001", &dto(engine, "x"), &emit)
                })
            };
            wait_for_request(&m);
            if via_set_settings {
                // The command cancels the running request.
                set_settings(&core, false, false).unwrap();
            } else {
                // A switch changed behind the running request's back (nothing cancels it): the
                // write path reads the switches again and refuses to store.
                update_prefs(&core, |p| {
                    p.content_generation = ContentGenerationPrefs::default();
                    true
                })
                .unwrap();
            }
            let e = worker.join().unwrap().expect_err("refused");
            if via_set_settings {
                assert_eq!(e.kind, "cancelled", "body {body}");
            } else {
                assert_eq!(e.kind, "generationOff", "body {body}");
            }
            assert_eq!(stored(&core), 0, "body {body} via {via_set_settings}");
        }
    }
}

#[test]
fn only_the_body_switch_going_off_also_stops_the_write() {
    let dir = tempfile::tempdir().unwrap();
    let core = Arc::new(core_in(dir.path()));
    let m = mock(vec![Reply::Slow(movement_reply(), 1500)]);
    add_profile(&core, &m.base, None);
    turn_on(&core, true);
    let worker = {
        let core = Arc::clone(&core);
        std::thread::spawn(move || {
            let (_, emit) = collect();
            write_set(&core, "req-00000001", &dto("movement", "x"), &emit)
        })
    };
    wait_for_request(&m);
    update_prefs(&core, |p| {
        p.content_generation.body_enabled = false;
        true
    })
    .unwrap();
    assert_eq!(err_kind(worker.join().unwrap()), "generationOff");
    assert_eq!(stored(&core), 0);
}

#[test]
fn other_preference_writers_cannot_turn_the_switches_back_on() {
    let dir = tempfile::tempdir().unwrap();
    let core = Arc::new(core_in(dir.path()));
    add_profile(&core, "http://127.0.0.1:9/v1", None);
    for _ in 0..20 {
        turn_on(&core, true);
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flipper = {
            let core = Arc::clone(&core);
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                let mut on = false;
                while !stop.load(std::sync::atomic::Ordering::SeqCst) {
                    on = !on;
                    set_tools_enabled(&core, on).unwrap();
                    let _ = update_prefs(&core, |p| {
                        p.last_scope = None;
                        true
                    });
                }
            })
        };
        std::thread::sleep(Duration::from_millis(5));
        set_settings(&core, false, false).unwrap();
        stop.store(true, std::sync::atomic::Ordering::SeqCst);
        flipper.join().unwrap();
        let after = settings(&core);
        assert!(!after.enabled && !after.body_enabled);
    }
}

#[test]
fn avoid_rows_that_fail_the_engine_check_are_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let core = core_in(dir.path());
    add_profile(&core, "http://127.0.0.1:9/v1", None);
    let put = |id: &str, engine: &str, data: Value| {
        with_store(&core, |s| {
            s.insert_generated_set(&local_store::NewGeneratedSet {
                id,
                engine,
                topic: "Desk",
                difficulty: "gentle",
                data_json: &data.to_string(),
                created: OffsetDateTime::now_utc(),
                provider_host: "h",
                model: None,
                origin_pack: None,
            })
        })
        .unwrap();
    };
    let routine = |name: &str, step: &str| {
        json!({"title": "T", "wayOut": "Stop at any time.", "restSeconds": 15, "closing": "Done.",
            "moves": [{"name": name, "steps": [step], "reps": 5}]})
    };
    put(
        "lullshift.generated.movement.aaaaaaaaaaaa",
        "lullshift.movement@1",
        routine("Good shrug", "Lift and drop."),
    );
    put(
        "lullshift.generated.movement.bbbbbbbbbbbb",
        "lullshift.movement@1",
        routine("Bad jumper", "Now jump up"),
    );
    let req = base_request(&{
        let mut d = dto("movement", "desk");
        d.avoid = true;
        d
    })
    .unwrap();
    let avoid = avoid_list(&core, &req, None).unwrap();
    assert!(avoid.iter().any(|a| a.contains("Good shrug")), "{avoid:?}");
    assert!(avoid.iter().all(|a| !a.contains("Bad jumper")), "{avoid:?}");
}
