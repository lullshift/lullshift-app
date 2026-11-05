//! End-to-end over the real platform endpoint (named pipe on Windows, Unix socket elsewhere).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use agent_core::AgentEvent;
use agent_ipc::{
    ClientError, Config, Endpoint, ErrorCode, EventSink, SinkError, send_event, serve,
};

const EVENT: &str = r#"{"schemaVersion":1,"eventId":"6f1c2a52-8a63-4f53-9a0e-1d1f6b7e2c11","provider":"claude-code","providerSessionId":"s1","eventType":"working","timestampUtc":"2026-11-05T10:15:30Z"}"#;
const DEADLINE: Duration = Duration::from_secs(5);

fn sink() -> (Arc<dyn EventSink>, Arc<Mutex<Vec<String>>>) {
    let seen = Arc::new(Mutex::new(Vec::<String>::new()));
    let s2 = Arc::clone(&seen);
    let sink =
        move |e: AgentEvent, _c: Option<agent_ipc::EventContext>| -> Result<bool, SinkError> {
            let mut v = s2.lock().map_err(|_| SinkError)?;
            let id = e.event_id.to_string();
            let new = !v.contains(&id);
            if new {
                v.push(id);
            }
            Ok(new)
        };
    (Arc::new(sink), seen)
}

#[tokio::test(flavor = "multi_thread")]
async fn delivers_authenticated_events_and_rejects_bad_token() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (sink, seen) = sink();
    let running = serve(dir.path(), sink, Config::default()).expect("serve");
    assert_eq!(
        Endpoint::read(dir.path()).expect("endpoint.json"),
        running.endpoint
    );
    let endpoint_json =
        std::fs::read_to_string(dir.path().join("ipc").join("endpoint.json")).expect("read");
    let token = std::fs::read_to_string(dir.path().join("ipc").join("token")).expect("token");
    assert!(
        !endpoint_json.contains(token.trim()),
        "endpoint.json must not contain the token"
    );

    let r = send_event(dir.path(), EVENT, DEADLINE).await.expect("send");
    assert!(r.ok && !r.duplicate);
    let r = send_event(dir.path(), EVENT, DEADLINE).await.expect("send");
    assert!(r.ok && r.duplicate);

    std::fs::write(dir.path().join("ipc").join("token"), "0".repeat(64)).expect("tamper");
    let r = send_event(dir.path(), EVENT, DEADLINE).await.expect("send");
    assert_eq!(r.error, Some(ErrorCode::Unauthorized));

    assert_eq!(seen.lock().expect("lock").len(), 1);
    let stats = running.stats();
    assert_eq!(
        (stats.stored, stats.duplicate, stats.unauthorized),
        (1, 1, 1)
    );
}

/// Unix: one socket path per data directory, so a second server is refused.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn second_server_on_same_endpoint_is_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    let _first = serve(dir.path(), sink().0, Config::default()).expect("serve");
    match serve(dir.path(), sink().0, Config::default()) {
        Err(agent_ipc::ServeError::AddressInUse) => {}
        Err(e) => panic!("expected AddressInUse, got {e}"),
        Ok(_) => panic!("second server must not bind"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn client_fails_fast_when_app_is_not_running() {
    let dir = tempfile::tempdir().expect("tempdir");
    assert!(matches!(
        send_event(dir.path(), EVENT, DEADLINE).await,
        Err(ClientError::NotRunning)
    ));

    // Endpoint file left behind by an app that has exited.
    let running = serve(dir.path(), sink().0, Config::default()).expect("serve");
    drop(running);
    tokio::time::sleep(Duration::from_millis(50)).await;
    let started = std::time::Instant::now();
    let r = send_event(dir.path(), EVENT, DEADLINE).await;
    assert!(r.is_err(), "nothing is listening");
    assert!(started.elapsed() < Duration::from_secs(2), "must fail fast");
}

/// Windows: pipe names are random per launch and not derived from the data directory, so
/// another local user can't pre-create (squat) the pipe a forwarder will connect to.
#[cfg(windows)]
#[tokio::test(flavor = "multi_thread")]
async fn pipe_names_are_unpredictable_per_launch() {
    let dir = tempfile::tempdir().expect("tempdir");
    let a = serve(dir.path(), sink().0, Config::default()).expect("serve");
    let first = a.endpoint.address.clone();
    assert!(a.is_accepting());
    drop(a);
    let b = serve(dir.path(), sink().0, Config::default()).expect("serve again");
    assert_ne!(first, b.endpoint.address);
    let suffix = b.endpoint.address.rsplit('-').next().expect("suffix");
    assert_eq!(suffix.len(), 32, "128-bit nonce");
    assert_eq!(
        Endpoint::read(dir.path()).expect("endpoint.json"),
        b.endpoint
    );
    let r = send_event(dir.path(), EVENT, DEADLINE).await.expect("send");
    assert!(r.ok);
}
