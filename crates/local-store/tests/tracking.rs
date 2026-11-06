//! Sprint 05: disconnecting a tool ends tracking of its open sessions (S05-T10) and session
//! titles (S05-T11, ADR-013). Uses migration 0005's columns; no new migration.

use agent_core::{AgentEvent, parse_event};
use local_store::{MAX_TITLE_CHARS, Store, StoreError};
use time::macros::datetime;
use time::{Duration, OffsetDateTime};

const T0: OffsetDateTime = datetime!(2026-11-05 10:00:00 UTC);
const P: &str = "claude-code";

fn ev(n: u32, provider: &str, session: &str, kind: &str) -> AgentEvent {
    parse_event(&format!(
        r#"{{"schemaVersion":1,"eventId":"00000000-0000-4000-8000-{n:012}","provider":"{provider}","providerSessionId":"{session}","eventType":"{kind}","timestampUtc":"2026-11-05T10:00:00Z"}}"#
    ))
    .expect("valid event")
}

/// claude-code: idle, working, attention, ended, completed; cursor: working.
fn populated() -> Store {
    let mut s = Store::open_in_memory().expect("open");
    for (n, (provider, id, kind)) in [
        (P, "idle", "session_started"),
        (P, "work", "working"),
        (P, "ask", "needs_attention"),
        (P, "done", "completed"),
        (P, "gone", "session_ended"),
        ("cursor", "other", "working"),
    ]
    .into_iter()
    .enumerate()
    {
        let n = u32::try_from(n).expect("small") + 1;
        s.ingest(&ev(n, provider, id, kind), T0).expect("ingest");
    }
    s
}

fn key(p: &str, s: &str) -> (String, String) {
    (p.to_owned(), s.to_owned())
}

#[test]
fn disconnect_marks_only_that_providers_open_sessions() {
    let mut s = populated();
    let at = T0 + Duration::minutes(5);
    assert_eq!(s.disconnect_provider(P, at).expect("disconnect"), 3);
    let marked = s.disconnected_sessions().expect("marked");
    assert_eq!(marked.len(), 3);
    for id in ["idle", "work", "ask"] {
        assert_eq!(marked.get(&key(P, id)), Some(&at), "{id}");
    }
    for terminal in ["done", "gone"] {
        assert!(!marked.contains_key(&key(P, terminal)), "{terminal}");
    }
    assert!(!marked.contains_key(&key("cursor", "other")));

    // The last reported state is kept, not rewritten.
    let work = s.session(P, "work").expect("read").expect("session");
    assert_eq!(work.state, agent_core::SessionState::Working);
    let ask = s.session(P, "ask").expect("read").expect("session");
    assert_eq!(ask.state, agent_core::SessionState::NeedsAttention);

    // Running it again changes and counts nothing.
    assert_eq!(
        s.disconnect_provider(P, at + Duration::minutes(1))
            .expect("again"),
        0
    );
    assert_eq!(s.disconnected_sessions().expect("marked").len(), 3);
    assert_eq!(
        s.disconnected_sessions()
            .expect("marked")
            .get(&key(P, "work")),
        Some(&at),
        "the first disconnect time stays"
    );
    // A provider with no sessions: zero.
    assert_eq!(s.disconnect_provider("codex", at).expect("none"), 0);
}

#[test]
fn a_new_event_brings_the_session_back() {
    let mut s = populated();
    s.disconnect_provider(P, T0 + Duration::minutes(5))
        .expect("disconnect");
    // A duplicate event id changes nothing.
    s.ingest(&ev(2, P, "work", "working"), T0 + Duration::minutes(6))
        .expect("duplicate");
    assert!(
        s.disconnected_sessions()
            .expect("marked")
            .contains_key(&key(P, "work"))
    );
    // A new event from that session clears only that session.
    s.ingest(&ev(50, P, "work", "working"), T0 + Duration::minutes(7))
        .expect("event");
    let marked = s.disconnected_sessions().expect("marked");
    assert!(!marked.contains_key(&key(P, "work")));
    assert!(marked.contains_key(&key(P, "ask")));
    assert!(marked.contains_key(&key(P, "idle")));
    // After the session is back, a later disconnect marks it again.
    assert_eq!(
        s.disconnect_provider(P, T0 + Duration::minutes(8))
            .expect("again"),
        1
    );
}

#[test]
fn existing_sessions_start_tracked() {
    let s = populated();
    assert!(s.disconnected_sessions().expect("marked").is_empty());
}

