//! Transport-independent request handling (ADR-009 protocol v1).
//!
//! One request per connection: `{"v":1,"token":"<hex>","event":{...}}` followed by `\n`.
//! One reply line. Error codes are fixed strings and never echo input.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use agent_core::{AgentEvent, parse_event};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::token::Token;

pub const PROTOCOL_VERSION: u64 = 1;

#[derive(Debug, Clone)]
pub struct Config {
    /// Request size cap in bytes (excluding the newline), enforced while reading.
    pub max_request_bytes: usize,
    /// Deadline for receiving the whole request.
    pub read_timeout: Duration,
    pub max_connections: usize,
    /// Token bucket: sustained events per second, burst of the same size.
    pub events_per_second: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_request_bytes: 64 * 1024,
            read_timeout: Duration::from_secs(2),
            max_connections: 16,
            events_per_second: 100,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    TooLarge,
    Timeout,
    Malformed,
    Unauthorized,
    InvalidEvent,
    RateLimited,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reply {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub duplicate: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorCode>,
}

impl Reply {
    fn stored() -> Self {
        Self {
            ok: true,
            duplicate: false,
            error: None,
        }
    }
    fn duplicate() -> Self {
        Self {
            ok: true,
            duplicate: true,
            error: None,
        }
    }
    fn err(code: ErrorCode) -> Self {
        Self {
            ok: false,
            duplicate: false,
            error: Some(code),
        }
    }
}

/// Most characters kept of a captured intent / latest request.
pub const MAX_INTENT_CHARS: usize = 2000;
/// Most characters kept of what the agent is asking.
pub const MAX_ASK_CHARS: usize = 300;
/// Most characters kept of a session title (Sprint 05, ADR-013).
pub const MAX_TITLE_CHARS: usize = 120;

/// Optional task text sent beside an event, only with the user's opt-in (ADR-013). Unknown
/// fields are rejected. Longer strings are cut to the caps; empty strings count as absent.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventContext {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask: Option<String>,
    /// The session's own name as the agent stores it (Claude Code `custom-title`), one line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

impl EventContext {
    pub fn is_empty(&self) -> bool {
        self.intent.is_none() && self.ask.is_none() && self.title.is_none()
    }

    /// Applies the caps and drops empty strings; `None` if nothing is left.
    pub fn normalized(self) -> Option<Self> {
        let cap = |s: Option<String>, max: usize| {
            s.filter(|t| !t.is_empty()).map(|t| {
                if t.chars().count() > max {
                    t.chars().take(max).collect()
                } else {
                    t
                }
            })
        };
        let out = Self {
            intent: cap(self.intent, MAX_INTENT_CHARS),
            ask: cap(self.ask, MAX_ASK_CHARS),
            title: cap(self.title, MAX_TITLE_CHARS),
        };
        (!out.is_empty()).then_some(out)
    }
}

/// The agent's process behind a hook (decision 35, ADR-025): the hook records it so the app can
/// notice later that the process is gone. Not task text, so it travels beside the event, not in
/// the consent-gated [`EventContext`]. The `start` key tells the process apart from a later one
/// that reuses the PID. Both numbers come from outside and are untrusted: [`Self::validated`]
/// drops anything out of range, and the app only ever *reads* process information about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessRef {
    pub pid: u32,
    pub start: u64,
}

impl ProcessRef {
    /// `Some(self)` if the PID and the start key are in range, else `None` (the event is still
    /// accepted, without a process).
    pub fn validated(self) -> Option<Self> {
        (process_probe::valid_pid(self.pid) && process_probe::valid_start(self.start))
            .then_some(self)
    }
}

impl From<process_probe::ProcessId> for ProcessRef {
    fn from(id: process_probe::ProcessId) -> Self {
        Self {
            pid: id.pid,
            start: id.start,
        }
    }
}

/// The sink could not store the event; reported to the client as `internal`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SinkError;

