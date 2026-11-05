//! ADR-013: the optional `context` envelope field, end to end over the real endpoint.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use agent_core::AgentEvent;
use agent_ipc::{
    Config, EventContext, SinkError, request_line, send_event, send_event_with_context, serve,
};

const EVENT: &str = r#"{"schemaVersion":1,"eventId":"6f1c2a52-8a63-4f53-9a0e-1d1f6b7e2c11","provider":"claude-code","providerSessionId":"s1","eventType":"working","timestampUtc":"2026-11-05T10:15:30Z"}"#;
const DEADLINE: Duration = Duration::from_secs(5);

#[test]
fn envelope_has_no_context_key_without_context() {
    for ctx in [None, Some(EventContext::default())] {
        let line = request_line("tok", EVENT, ctx.as_ref()).expect("line");
        let text = String::from_utf8(line).expect("utf8");
        assert!(!text.contains("context"), "{text}");
        assert!(text.ends_with('\n'));
    }
    let ctx = EventContext {
        intent: Some("do it".into()),
        ask: None,
    };
    let text =
        String::from_utf8(request_line("tok", EVENT, Some(&ctx)).expect("line")).expect("utf8");
    assert!(text.contains(r#""context":{"intent":"do it"}"#), "{text}");
}

#[tokio::test(flavor = "multi_thread")]
async fn context_travels_beside_the_event_over_the_real_endpoint() {
    let dir = tempfile::tempdir().expect("tempdir");
    let seen: Arc<Mutex<Vec<Option<EventContext>>>> = Arc::default();
    let s2 = Arc::clone(&seen);
    let sink = move |_e: AgentEvent, c: Option<EventContext>| -> Result<bool, SinkError> {
        s2.lock().map_err(|_| SinkError)?.push(c);
        Ok(true)
    };
    let _running = serve(dir.path(), Arc::new(sink), Config::default()).expect("serve");

    let ctx = EventContext {
        intent: Some("fix it".into()),
        ask: Some("Permission to use Bash".into()),
    };
    let r = send_event_with_context(dir.path(), EVENT, Some(&ctx), DEADLINE)
        .await
        .expect("send");
    assert!(r.ok);
    // Old-style client: no context.
    let r = send_event(dir.path(), EVENT, DEADLINE).await.expect("send");
    assert!(r.ok);

    let got = seen.lock().expect("lock").clone();
    assert_eq!(got, vec![Some(ctx), None]);
}