#[test]
fn user_title_is_trimmed_bounded_and_clearable() {
    let mut s = populated();
    s.set_user_title(P, "work", "  Fix login  ", T0)
        .expect("set");
    let ctx = s.context(P, "work").expect("read").expect("row");
    assert_eq!(ctx.user_title.as_deref(), Some("Fix login"));

    for bad in [
        "a\nb",
        "a\tb",
        "a\u{0007}b",
        "a\u{202E}b",
        "a\u{200B}b",
        &"x".repeat(MAX_TITLE_CHARS + 1),
    ] {
        assert!(
            matches!(
                s.set_user_title(P, "work", bad, T0),
                Err(StoreError::InvalidInput(_))
            ),
            "{bad:?}"
        );
    }
    s.set_user_title(P, "work", &"é".repeat(MAX_TITLE_CHARS), T0)
        .expect("exactly the limit");
    s.set_user_title(P, "work", "   ", T0).expect("clear");
    assert_eq!(
        s.context(P, "work").expect("read").expect("row").user_title,
        None
    );
    assert!(matches!(
        s.set_user_title(P, "nope", "x", T0),
        Err(StoreError::NotFound)
    ));
}

#[test]
fn agent_title_is_kept_until_replaced_and_cleared_with_captured_text() {
    let mut s = populated();
    s.record_agent_title(P, "work", "Agent name", T0)
        .expect("set");
    s.set_user_title(P, "work", "My name", T0).expect("user");
    s.record_agent_title(P, "ask", "Other", T0).expect("set");
    let ctx = s.context(P, "work").expect("read").expect("row");
    assert_eq!(
        ctx.agent_title.as_ref().map(|c| c.text.as_str()),
        Some("Agent name")
    );
    assert!(ctx.has_captured());
    for bad in ["", "a\nb", &"x".repeat(MAX_TITLE_CHARS + 1)] {
        assert!(s.record_agent_title(P, "work", bad, T0).is_err(), "{bad:?}");
    }

    s.clear_captured(P, "work").expect("clear one");
    let ctx = s.context(P, "work").expect("read").expect("row");
    assert_eq!(ctx.agent_title, None);
    assert_eq!(
        ctx.user_title.as_deref(),
        Some("My name"),
        "user text stays"
    );
    assert!(
        s.context(P, "ask")
            .expect("read")
            .expect("row")
            .agent_title
            .is_some(),
        "other sessions untouched"
    );

    s.clear_all_captured().expect("clear all");
    assert_eq!(
        s.context(P, "ask").expect("read").expect("row").agent_title,
        None
    );
    assert_eq!(
        s.context(P, "work")
            .expect("read")
            .expect("row")
            .user_title
            .as_deref(),
        Some("My name")
    );
}

#[test]
fn labels_carry_titles_and_the_first_intent_line() {
    let mut s = populated();
    s.record_agent_title(P, "work", "Agent name", T0)
        .expect("agent");
    s.set_user_title(P, "work", "My name", T0).expect("user");
    s.record_prompt(P, "ask", "\n\n  First line here\nsecond line", T0)
        .expect("prompt");
    s.record_prompt(P, "idle", &format!("{} tail", "w ".repeat(400)), T0)
        .expect("long prompt");
    let labels = s.session_labels().expect("labels");
    let work = &labels[&key(P, "work")];
    assert_eq!(work.user_title.as_deref(), Some("My name"));
    assert_eq!(work.agent_title.as_deref(), Some("Agent name"));
    assert_eq!(work.intent_line, None);
    assert_eq!(
        labels[&key(P, "ask")].intent_line.as_deref(),
        Some("First line here")
    );
    assert!(
        labels[&key(P, "idle")]
            .intent_line
            .as_deref()
            .is_some_and(|l| l.chars().count() <= 200)
    );
    assert!(
        !labels.contains_key(&key(P, "done")),
        "sessions without names have no entry"
    );

    s.delete_session(P, "work").expect("delete");
    assert!(
        !s.session_labels()
            .expect("labels")
            .contains_key(&key(P, "work"))
    );
}

#[test]
fn last_received_from_is_per_provider_and_none_without_events() {
    let mut s = Store::open_in_memory().expect("open");
    assert_eq!(s.last_received_from("opencode").expect("query"), None);
    s.ingest(&ev(1, "cursor", "c", "working"), T0)
        .expect("ingest");
    assert_eq!(s.last_received_from("opencode").expect("query"), None);
    s.ingest(&ev(2, "opencode", "o", "session_started"), T0)
        .expect("ingest");
    s.ingest(
        &ev(3, "opencode", "o", "working"),
        T0 + Duration::minutes(2),
    )
    .expect("ingest");
    assert_eq!(
        s.last_received_from("opencode").expect("query"),
        Some(T0 + Duration::minutes(2))
    );
    assert_eq!(s.last_received_from("cursor").expect("query"), Some(T0));
}