/// Where accepted events go (the desktop app's store). Called on a blocking thread.
pub trait EventSink: Send + Sync + 'static {
    /// `Ok(true)` if the event was new, `Ok(false)` for a duplicate `eventId`. `context` is the
    /// optional task text from the envelope; the sink stores it only if its own consent gate
    /// allows, and never in the event log.
    fn deliver(&self, event: AgentEvent, context: Option<EventContext>) -> Result<bool, SinkError>;

    /// [`Self::deliver`] with the agent's process when the hook sent one (already validated).
    /// The default ignores it, so sinks that don't track processes need no change.
    fn deliver_with_process(
        &self,
        event: AgentEvent,
        context: Option<EventContext>,
        _process: Option<ProcessRef>,
    ) -> Result<bool, SinkError> {
        self.deliver(event, context)
    }
}

/// A sink that also receives the agent's process.
pub struct WithProcess<F>(pub F);

impl<F> EventSink for WithProcess<F>
where
    F: Fn(AgentEvent, Option<EventContext>, Option<ProcessRef>) -> Result<bool, SinkError>
        + Send
        + Sync
        + 'static,
{
    fn deliver(&self, event: AgentEvent, context: Option<EventContext>) -> Result<bool, SinkError> {
        (self.0)(event, context, None)
    }

    fn deliver_with_process(
        &self,
        event: AgentEvent,
        context: Option<EventContext>,
        process: Option<ProcessRef>,
    ) -> Result<bool, SinkError> {
        (self.0)(event, context, process)
    }
}

impl<F> EventSink for F
where
    F: Fn(AgentEvent, Option<EventContext>) -> Result<bool, SinkError> + Send + Sync + 'static,
{
    fn deliver(&self, event: AgentEvent, context: Option<EventContext>) -> Result<bool, SinkError> {
        self(event, context)
    }
}

/// Counters only; never request content (ADR-009 logging rule).
#[derive(Debug, Default)]
pub struct Stats {
    pub stored: AtomicU64,
    pub duplicate: AtomicU64,
    pub too_large: AtomicU64,
    pub timeout: AtomicU64,
    pub malformed: AtomicU64,
    pub unauthorized: AtomicU64,
    pub invalid_event: AtomicU64,
    pub rate_limited: AtomicU64,
    pub internal: AtomicU64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsSnapshot {
    pub stored: u64,
    pub duplicate: u64,
    pub rejected: u64,
    pub unauthorized: u64,
}

impl Stats {
    fn count(&self, reply: &Reply) {
        let c = match (reply.error, reply.duplicate) {
            (None, false) => &self.stored,
            (None, true) => &self.duplicate,
            (Some(ErrorCode::TooLarge), _) => &self.too_large,
            (Some(ErrorCode::Timeout), _) => &self.timeout,
            (Some(ErrorCode::Malformed), _) => &self.malformed,
            (Some(ErrorCode::Unauthorized), _) => &self.unauthorized,
            (Some(ErrorCode::InvalidEvent), _) => &self.invalid_event,
            (Some(ErrorCode::RateLimited), _) => &self.rate_limited,
            (Some(ErrorCode::Internal), _) => &self.internal,
        };
        c.fetch_add(1, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> StatsSnapshot {
        let g = |c: &AtomicU64| c.load(Ordering::Relaxed);
        StatsSnapshot {
            stored: g(&self.stored),
            duplicate: g(&self.duplicate),
            unauthorized: g(&self.unauthorized),
            rejected: g(&self.too_large)
                + g(&self.timeout)
                + g(&self.malformed)
                + g(&self.unauthorized)
                + g(&self.invalid_event)
                + g(&self.rate_limited)
                + g(&self.internal),
        }
    }
}

struct Bucket {
    tokens: f64,
    last: Instant,
}

/// Shared state for one running endpoint.
pub struct Handler {
    token: Token,
    sink: Arc<dyn EventSink>,
    pub(crate) cfg: Config,
    bucket: Mutex<Bucket>,
    pub stats: Stats,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope<'a> {
    v: u64,
    #[serde(borrow)]
    token: &'a str,
    #[serde(borrow)]
    event: &'a RawValue,
    #[serde(default)]
    context: Option<EventContext>,
    #[serde(default)]
    process: Option<ProcessRef>,
}

impl Handler {
    pub fn new(token: Token, sink: Arc<dyn EventSink>, cfg: Config) -> Self {
        let burst = f64::from(cfg.events_per_second);
        Self {
            token,
            sink,
            cfg,
            bucket: Mutex::new(Bucket {
                tokens: burst,
                last: Instant::now(),
            }),
            stats: Stats::default(),
        }
    }

    fn take_rate_token(&self) -> bool {
        let Ok(mut b) = self.bucket.lock() else {
            return false;
        };
        let rate = f64::from(self.cfg.events_per_second);
        let now = Instant::now();
        b.tokens = (b.tokens + now.duration_since(b.last).as_secs_f64() * rate).min(rate);
        b.last = now;
        if b.tokens >= 1.0 {
            b.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// Serves one connection: read one request, reply once, close.
    pub async fn serve_connection<S>(self: &Arc<Self>, mut stream: S)
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        let reply = match tokio::time::timeout(
            self.cfg.read_timeout,
            read_request(&mut stream, self.cfg.max_request_bytes),
        )
        .await
        {
            Err(_) => Reply::err(ErrorCode::Timeout),
            Ok(Err(code)) => Reply::err(code),
            Ok(Ok(bytes)) => self.process(bytes).await,
        };
        self.stats.count(&reply);
        self.write_reply(&mut stream, &reply).await;
    }

    /// Replies `rate_limited` without reading, for connections over the concurrency limit.
    pub(crate) async fn reject_busy<S>(&self, mut stream: S)
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        let reply = Reply::err(ErrorCode::RateLimited);
        self.stats.count(&reply);
        self.write_reply(&mut stream, &reply).await;
    }

    async fn write_reply<S: AsyncWrite + Unpin>(&self, stream: &mut S, reply: &Reply) {
        let mut line = serde_json::to_vec(reply)
            .unwrap_or_else(|_| br#"{"ok":false,"error":"internal"}"#.to_vec());
        line.push(b'\n');
        let _ = tokio::time::timeout(self.cfg.read_timeout, async {
            stream.write_all(&line).await?;
            stream.shutdown().await
        })
        .await;
    }

    async fn process(self: &Arc<Self>, bytes: Vec<u8>) -> Reply {
        let Ok(text) = std::str::from_utf8(&bytes) else {
            return Reply::err(ErrorCode::Malformed);
        };
        let Ok(env) = serde_json::from_str::<Envelope<'_>>(text) else {
            return Reply::err(ErrorCode::Malformed);
        };
        if env.v != PROTOCOL_VERSION {
            return Reply::err(ErrorCode::Malformed);
        }
        if !self.token.matches(env.token) {
            return Reply::err(ErrorCode::Unauthorized);
        }
        if !self.take_rate_token() {
            return Reply::err(ErrorCode::RateLimited);
        }
        let Ok(event) = parse_event(env.event.get()) else {
            return Reply::err(ErrorCode::InvalidEvent);
        };
        let context = env.context.and_then(EventContext::normalized);
        let process = env.process.and_then(ProcessRef::validated);
        let sink = Arc::clone(&self.sink);
        match tokio::task::spawn_blocking(move || {
            sink.deliver_with_process(event, context, process)
        })
        .await
        {
            Ok(Ok(true)) => Reply::stored(),
            Ok(Ok(false)) => Reply::duplicate(),
            _ => Reply::err(ErrorCode::Internal),
        }
    }
}

/// Reads up to the first `\n` (or EOF), failing as soon as more than `max` bytes arrive.
async fn read_request<R: AsyncRead + Unpin>(r: &mut R, max: usize) -> Result<Vec<u8>, ErrorCode> {
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 4096];
    loop {
        let n = r.read(&mut chunk).await.map_err(|_| ErrorCode::Malformed)?;
        if n == 0 {
            return Ok(buf);
        }
        let part = &chunk[..n];
        let (take, done) = match part.iter().position(|&b| b == b'\n') {
            Some(pos) => (&part[..pos], true),
            None => (part, false),
        };
        if buf.len() + take.len() > max {
            return Err(ErrorCode::TooLarge);
        }
        buf.extend_from_slice(take);
        if done {
            return Ok(buf);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use tokio::io::{AsyncBufReadExt, BufReader, duplex};

    const EVENT: &str = r#"{"schemaVersion":1,"eventId":"6f1c2a52-8a63-4f53-9a0e-1d1f6b7e2c11","provider":"claude-code","providerSessionId":"s1","eventType":"working","timestampUtc":"2026-11-05T10:15:30Z"}"#;

    struct Recorder {
        seen: Mutex<std::collections::HashSet<String>>,
        calls: AtomicUsize,
        contexts: Mutex<Vec<Option<EventContext>>>,
        processes: Mutex<Vec<Option<ProcessRef>>>,
    }

    impl EventSink for Recorder {
        fn deliver_with_process(
            &self,
            event: AgentEvent,
            context: Option<EventContext>,
            process: Option<ProcessRef>,
        ) -> Result<bool, SinkError> {
            self.processes.lock().map_err(|_| SinkError)?.push(process);
            self.deliver(event, context)
        }

        fn deliver(
            &self,
            event: AgentEvent,
            context: Option<EventContext>,
        ) -> Result<bool, SinkError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.contexts.lock().map_err(|_| SinkError)?.push(context);
            Ok(self
                .seen
                .lock()
                .map_err(|_| SinkError)?
                .insert(event.event_id.to_string()))
        }
    }

    fn handler(cfg: Config) -> (Arc<Handler>, Arc<Recorder>, Token) {
        let token = Token::generate().expect("token");
        let rec = Arc::new(Recorder {
            seen: Mutex::default(),
            calls: AtomicUsize::new(0),
            contexts: Mutex::default(),
            processes: Mutex::default(),
        });
        let sink: Arc<dyn EventSink> = rec.clone();
        (Arc::new(Handler::new(token.clone(), sink, cfg)), rec, token)
    }

    async fn roundtrip(h: &Arc<Handler>, request: &[u8]) -> (Reply, String) {
        let (mut client, server) = duplex(256 * 1024);
        let h2 = Arc::clone(h);
        let task = tokio::spawn(async move { h2.serve_connection(server).await });
        client.write_all(request).await.expect("write");
        let mut line = String::new();
        BufReader::new(client)
            .read_line(&mut line)
            .await
            .expect("read");
        task.await.expect("join");
        (serde_json::from_str(&line).expect("reply json"), line)
    }

    fn request(token: &str, event: &str) -> Vec<u8> {
        format!("{{\"v\":1,\"token\":\"{token}\",\"event\":{event}}}\n").into_bytes()
    }

    #[tokio::test]
    async fn stores_then_reports_duplicate() {
        let (h, rec, token) = handler(Config::default());
        let (r, _) = roundtrip(&h, &request(token.expose(), EVENT)).await;
        assert_eq!(r, Reply::stored());
        let (r, _) = roundtrip(&h, &request(token.expose(), EVENT)).await;
        assert_eq!(r, Reply::duplicate());
        assert_eq!(rec.calls.load(Ordering::SeqCst), 2);
        let s = h.stats.snapshot();
        assert_eq!((s.stored, s.duplicate, s.rejected), (1, 1, 0));
    }

    #[tokio::test]
    async fn rejects_missing_or_wrong_token_before_parsing_event() {
        let (h, rec, token) = handler(Config::default());
        let wrong = Token::generate().expect("token");
        for req in [
            request(wrong.expose(), EVENT),
            request("", EVENT),
            request(&token.expose()[..63], EVENT),
            request(wrong.expose(), r#"{"not":"an event"}"#),
        ] {
            let (r, _) = roundtrip(&h, &req).await;
            assert_eq!(r.error, Some(ErrorCode::Unauthorized));
        }
        let (r, _) = roundtrip(&h, format!("{{\"v\":1,\"event\":{EVENT}}}\n").as_bytes()).await;
        assert_eq!(r.error, Some(ErrorCode::Malformed), "missing token field");
        assert_eq!(rec.calls.load(Ordering::SeqCst), 0);
        assert_eq!(h.stats.snapshot().unauthorized, 4);
    }

    #[tokio::test]
    async fn rejects_oversize_before_parsing() {
        let (h, rec, token) = handler(Config::default());
        let pad = "x".repeat(70 * 1024);
        let big = format!(
            r#"{{"v":1,"token":"{}","event":{EVENT},"pad":"{pad}"}}"#,
            token.expose()
        );
        let (r, _) = roundtrip(&h, format!("{big}\n").as_bytes()).await;
        assert_eq!(r.error, Some(ErrorCode::TooLarge));
        assert_eq!(rec.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn rejects_malformed_and_invalid_without_echo() {
        const SENTINEL: &str = "SENTINEL_PROMPT_TEXT";
        let (h, rec, token) = handler(Config::default());
        let bad_event = EVENT.replace("working", SENTINEL);
        let cases: [(Vec<u8>, ErrorCode); 5] = [
            (format!("{SENTINEL}\n").into_bytes(), ErrorCode::Malformed),
            (b"\xff\xfe\n".to_vec(), ErrorCode::Malformed),
            (
                format!(
                    "{{\"v\":2,\"token\":\"{}\",\"event\":{EVENT}}}\n",
                    token.expose()
                )
                .into_bytes(),
                ErrorCode::Malformed,
            ),
            (
                format!(
                    "{{\"v\":1,\"token\":\"{}\",\"event\":{EVENT},\"{SENTINEL}\":1}}\n",
                    token.expose()
                )
                .into_bytes(),
                ErrorCode::Malformed,
            ),
            (request(token.expose(), &bad_event), ErrorCode::InvalidEvent),
        ];
        for (req, code) in cases {
            let (r, raw) = roundtrip(&h, &req).await;
            assert_eq!(r.error, Some(code));
            assert!(!raw.contains(SENTINEL));
        }
        assert_eq!(rec.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn slow_client_times_out() {
        let (h, rec, _) = handler(Config::default());
        let (mut client, server) = duplex(1024);
        let h2 = Arc::clone(&h);
        let task = tokio::spawn(async move { h2.serve_connection(server).await });
        client.write_all(b"{\"v\":1,").await.expect("write");
        let mut line = String::new();
        BufReader::new(client)
            .read_line(&mut line)
            .await
            .expect("read");
        task.await.expect("join");
        let r: Reply = serde_json::from_str(&line).expect("json");
        assert_eq!(r.error, Some(ErrorCode::Timeout));
        assert_eq!(rec.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn rate_limit_applies_to_authenticated_events() {
        let (h, _, token) = handler(Config {
            events_per_second: 2,
            ..Config::default()
        });
        let ev = |n: u32| {
            EVENT.replace(
                "6f1c2a52-8a63-4f53-9a0e-1d1f6b7e2c11",
                &format!("00000000-0000-4000-8000-{n:012}"),
            )
        };
        let mut codes = Vec::new();
        for n in 0..4 {
            codes.push(
                roundtrip(&h, &request(token.expose(), &ev(n)))
                    .await
                    .0
                    .error,
            );
        }
        assert_eq!(codes[..2], [None, None]);
        assert!(
            codes[2..]
                .iter()
                .all(|c| *c == Some(ErrorCode::RateLimited))
        );
    }

    #[tokio::test]
    async fn sink_failure_is_internal() {
        let token = Token::generate().expect("token");
        let sink: Arc<dyn EventSink> =
            Arc::new(|_e: AgentEvent, _c: Option<EventContext>| Err(SinkError));
        let h = Arc::new(Handler::new(token.clone(), sink, Config::default()));
        let (r, _) = roundtrip(&h, &request(token.expose(), EVENT)).await;
        assert_eq!(r.error, Some(ErrorCode::Internal));
    }

    fn request_with_context(token: &str, context: &str) -> Vec<u8> {
        format!("{{\"v\":1,\"token\":\"{token}\",\"event\":{EVENT},\"context\":{context}}}\n")
            .into_bytes()
    }

    #[tokio::test]
    async fn old_clients_without_context_still_work() {
        let (h, rec, token) = handler(Config::default());
        let (r, _) = roundtrip(&h, &request(token.expose(), EVENT)).await;
        assert_eq!(r, Reply::stored());
        assert_eq!(*rec.contexts.lock().expect("lock"), vec![None]);
    }

    fn request_with_process(token: &str, process: &str) -> Vec<u8> {
        format!("{{\"v\":1,\"token\":\"{token}\",\"event\":{EVENT},\"process\":{process}}}\n")
            .into_bytes()
    }

    #[tokio::test]
    async fn the_agent_process_is_delivered_beside_the_event_and_old_clients_have_none() {
        let (h, rec, token) = handler(Config::default());
        let req = request_with_process(token.expose(), r#"{"pid":4242,"start":133000000000}"#);
        assert_eq!(roundtrip(&h, &req).await.0, Reply::stored());
        let old = request(token.expose(), &EVENT.replace("6f1c2a52", "6f1c2a53"));
        assert_eq!(roundtrip(&h, &old).await.0, Reply::stored());
        let got = rec.processes.lock().expect("lock").clone();
        assert_eq!(
            got,
            vec![
                Some(ProcessRef {
                    pid: 4242,
                    start: 133_000_000_000
                }),
                None
            ]
        );
    }

    #[tokio::test]
    async fn an_out_of_range_process_is_dropped_but_the_event_is_kept() {
        let (h, rec, token) = handler(Config::default());
        for (n, process) in [
            r#"{"pid":0,"start":5}"#,
            r#"{"pid":4294967295,"start":5}"#,
            r#"{"pid":7,"start":0}"#,
            r#"{"pid":7,"start":9223372036854775807}"#,
        ]
        .into_iter()
        .enumerate()
        {
            let event = EVENT.replace("6f1c2a52", &format!("6f1c2a5{n}"));
            let req = format!(
                "{{\"v\":1,\"token\":\"{}\",\"event\":{event},\"process\":{process}}}\n",
                token.expose()
            );
            assert_eq!(roundtrip(&h, req.as_bytes()).await.0, Reply::stored());
        }
        assert_eq!(
            *rec.processes.lock().expect("lock"),
            vec![None, None, None, None]
        );
    }

    #[tokio::test]
    async fn a_malformed_process_is_rejected_without_echo() {
        let (h, rec, token) = handler(Config::default());
        for process in [
            r#"{"pid":"x","start":5}"#,
            r#"{"pid":-1,"start":5}"#,
            r#"{"pid":7,"start":5,"extra":1}"#,
            r#"{"pid":7}"#,
            "5",
        ] {
            let (r, raw) = roundtrip(&h, &request_with_process(token.expose(), process)).await;
            assert_eq!(r.error, Some(ErrorCode::Malformed), "{process}");
            assert!(!raw.contains("extra"), "no echo");
        }
        assert_eq!(rec.calls.load(Ordering::SeqCst), 0, "nothing was delivered");
    }

    #[tokio::test]
    async fn process_is_not_delivered_for_unauthorized_requests() {
        let (h, rec, _token) = handler(Config::default());
        let wrong = Token::generate().expect("token");
        let (r, _) = roundtrip(
            &h,
            &request_with_process(wrong.expose(), r#"{"pid":4242,"start":5}"#),
        )
        .await;
        assert_eq!(r.error, Some(ErrorCode::Unauthorized));
        assert!(rec.processes.lock().expect("lock").is_empty());
    }

    #[tokio::test]
    async fn context_is_delivered_beside_the_event() {
        let (h, rec, token) = handler(Config::default());
        let req = request_with_context(token.expose(), r#"{"intent":"fix the bug","ask":"ok?"}"#);
        let (r, _) = roundtrip(&h, &req).await;
        assert_eq!(r, Reply::stored());
        let got = rec.contexts.lock().expect("lock").clone();
        assert_eq!(
            got,
            vec![Some(EventContext {
                intent: Some("fix the bug".into()),
                ask: Some("ok?".into()),
                title: None,
            })]
        );
    }

    #[tokio::test]
    async fn title_is_accepted_capped_and_alone_is_a_context() {
        let (h, rec, token) = handler(Config::default());
        let long = "é".repeat(500);
        for ctx in [
            r#"{"title":"My task"}"#.to_owned(),
            format!(r#"{{"title":"{long}"}}"#),
            r#"{"title":""}"#.to_owned(),
        ] {
            let (r, _) = roundtrip(&h, &request_with_context(token.expose(), &ctx)).await;
            assert!(r.ok && r.error.is_none());
        }
        let (r, _) = roundtrip(&h, &request_with_context(token.expose(), r#"{"title":5}"#)).await;
        assert_eq!(r.error, Some(ErrorCode::Malformed));
        let got = rec.contexts.lock().expect("lock").clone();
        assert_eq!(
            got[0].as_ref().and_then(|c| c.title.as_deref()),
            Some("My task")
        );
        let capped = got[1].as_ref().and_then(|c| c.title.as_ref());
        assert_eq!(capped.map(|t| t.chars().count()), Some(120));
        assert_eq!(got[2], None);
    }

    #[tokio::test]
    async fn context_is_capped_and_empty_context_is_none() {
        let (h, rec, token) = handler(Config::default());
        let long = "é".repeat(5000);
        let req = request_with_context(
            token.expose(),
            &format!(r#"{{"intent":"{long}","ask":"{long}"}}"#),
        );
        let (r, _) = roundtrip(&h, &req).await;
        assert_eq!(r, Reply::stored());
        let empty = request_with_context(token.expose(), r#"{"intent":"","ask":""}"#);
        let (r, _) = roundtrip(&h, &empty).await;
        assert_eq!(r.error, None);
        let got = rec.contexts.lock().expect("lock").clone();
        let first = got[0].as_ref().expect("context");
        assert_eq!(first.intent.as_ref().expect("intent").chars().count(), 2000);
        assert_eq!(first.ask.as_ref().expect("ask").chars().count(), 300);
        assert_eq!(got[1], None, "empty strings are no context");
    }

    #[tokio::test]
    async fn unknown_context_fields_are_rejected_without_echo() {
        const SENTINEL: &str = "SENTINEL_CONTEXT_FIELD";
        let (h, rec, token) = handler(Config::default());
        for ctx in [
            format!(r#"{{"intent":"x","{SENTINEL}":"y"}}"#),
            r#"{"intent":5}"#.to_owned(),
            r#""just a string""#.to_owned(),
        ] {
            let (r, raw) = roundtrip(&h, &request_with_context(token.expose(), &ctx)).await;
            assert_eq!(r.error, Some(ErrorCode::Malformed));
            assert!(!raw.contains(SENTINEL));
        }
        assert_eq!(rec.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn context_is_not_delivered_for_unauthorized_requests() {
        let (h, rec, _) = handler(Config::default());
        let wrong = Token::generate().expect("token");
        let (r, _) = roundtrip(
            &h,
            &request_with_context(wrong.expose(), r#"{"intent":"secret"}"#),
        )
        .await;
        assert_eq!(r.error, Some(ErrorCode::Unauthorized));
        assert_eq!(rec.calls.load(Ordering::SeqCst), 0);
    }
}
