//! Streaming provider calls against a local mock server on loopback (S12-T15). No real network.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use companion::{
    CancelToken, ChatMessage, ErrorKind, HttpConfig, LateUsage, ProviderKind, ProviderSpec,
    RequestPlan, Role, Secret, chat_stream, tools::ToolCall,
};

const KEY: &str = "sk-test-SUPERSECRETKEY-0123456789";

enum Step {
    Write(&'static str),
    Bytes(Vec<u8>),
    Sleep(u64),
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

/// Serves one scripted response per connection: head, then the steps, then closes.
fn start(content_type: &'static str, steps: fn() -> Vec<Step>) -> Mock {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&requests);
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(mut s) = conn else { continue };
            let log = Arc::clone(&log);
            std::thread::spawn(move || {
                let Some(req) = read_request(&mut s) else {
                    return;
                };
                log.lock().unwrap().push(req);
                let head = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: {content_type}\r\nconnection: close\r\n\r\n"
                );
                if s.write_all(head.as_bytes()).is_err() {
                    return;
                }
                for step in steps() {
                    let r = match step {
                        Step::Write(t) => s.write_all(t.as_bytes()),
                        Step::Bytes(b) => s.write_all(&b),
                        Step::Sleep(ms) => {
                            std::thread::sleep(Duration::from_millis(ms));
                            Ok(())
                        }
                    };
                    if r.is_err() || s.flush().is_err() {
                        return;
                    }
                }
            });
        }
    });
    Mock {
        base: format!("http://127.0.0.1:{port}"),
        requests,
    }
}

fn spec(kind: ProviderKind, base: &str) -> ProviderSpec {
    ProviderSpec {
        kind,
        base_url: base.to_owned(),
        model: "m".to_owned(),
    }
}

fn msgs() -> Vec<ChatMessage> {
    vec![ChatMessage {
        role: Role::User,
        content: "hello there, this is a request of some size".to_owned(),
    }]
}

type Streamed = (
    Result<companion::StreamReply, companion::StreamFailure>,
    Vec<String>,
);

fn run_with(
    mock: &Mock,
    kind: ProviderKind,
    cfg: HttpConfig,
    tools: bool,
    cancel: &CancelToken,
    late: Option<mpsc::Sender<LateUsage>>,
) -> Streamed {
    run_opts(mock, kind, cfg, tools, (true, false), cancel, late)
}

/// `opts` is `(stream, reasoning)` as asked of the request.
fn run_opts(
    mock: &Mock,
    kind: ProviderKind,
    cfg: HttpConfig,
    tools: bool,
    opts: (bool, bool),
    cancel: &CancelToken,
    late: Option<mpsc::Sender<LateUsage>>,
) -> Streamed {
    let key = Secret::parse(KEY).unwrap();
    let m = msgs();
    let plan = RequestPlan {
        messages: &m,
        rounds: &[],
        tools,
        stream: opts.0,
        reasoning: opts.1,
        legacy_thinking: false,
        no_summary: false,
        max_output_tokens: 64,
        system: None,
    };
    let mut deltas = Vec::new();
    let r = chat_stream(
        &cfg,
        &spec(kind, &mock.base),
        &key,
        &plan,
        cancel,
        |d| deltas.push(d.to_owned()),
        |_| {},
        move |u| {
            if let Some(tx) = late {
                let _ = tx.send(u);
            }
        },
    );
    (r, deltas)
}

fn run(mock: &Mock, kind: ProviderKind, cfg: HttpConfig, tools: bool) -> Streamed {
    run_with(mock, kind, cfg, tools, &CancelToken::new(), None)
}

fn openai_ok() -> Vec<Step> {
    vec![
        Step::Write(
            "data: {\"choices\":[{\"delta\":{\"role\":\"assistant\",\"content\":\"\"}}]}\r\n\r\ndata: {\"choices\":[{\"delta\":{\"content\":\"Hel",
        ),
        Step::Sleep(30),
        Step::Write(
            "lo\"}}]}\r\n\r\n: keep-alive\r\n\r\ndata: {\"choices\":[{\"delta\":{\"content\":\" wor\\u202eld\\u0007\"}}]}\n\ndata: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
        ),
        Step::Write(
            "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":21,\"completion_tokens\":5}}\n\ndata: [DONE]\n\n",
        ),
    ]
}

#[test]
fn chat_completions_stream_is_cleaned_chunk_by_chunk_and_usage_is_reported() {
    let mock = start("text/event-stream", openai_ok);
    let (r, deltas) = run(
        &mock,
        ProviderKind::OpenAiCompatible,
        HttpConfig::default(),
        false,
    );
    let r = r.unwrap();
    assert_eq!(r.text, "Hello world");
    // Cleaning is per chunk: the bidi override and the bell never reach the caller.
    assert_eq!(deltas.concat(), "Hello world");
    assert!(deltas.len() >= 2);
    assert_eq!((r.input_tokens, r.output_tokens, r.reported), (21, 5, true));
    let req = mock.requests.lock().unwrap()[0].clone();
    assert!(req.contains("\"stream\":true"));
    assert!(!req.contains("include_usage"));
    assert!(!req.contains("\"tools\""));
}

#[test]
fn compatible_servers_get_no_stream_options_and_usage_is_estimated() {
    fn steps() -> Vec<Step> {
        vec![Step::Write(
            "data: {\"choices\":[{\"delta\":{\"content\":\"abcdefgh\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n",
        )]
    }
    let mock = start("text/event-stream", steps);
    let (r, _) = run(
        &mock,
        ProviderKind::OpenAiCompatible,
        HttpConfig::default(),
        false,
    );
    let r = r.unwrap();
    assert!(!r.reported);
    assert!(r.input_tokens > 0 && r.output_tokens > 0);
    assert!(!mock.requests.lock().unwrap()[0].contains("stream_options"));
}

#[test]
fn anthropic_stream_with_a_tool_call() {
    fn steps() -> Vec<Step> {
        vec![
            Step::Write(
                "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":30,\"output_tokens\":1}}}\n\n",
            ),
            Step::Write(
                "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
            ),
            Step::Write("event: ping\ndata: {\"type\": \"ping\"}\n\n"),
            Step::Write(
                "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"One moment.\"}}\n\n",
            ),
            Step::Write(
                "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
            ),
            Step::Write(
                "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_01\",\"name\":\"suggest_break\",\"input\":{}}}\n\n",
            ),
            Step::Write(
                "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"available_minutes\\\": 4}\"}}\n\n",
            ),
            Step::Write(
                "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":1}\n\n",
            ),
            Step::Write(
                "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":17}}\n\n",
            ),
            Step::Write("event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"),
        ]
    }
    let mock = start("text/event-stream; charset=utf-8", steps);
    let (r, _) = run(&mock, ProviderKind::Anthropic, HttpConfig::default(), true);
    let r = r.unwrap();
    assert_eq!(r.text, "One moment.");
    assert_eq!(
        r.tool_calls,
        vec![ToolCall {
            id: "toolu_01".into(),
            name: "suggest_break".into(),
            arguments: "{\"available_minutes\": 4}".into()
        }]
    );
    assert_eq!(
        (r.input_tokens, r.output_tokens, r.reported),
        (30, 17, true)
    );
    let req = mock.requests.lock().unwrap()[0].clone();
    assert!(req.contains("\"stream\":true"));
    assert!(req.contains("\"input_schema\""));
    assert!(req.contains("session_overview") && req.contains("activity_help"));
}

#[test]
fn tool_calls_are_ignored_when_tools_were_not_offered() {
    fn steps() -> Vec<Step> {
        vec![Step::Write(
            "data: {\"choices\":[{\"delta\":{\"content\":\"hi\",\"tool_calls\":[{\"index\":0,\"id\":\"c1\",\"function\":{\"name\":\"suggest_break\",\"arguments\":\"{}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\ndata: [DONE]\n\n",
        )]
    }
    let mock = start("text/event-stream", steps);
    let (r, _) = run(
        &mock,
        ProviderKind::OpenAiCompatible,
        HttpConfig::default(),
        false,
    );
    assert!(r.unwrap().tool_calls.is_empty());
}

#[test]
fn a_dropped_stream_fails_cleanly_and_counts_the_partial_reply() {
    fn steps() -> Vec<Step> {
        vec![Step::Write(
            "data: {\"choices\":[{\"delta\":{\"content\":\"partial text of some length\"}}]}\n\n",
        )]
    }
    let mock = start("text/event-stream", steps);
    let (r, deltas) = run(
        &mock,
        ProviderKind::OpenAiCompatible,
        HttpConfig::default(),
        false,
    );
    let f = r.unwrap_err();
    assert_eq!(f.error.kind, ErrorKind::ProviderError);
    assert_eq!(deltas.concat(), "partial text of some length");
    assert_eq!(f.partial, "partial text of some length");
    assert!(f.received);
    // Estimated from the request and the text received: both are counted.
    assert!(f.input_tokens > 0 && f.output_tokens > 0);
}

#[test]
fn malformed_events_are_clean_errors() {
    fn bad_json() -> Vec<Step> {
        vec![Step::Write("data: {not json}\n\n")]
    }
    fn bad_utf8() -> Vec<Step> {
        vec![Step::Bytes(b"data: \xff\xfe\n\n".to_vec())]
    }
    for steps in [bad_json as fn() -> Vec<Step>, bad_utf8] {
        for kind in [ProviderKind::OpenAi, ProviderKind::Anthropic] {
            let mock = start("text/event-stream", steps);
            let (r, _) = run(&mock, kind, HttpConfig::default(), false);
            let f = r.unwrap_err();
            assert_eq!(f.error.kind, ErrorKind::InvalidResponse);
            assert!(!f.error.message.contains(KEY));
        }
    }
}

#[test]
fn oversized_events_and_streams_are_refused() {
    fn huge_line() -> Vec<Step> {
        vec![Step::Bytes(
            [b"data: ".as_slice(), &vec![b'a'; 4096], b"\n\n"].concat(),
        )]
    }
    let cfg = HttpConfig {
        max_event_bytes: 1024,
        ..HttpConfig::default()
    };
    let mock = start("text/event-stream", huge_line);
    let (r, _) = run(&mock, ProviderKind::OpenAiCompatible, cfg, false);
    assert_eq!(r.unwrap_err().error.kind, ErrorKind::InvalidResponse);

    fn endless_pings() -> Vec<Step> {
        (0..200)
            .map(|_| Step::Write(": ping ping ping ping\n"))
            .collect()
    }
    let cfg = HttpConfig {
        max_stream_bytes: 1024,
        ..HttpConfig::default()
    };
    let mock = start("text/event-stream", endless_pings);
    let (r, _) = run(&mock, ProviderKind::OpenAiCompatible, cfg, false);
    assert_eq!(r.unwrap_err().error.kind, ErrorKind::InvalidResponse);
}

#[test]
fn a_reply_over_the_byte_limit_is_cut_and_the_stream_dropped() {
    fn steps() -> Vec<Step> {
        let mut v = Vec::new();
        for _ in 0..50 {
            v.push(Step::Write(
                "data: {\"choices\":[{\"delta\":{\"content\":\"0123456789\"}}]}\n\n",
            ));
        }
        v
    }
    let cfg = HttpConfig {
        max_reply_bytes: 25,
        ..HttpConfig::default()
    };
    let mock = start("text/event-stream", steps);
    let (r, deltas) = run(&mock, ProviderKind::OpenAiCompatible, cfg, false);
    let r = r.unwrap();
    assert!(r.cut);
    assert_eq!(r.text.len(), 25);
    assert_eq!(deltas.concat(), r.text);
}

#[test]
fn silence_after_the_first_chunk_times_out_and_the_late_usage_is_booked() {
    fn steps() -> Vec<Step> {
        vec![
            Step::Write("data: {\"choices\":[{\"delta\":{\"content\":\"started\"}}]}\n\n"),
            Step::Sleep(3000),
        ]
    }
    let cfg = HttpConfig {
        idle_timeout: Duration::from_millis(300),
        ..HttpConfig::default()
    };
    let mock = start("text/event-stream", steps);
    let (tx, rx) = mpsc::channel();
    let t = Instant::now();
    let (r, deltas) = run_with(
        &mock,
        ProviderKind::OpenAiCompatible,
        cfg,
        false,
        &CancelToken::new(),
        Some(tx),
    );
    assert_eq!(r.unwrap_err().error.kind, ErrorKind::Timeout);
    assert!(t.elapsed() < Duration::from_secs(2));
    assert_eq!(deltas.concat(), "started");
    // The worker is still blocked in the read; once the server closes it books what the partial
    // reply cost.
    let u = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("late usage");
    assert!(!u.reported);
    assert!(u.input_tokens > 0 && u.output_tokens > 0);
}

#[test]
fn keep_alive_pings_reset_the_idle_timer() {
    fn steps() -> Vec<Step> {
        let mut v = vec![Step::Write(
            "data: {\"choices\":[{\"delta\":{\"content\":\"a\"}}]}\n\n",
        )];
        for _ in 0..6 {
            v.push(Step::Sleep(150));
            v.push(Step::Write(": ping\n\n"));
        }
        v.push(Step::Write(
            "data: {\"choices\":[{\"delta\":{\"content\":\"b\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n",
        ));
        v
    }
    let cfg = HttpConfig {
        idle_timeout: Duration::from_millis(400),
        ..HttpConfig::default()
    };
    let mock = start("text/event-stream", steps);
    let (r, _) = run(&mock, ProviderKind::OpenAiCompatible, cfg, false);
    assert_eq!(r.unwrap().text, "ab");
}

#[test]
fn no_first_byte_times_out() {
    fn steps() -> Vec<Step> {
        vec![Step::Sleep(3000)]
    }
    let cfg = HttpConfig {
        first_byte_timeout: Duration::from_millis(300),
        ..HttpConfig::default()
    };
    let mock = start("text/event-stream", steps);
    let (r, _) = run(&mock, ProviderKind::OpenAiCompatible, cfg, false);
    let f = r.unwrap_err();
    assert_eq!(f.error.kind, ErrorKind::Timeout);
    assert!(!f.received);
}

#[test]
fn cancel_returns_at_once_discards_late_chunks_and_books_usage() {
    // The server pauses 3 s after the first chunk. The cancel is fired by the first chunk itself,
    // so it no longer depends on how fast the request was made, and "at once" is measured from
    // the cancel to the return, with a bound (1.5 s) far under the pause: a call that waited for
    // the stream to go on would take the full 3 s.
    fn steps() -> Vec<Step> {
        vec![
            Step::Write("data: {\"choices\":[{\"delta\":{\"content\":\"early\"}}]}\n\n"),
            Step::Sleep(3000),
            Step::Write("data: {\"choices\":[{\"delta\":{\"content\":\"LATE\"}}]}\n\n"),
            Step::Write(
                "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n",
            ),
        ]
    }
    let mock = start("text/event-stream", steps);
    let cancel = CancelToken::new();
    let (tx, rx) = mpsc::channel();
    let key = Secret::parse(KEY).unwrap();
    let m = msgs();
    let plan = RequestPlan {
        messages: &m,
        rounds: &[],
        tools: false,
        stream: true,
        reasoning: false,
        legacy_thinking: false,
        no_summary: false,
        max_output_tokens: 64,
        system: None,
    };
    let mut deltas: Vec<String> = Vec::new();
    let mut cancelled_at: Option<Instant> = None;
    let r = chat_stream(
        &HttpConfig::default(),
        &spec(ProviderKind::OpenAiCompatible, &mock.base),
        &key,
        &plan,
        &cancel,
        |d| {
            deltas.push(d.to_owned());
            if cancelled_at.is_none() {
                cancelled_at = Some(Instant::now());
                cancel.cancel();
            }
        },
        |_| {},
        move |u| {
            let _ = tx.send(u);
        },
    );
    let returned = cancelled_at.expect("the first chunk arrived").elapsed();
    assert_eq!(r.unwrap_err().error.kind, ErrorKind::Cancelled);
    assert!(returned < Duration::from_millis(1500), "returned at once");
    assert_eq!(deltas.concat(), "early");
    let u = rx
        .recv_timeout(Duration::from_secs(15))
        .expect("late usage");
    assert!(u.input_tokens > 0);
    // Nothing late reached the caller.
    assert!(!deltas.concat().contains("LATE"));
}

#[test]
fn a_server_that_ignores_stream_true_is_read_as_one_chunk() {
    fn steps() -> Vec<Step> {
        vec![Step::Write(
            "{\"choices\":[{\"message\":{\"content\":\"whole reply\"}}],\"usage\":{\"prompt_tokens\":4,\"completion_tokens\":2}}",
        )]
    }
    let mock = start("application/json", steps);
    let (r, deltas) = run(
        &mock,
        ProviderKind::OpenAiCompatible,
        HttpConfig::default(),
        false,
    );
    let r = r.unwrap();
    assert_eq!(r.text, "whole reply");
    assert_eq!(deltas, vec!["whole reply".to_owned()]);
    assert_eq!((r.input_tokens, r.output_tokens), (4, 2));
}

#[test]
fn http_errors_map_to_fixed_messages_with_the_sanitised_reason_and_count_nothing() {
    let mock = start_scripted(vec![Resp::json(401, "{\"error\":\"PROVIDER-BODY\"}")]);
    let (r, _) = run(&mock, ProviderKind::OpenAi, HttpConfig::default(), false);
    let f = r.unwrap_err();
    assert_eq!(f.error.kind, ErrorKind::KeyRejected);
    assert_eq!((f.input_tokens, f.output_tokens, f.received), (0, 0, false));
    assert!(
        said(&f.error).ends_with("OpenAI said: \"PROVIDER-BODY\""),
        "{}",
        said(&f.error)
    );
}

#[test]
fn stream_error_excerpts_cover_shapes_html_oversize_key_and_bidi() {
    let cases: Vec<(ProviderKind, Resp, &str)> = vec![
        (
            ProviderKind::Anthropic,
            Resp::json(
                400,
                r#"{"type":"error","error":{"type":"invalid_request_error","message":"max_tokens: too large"}}"#,
            ),
            "Anthropic said: \"max_tokens: too large\"",
        ),
        (
            ProviderKind::OpenAi,
            Resp::json(
                400,
                "{\"error\":{\"message\":\"bad\\u202e \\u0007thing\\n\\nhere\"}}",
            ),
            "OpenAI said: \"bad thing here\"",
        ),
    ];
    for (kind, resp, expect) in cases {
        let mock = start_scripted(vec![resp]);
        let (r, _) = run(&mock, kind, HttpConfig::default(), false);
        let m = said(&r.unwrap_err().error);
        assert!(m.ends_with(expect), "{m}");
    }
    // HTML and oversized bodies: fixed message only.
    let mut big = br#"{"error":{"message":"BIG-REASON"}}"#.to_vec();
    big.resize(64 * 1024, b' ');
    for resp in [
        Resp::json(502, "<html>Bad gateway</html>"),
        Resp::json(400, &String::from_utf8(big).unwrap()),
    ] {
        let mock = start_scripted(vec![resp]);
        let (r, _) = run(&mock, ProviderKind::OpenAi, HttpConfig::default(), false);
        let m = said(&r.unwrap_err().error);
        assert!(
            !m.contains("BIG-REASON") && !m.contains("said:") && !m.contains("html"),
            "{m}"
        );
    }
    // A body that contains the key.
    let mock = start_scripted(vec![Resp::json(
        400,
        &format!("{{\"error\":{{\"message\":\"you sent {KEY} oops\"}}}}"),
    )]);
    let (r, _) = run(&mock, ProviderKind::OpenAi, HttpConfig::default(), false);
    let m = said(&r.unwrap_err().error);
    assert!(
        !m.contains("SUPERSECRETKEY") && m.contains("[hidden]"),
        "{m}"
    );
    assert!(m.chars().count() <= 500);
}

struct Resp {
    status: u16,
    content_type: &'static str,
    body: String,
    delay_ms: u64,
}

impl Resp {
    fn json(status: u16, body: &str) -> Self {
        Self {
            status,
            content_type: "application/json",
            body: body.to_owned(),
            delay_ms: 0,
        }
    }
}

impl Resp {
    /// Waits this long after reading the request before answering.
    fn after(mut self, ms: u64) -> Self {
        self.delay_ms = ms;
        self
    }
}

/// Answers the n-th connection with the n-th scripted response (the last one repeats).
fn start_scripted(script: Vec<Resp>) -> Mock {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&requests);
    std::thread::spawn(move || {
        let mut n = 0usize;
        for conn in listener.incoming() {
            let Ok(mut s) = conn else { continue };
            let Some(req) = read_request(&mut s) else {
                continue;
            };
            log.lock().unwrap().push(req);
            let r = &script[n.min(script.len() - 1)];
            std::thread::sleep(Duration::from_millis(r.delay_ms));
            n += 1;
            let _ = write!(
                s,
                "HTTP/1.1 {} X\r\ncontent-type: {}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                r.status,
                r.content_type,
                r.body.len(),
                r.body
            );
            let _ = s.flush();
        }
    });
    Mock {
        base: format!("http://127.0.0.1:{port}"),
        requests,
    }
}

// ---- OpenAI Responses API (official OpenAI kind) -------------------------------------------

impl Resp {
    fn sse(body: &str) -> Self {
        Self {
            status: 200,
            content_type: "text/event-stream",
            body: body.to_owned(),
            delay_ms: 0,
        }
    }
}

fn ev(json: &str) -> String {
    format!("event: x\ndata: {json}\n\n")
}

fn responses_text_stream() -> String {
    [
        r#"{"type":"response.created","response":{"id":"resp_1","status":"in_progress"}}"#,
        r#"{"type":"response.output_text.delta","item_id":"msg_1","output_index":0,"content_index":0,"delta":"Hel"}"#,
        r#"{"type":"response.output_text.delta","item_id":"msg_1","output_index":0,"content_index":0,"delta":"loBIDI wor\u0007ld"}"#,
        r#"{"type":"response.completed","response":{"id":"resp_1","status":"completed","usage":{"input_tokens":21,"output_tokens":5,"total_tokens":26}}}"#,
    ]
    .iter()
    .map(|e| ev(&e.replace("BIDI", "\\u202e")))
    .collect()
}

/// The provider's words as the UI shows them: `<speaker> said: "<text>"` (empty if none).
fn said(e: &companion::CompanionError) -> String {
    e.reason
        .as_ref()
        .map(|r| format!("{} said: \"{}\"", r.speaker, r.text))
        .unwrap_or_default()
}

fn body_of(req: &str) -> serde_json::Value {
    serde_json::from_str(req.split_once("\r\n\r\n").unwrap().1).unwrap()
}

#[test]
fn responses_stream_is_sent_to_responses_cleaned_and_usage_is_reported() {
    let mock = start_scripted(vec![Resp::sse(&responses_text_stream())]);
    let (r, deltas) = run(&mock, ProviderKind::OpenAi, HttpConfig::default(), false);
    let r = r.unwrap();
    assert_eq!(r.text, "Hello world");
    assert_eq!(deltas.concat(), "Hello world");
    assert_eq!((r.input_tokens, r.output_tokens, r.reported), (21, 5, true));
    let reqs = mock.requests.lock().unwrap();
    assert_eq!(reqs.len(), 1);
    assert!(reqs[0].starts_with("POST /responses "), "{}", reqs[0]);
    let b = body_of(&reqs[0]);
    assert_eq!(b["stream"], true);
    assert_eq!(b["store"], false);
    assert_eq!(b["max_output_tokens"], 64);
    assert!(b.get("stream_options").is_none() && b.get("tools").is_none());
    assert!(!b.to_string().contains("SUPERSECRET"));
}

#[test]
fn responses_tool_call_round_trip() {
    let calls = [
        r#"{"type":"response.output_item.added","output_index":0,"item":{"id":"fc_1","type":"function_call","call_id":"call_abc","name":"suggest_break","arguments":""}}"#,
        r#"{"type":"response.function_call_arguments.delta","item_id":"fc_1","output_index":0,"delta":"{\"available_minutes\":5}"}"#,
        r#"{"type":"response.output_item.done","output_index":0,"item":{"id":"fc_1","type":"function_call","call_id":"call_abc","name":"suggest_break","arguments":"{\"available_minutes\":5}"}}"#,
        r#"{"type":"response.completed","response":{"usage":{"input_tokens":30,"output_tokens":9}}}"#,
    ]
    .iter()
    .map(|e| ev(e))
    .collect::<String>();
    let mock = start_scripted(vec![Resp::sse(&calls), Resp::sse(&responses_text_stream())]);
    let key = Secret::parse(KEY).unwrap();
    let m = msgs();
    let cancel = CancelToken::new();
    let first = chat_stream(
        &HttpConfig::default(),
        &spec(ProviderKind::OpenAi, &mock.base),
        &key,
        &RequestPlan {
            messages: &m,
            rounds: &[],
            tools: true,
            stream: true,
            reasoning: false,
            legacy_thinking: false,
            no_summary: false,
            max_output_tokens: 64,
            system: None,
        },
        &cancel,
        |_| {},
        |_| {},
        |_| {},
    )
    .unwrap();
    assert_eq!(first.tool_calls.len(), 1);
    assert_eq!(first.tool_calls[0].id, "call_abc");
    assert_eq!(first.tool_calls[0].arguments, "{\"available_minutes\":5}");
    assert_eq!((first.input_tokens, first.output_tokens), (30, 9));
    // The approved answer goes back as function_call / function_call_output items.
    let rounds = [companion::ToolExchange {
        text: String::new(),
        calls: first.tool_calls.clone(),
        results: vec!["Tea Sort, about 5 minutes.".to_owned()],
    }];
    let second = chat_stream(
        &HttpConfig::default(),
        &spec(ProviderKind::OpenAi, &mock.base),
        &key,
        &RequestPlan {
            messages: &m,
            rounds: &rounds,
            tools: true,
            stream: true,
            reasoning: false,
            legacy_thinking: false,
            no_summary: false,
            max_output_tokens: 64,
            system: None,
        },
        &cancel,
        |_| {},
        |_| {},
        |_| {},
    )
    .unwrap();
    assert_eq!(second.text, "Hello world");
    let reqs = mock.requests.lock().unwrap();
    let b = body_of(&reqs[0]);
    assert_eq!(b["tools"].as_array().unwrap().len(), 3);
    assert_eq!(b["tools"][0]["strict"], false);
    let input = body_of(&reqs[1])["input"].as_array().unwrap().clone();
    assert_eq!(input.len(), 3); // user, function_call, function_call_output
    assert_eq!(input[1]["type"], "function_call");
    assert_eq!(input[1]["call_id"], "call_abc");
    assert_eq!(input[2]["type"], "function_call_output");
    assert_eq!(input[2]["call_id"], "call_abc");
    assert_eq!(input[2]["output"], "Tea Sort, about 5 minutes.");
}

#[test]
fn responses_failure_events_are_errors_with_the_reason() {
    let failed = ev(
        r#"{"type":"response.failed","response":{"status":"failed","error":{"code":"server_error","message":"The model failed to generate a response."}}}"#,
    );
    let mock = start_scripted(vec![Resp::sse(&failed)]);
    let (r, _) = run(&mock, ProviderKind::OpenAi, HttpConfig::default(), false);
    let m = said(&r.unwrap_err().error);
    assert!(
        m.ends_with("OpenAI said: \"The model failed to generate a response.\""),
        "{m}"
    );
    // A stream that just ends is "stopped early", not a reply.
    let cut = ev(r#"{"type":"response.output_text.delta","delta":"par"}"#);
    let mock = start_scripted(vec![Resp::sse(&cut)]);
    let (r, _) = run(&mock, ProviderKind::OpenAi, HttpConfig::default(), false);
    let f = r.unwrap_err();
    assert_eq!(f.error.kind, ErrorKind::ProviderError);
    assert_eq!(f.partial, "par");
}

fn refuse_stream() -> Resp {
    Resp::json(
        400,
        r#"{"error":{"message":"Your organization must be verified to stream this model.","type":"invalid_request_error","param":"stream","code":"unsupported_value"}}"#,
    )
}

fn responses_json() -> String {
    RESPONSES_JSON_RAW.replace("BIDI", "\\u202e")
}

const RESPONSES_JSON_RAW: &str = r#"{"id":"resp_2","status":"completed","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"WholeBIDI reply"}]}],"usage":{"input_tokens":12,"output_tokens":3}}"#;

#[test]
fn a_refused_stream_is_retried_once_without_streaming_and_reported_as_metadata() {
    let mock = start_scripted(vec![refuse_stream(), Resp::json(200, &responses_json())]);
    let (r, deltas) = run(&mock, ProviderKind::OpenAi, HttpConfig::default(), false);
    let r = r.unwrap();
    // The reply is the model's words only; the fact that it was not streamed is metadata.
    assert_eq!(r.text, "Whole reply");
    assert!(!r.streamed);
    assert!(r.learned.stream_refused && !r.learned.reasoning_refused);
    assert_eq!(deltas.concat(), "Whole reply");
    assert_eq!((r.input_tokens, r.output_tokens), (12, 3));
    let reqs = mock.requests.lock().unwrap();
    assert_eq!(reqs.len(), 2);
    assert_eq!(body_of(&reqs[0])["stream"], true);
    let second = body_of(&reqs[1]);
    assert!(second.get("stream").is_none());
    assert_eq!(second["store"], false);
    assert!(reqs[1].starts_with("POST /responses "));
}

#[test]
fn the_non_streaming_retry_keeps_tool_calls() {
    let json = r#"{"status":"completed","output":[{"type":"function_call","id":"fc_1","call_id":"call_z","name":"suggest_break","arguments":"{}"}],"usage":{"input_tokens":5,"output_tokens":2}}"#;
    let mock = start_scripted(vec![refuse_stream(), Resp::json(200, json)]);
    let (r, _) = run(&mock, ProviderKind::OpenAi, HttpConfig::default(), true);
    let r = r.unwrap();
    assert_eq!(r.tool_calls.len(), 1);
    assert_eq!(r.tool_calls[0].id, "call_z");
    assert!(r.text.is_empty());
}

#[test]
fn the_retry_happens_once_and_only_for_openai_and_only_for_the_stream_param() {
    // The retry is refused too: that error is shown, with no third request.
    let mock = start_scripted(vec![refuse_stream(), refuse_stream()]);
    let (r, _) = run(&mock, ProviderKind::OpenAi, HttpConfig::default(), false);
    let m = said(&r.unwrap_err().error);
    assert!(
        m.contains("OpenAI said: \"Your organization must be verified"),
        "{m}"
    );
    assert_eq!(mock.requests.lock().unwrap().len(), 2);
    // Another 400 (a different param) is not retried.
    let other = Resp::json(
        400,
        r#"{"error":{"message":"Unsupported parameter: store","param":"store"}}"#,
    );
    let mock = start_scripted(vec![other]);
    let (r, _) = run(&mock, ProviderKind::OpenAi, HttpConfig::default(), false);
    assert!(said(&r.unwrap_err().error).contains("Unsupported parameter: store"));
    assert_eq!(mock.requests.lock().unwrap().len(), 1);
    // Compatible servers and Anthropic are never retried, whatever the body says.
    for kind in [ProviderKind::OpenAiCompatible, ProviderKind::Anthropic] {
        let mock = start_scripted(vec![refuse_stream(), Resp::json(200, &responses_json())]);
        let (r, _) = run(&mock, kind, HttpConfig::default(), false);
        assert!(r.is_err());
        assert_eq!(mock.requests.lock().unwrap().len(), 1, "{kind:?}");
    }
}

#[test]
fn compatible_providers_still_use_chat_completions_with_max_tokens() {
    let ok = ev(r#"{"choices":[{"delta":{"content":"hi there"},"finish_reason":"stop"}]}"#)
        + "data: [DONE]\n\n";
    let mock = start_scripted(vec![Resp::sse(&ok)]);
    let (r, _) = run(
        &mock,
        ProviderKind::OpenAiCompatible,
        HttpConfig::default(),
        false,
    );
    assert_eq!(r.unwrap().text, "hi there");
    let reqs = mock.requests.lock().unwrap();
    assert!(reqs[0].starts_with("POST /chat/completions "));
    let b = body_of(&reqs[0]);
    assert_eq!(b["max_tokens"], 64);
    assert!(b.get("store").is_none() && b.get("input").is_none());
}

// ---- failures that cost tokens, refused options, retries (M1, M2, M3, L1, L3, L5) ----------

fn plain() -> CancelToken {
    CancelToken::new()
}

fn failed_stream(usage: &str) -> String {
    ev(&format!(
        r#"{{"type":"response.failed","response":{{"status":"failed","error":{{"message":"The model failed."}}{usage}}}}}"#
    ))
}

#[test]
fn m1_a_failed_stream_books_what_it_reported_else_the_output_cap() {
    let reported = failed_stream(r#","usage":{"input_tokens":30,"output_tokens":7}"#);
    let mock = start_scripted(vec![Resp::sse(&reported)]);
    let (r, _) = run(&mock, ProviderKind::OpenAi, HttpConfig::default(), false);
    let f = r.unwrap_err();
    assert_eq!((f.input_tokens, f.output_tokens), (30, 7));
    // Nothing reported: the request's output cap (64 here) is booked, not zero.
    let silent = failed_stream("");
    let mock = start_scripted(vec![Resp::sse(&silent)]);
    let (r, _) = run(&mock, ProviderKind::OpenAi, HttpConfig::default(), false);
    let f = r.unwrap_err();
    assert!(f.received && f.input_tokens > 0);
    assert_eq!(f.output_tokens, 64);
}

#[test]
fn m1_an_incomplete_stream_without_text_books_its_usage_and_says_why() {
    let inc = ev(
        r#"{"type":"response.incomplete","response":{"status":"incomplete","incomplete_details":{"reason":"max_output_tokens"},"usage":{"input_tokens":12,"output_tokens":64}}}"#,
    );
    let mock = start_scripted(vec![Resp::sse(&inc)]);
    let (r, _) = run(&mock, ProviderKind::OpenAi, HttpConfig::default(), false);
    let f = r.unwrap_err();
    assert_eq!((f.input_tokens, f.output_tokens), (12, 64));
    assert!(
        f.error.message.contains("output limit"),
        "{}",
        f.error.message
    );
    let filtered = ev(
        r#"{"type":"response.incomplete","response":{"incomplete_details":{"reason":"content_filter"}}}"#,
    );
    let mock = start_scripted(vec![Resp::sse(&filtered)]);
    let (r, _) = run(&mock, ProviderKind::OpenAi, HttpConfig::default(), false);
    let f = r.unwrap_err();
    assert!(f.error.message.contains("content_filter"));
    assert_eq!(f.output_tokens, 64, "no usage reported: the cap");
}

#[test]
fn m1_non_stream_failures_book_reported_usage_else_the_cap() {
    let with = r#"{"status":"failed","error":{"message":"x"},"usage":{"input_tokens":5,"output_tokens":9},"output":[]}"#;
    let without = r#"{"status":"failed","error":{"message":"x"},"output":[]}"#;
    for (body, want_out) in [(with, 9), (without, 64)] {
        let mock = start_scripted(vec![Resp::json(200, body)]);
        let (r, _) = run_opts(
            &mock,
            ProviderKind::OpenAi,
            HttpConfig::default(),
            false,
            (false, false),
            &plain(),
            None,
        );
        let f = r.unwrap_err();
        assert!(f.received);
        assert_eq!(f.output_tokens, want_out, "{body}");
    }
}

fn reasoning_refused() -> Resp {
    Resp::json(
        400,
        r#"{"error":{"message":"Unsupported parameter: 'reasoning.effort' is not supported with this model.","type":"invalid_request_error","param":"reasoning.effort","code":"unsupported_parameter"}}"#,
    )
}

#[test]
fn m2_reasoning_is_sent_and_dropped_once_when_the_model_refuses_it() {
    let mock = start_scripted(vec![
        reasoning_refused(),
        Resp::sse(&responses_text_stream()),
    ]);
    let (r, _) = run_opts(
        &mock,
        ProviderKind::OpenAi,
        HttpConfig::default(),
        false,
        (true, true),
        &plain(),
        None,
    );
    let r = r.unwrap();
    assert!(r.learned.reasoning_refused && !r.learned.stream_refused);
    assert!(r.streamed);
    let reqs = mock.requests.lock().unwrap();
    assert_eq!(reqs.len(), 2);
    assert_eq!(body_of(&reqs[0])["reasoning"]["effort"], "low");
    assert!(body_of(&reqs[1]).get("reasoning").is_none());
    assert_eq!(body_of(&reqs[1])["stream"], true);
}

#[test]
fn m2_a_model_that_refuses_both_gets_three_requests_at_most() {
    let mock = start_scripted(vec![
        reasoning_refused(),
        refuse_stream(),
        Resp::json(200, &responses_json()),
    ]);
    let (r, _) = run_opts(
        &mock,
        ProviderKind::OpenAi,
        HttpConfig::default(),
        false,
        (true, true),
        &plain(),
        None,
    );
    let r = r.unwrap();
    assert!(r.learned.reasoning_refused && r.learned.stream_refused);
    assert!(!r.streamed);
    let reqs = mock.requests.lock().unwrap();
    assert_eq!(reqs.len(), 3);
    let last = body_of(&reqs[2]);
    assert!(last.get("reasoning").is_none() && last.get("stream").is_none());
    // Another model with a plain 400 is not retried at all.
    let mock = start_scripted(vec![Resp::json(
        400,
        r#"{"error":{"message":"bad","param":"input"}}"#,
    )]);
    let (r, _) = run_opts(
        &mock,
        ProviderKind::OpenAi,
        HttpConfig::default(),
        false,
        (true, true),
        &plain(),
        None,
    );
    assert!(r.is_err());
    assert_eq!(mock.requests.lock().unwrap().len(), 1);
}

#[test]
fn m2_reasoning_is_only_sent_to_openai() {
    let ok = ev(r#"{"choices":[{"delta":{"content":"hi there"},"finish_reason":"stop"}]}"#)
        + "data: [DONE]\n\n";
    for kind in [ProviderKind::OpenAiCompatible, ProviderKind::Anthropic] {
        let mock = start_scripted(vec![Resp::sse(&ok)]);
        let _ = run_opts(
            &mock,
            kind,
            HttpConfig::default(),
            false,
            (true, true),
            &plain(),
            None,
        );
        let reqs = mock.requests.lock().unwrap();
        assert!(body_of(&reqs[0]).get("reasoning").is_none(), "{kind:?}");
    }
}

#[test]
fn m3_the_retry_waits_for_the_total_time_not_the_first_byte_time() {
    // The first byte limit is 400 ms; the non-streamed answer takes 1.2 s to produce. Without
    // the retry signal the caller would give up at 400 ms.
    let mock = start_scripted(vec![
        refuse_stream(),
        Resp::json(200, &responses_json()).after(1200),
    ]);
    let cfg = HttpConfig {
        first_byte_timeout: Duration::from_millis(400),
        idle_timeout: Duration::from_millis(400),
        ..HttpConfig::default()
    };
    let (r, _) = run(&mock, ProviderKind::OpenAi, cfg, false);
    let r = r.expect("the retry was waited for");
    assert_eq!(r.text, "Whole reply");
    assert!(!r.streamed);
    // A request that never streams starts in that mode too.
    let mock = start_scripted(vec![Resp::json(200, &responses_json()).after(1200)]);
    let (r, _) = run_opts(
        &mock,
        ProviderKind::OpenAi,
        cfg,
        false,
        (false, false),
        &plain(),
        None,
    );
    assert!(r.is_ok());
}

#[test]
fn m3_a_remembered_refusal_means_one_request() {
    // The caller remembered that streaming was refused: it asks without it from the start.
    let mock = start_scripted(vec![Resp::json(200, &responses_json())]);
    let (r, _) = run_opts(
        &mock,
        ProviderKind::OpenAi,
        HttpConfig::default(),
        false,
        (false, false),
        &plain(),
        None,
    );
    let r = r.unwrap();
    assert!(!r.streamed && !r.learned.stream_refused);
    let reqs = mock.requests.lock().unwrap();
    assert_eq!(reqs.len(), 1);
    assert!(body_of(&reqs[0]).get("stream").is_none());
}

#[test]
fn m3_l3_a_cancel_during_the_refused_request_gives_up_and_books_the_cap() {
    // The refusal arrives after 500 ms; the user cancels at 150 ms. No retry may start, and the
    // late usage books the request, with the cap as output because a non-stream answer is
    // produced whole.
    let mock = start_scripted(vec![
        refuse_stream().after(500),
        Resp::json(200, &responses_json()),
    ]);
    let cancel = CancelToken::new();
    let c2 = cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(150));
        c2.cancel();
    });
    let (tx, rx) = mpsc::channel();
    let (r, _) = run_with(
        &mock,
        ProviderKind::OpenAi,
        HttpConfig::default(),
        false,
        &cancel,
        Some(tx),
    );
    assert_eq!(r.unwrap_err().error.kind, ErrorKind::Cancelled);
    let u = rx.recv_timeout(Duration::from_secs(5)).expect("late usage");
    assert!(u.input_tokens > 0);
    std::thread::sleep(Duration::from_millis(900));
    assert_eq!(
        mock.requests.lock().unwrap().len(),
        1,
        "no second request after a cancel"
    );
}

#[test]
fn m3_a_cancel_while_the_non_stream_answer_is_pending_books_the_cap() {
    let mock = start_scripted(vec![Resp::json(200, &responses_json()).after(800)]);
    let cancel = CancelToken::new();
    let c2 = cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(200));
        c2.cancel();
    });
    let (tx, rx) = mpsc::channel();
    let (r, _) = run_opts(
        &mock,
        ProviderKind::OpenAi,
        HttpConfig::default(),
        false,
        (false, false),
        &cancel,
        Some(tx),
    );
    assert_eq!(r.unwrap_err().error.kind, ErrorKind::Cancelled);
    let u = rx.recv_timeout(Duration::from_secs(5)).expect("late usage");
    // The answer did arrive after the cancel and says 3 output tokens; that is what it cost.
    assert!(u.output_tokens == 3 || u.output_tokens == 64, "{u:?}");
}

#[test]
fn l1_a_failure_event_that_quotes_what_we_sent_shows_no_reason() {
    // The sent message is "hello there, this is a request of some size".
    let quoting = ev(
        r#"{"type":"response.failed","response":{"error":{"message":"Cannot handle: helloZW there, this is a request of"}}}"#,
    )
    .replace("ZW", "\\u200b"); // a zero-width space inside the quote
    let mock = start_scripted(vec![Resp::sse(&quoting)]);
    let (r, _) = run(&mock, ProviderKind::OpenAi, HttpConfig::default(), false);
    let f = r.unwrap_err();
    assert!(f.error.reason.is_none(), "{:?}", f.error.reason);
    let fine =
        ev(r#"{"type":"error","message":"The server had an error","code":"x","param":null}"#);
    let mock = start_scripted(vec![Resp::sse(&fine)]);
    let (r, _) = run(&mock, ProviderKind::OpenAi, HttpConfig::default(), false);
    assert!(r.unwrap_err().error.reason.is_some());
    // The same check applies to a failed non-stream body.
    let body = r#"{"status":"failed","error":{"message":"bad: this is a request of some size"},"output":[]}"#;
    let mock = start_scripted(vec![Resp::json(200, body)]);
    let (r, _) = run_opts(
        &mock,
        ProviderKind::OpenAi,
        HttpConfig::default(),
        false,
        (false, false),
        &plain(),
        None,
    );
    assert!(r.unwrap_err().error.reason.is_none());
}

#[test]
fn l5_calls_made_when_lookups_are_off_are_ignored_not_validated() {
    // A function call with an unusable id, and nothing else: with lookups off this is an empty
    // reply (not a crash, not an empty success).
    let bad_call = [
        r#"{"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","call_id":"bad id!","name":"x","arguments":"{}"}}"#,
        r#"{"type":"response.output_item.done","output_index":0,"item":{"type":"function_call","call_id":"bad id!","name":"x","arguments":"{}"}}"#,
        r#"{"type":"response.completed","response":{"usage":{"input_tokens":4,"output_tokens":2}}}"#,
    ]
    .iter()
    .map(|e| ev(e))
    .collect::<String>();
    let mock = start_scripted(vec![Resp::sse(&bad_call)]);
    let (r, _) = run(&mock, ProviderKind::OpenAi, HttpConfig::default(), false);
    let f = r.unwrap_err();
    assert_eq!(f.error.kind, ErrorKind::InvalidResponse);
    assert!(f.received);
    // With text as well, the text is the reply and the call is dropped silently.
    let with_text = format!(
        "{}{}",
        ev(r#"{"type":"response.output_text.delta","delta":"hi"}"#),
        bad_call
    );
    let mock = start_scripted(vec![Resp::sse(&with_text)]);
    let (r, _) = run(&mock, ProviderKind::OpenAi, HttpConfig::default(), false);
    let r = r.unwrap();
    assert_eq!(r.text, "hi");
    assert!(r.tool_calls.is_empty());
    // Non-stream: a body of only calls is empty when lookups are off, and keeps them when on.
    let body = r#"{"status":"completed","output":[{"type":"function_call","call_id":"call_1","name":"suggest_break","arguments":"{}"}]}"#;
    let mock = start_scripted(vec![Resp::json(200, body)]);
    let (r, _) = run_opts(
        &mock,
        ProviderKind::OpenAi,
        HttpConfig::default(),
        false,
        (false, false),
        &plain(),
        None,
    );
    assert_eq!(r.unwrap_err().error.kind, ErrorKind::InvalidResponse);
    let mock = start_scripted(vec![Resp::json(200, body)]);
    let (r, _) = run_opts(
        &mock,
        ProviderKind::OpenAi,
        HttpConfig::default(),
        true,
        (false, false),
        &plain(),
        None,
    );
    assert_eq!(r.unwrap().tool_calls.len(), 1);
}

// ---- review round 2: reasoning refusal shapes, malformed-event booking -----------------------

#[test]
fn a_reasoning_refusal_is_recognised_by_code_or_type_and_message_too() {
    // gpt-4o style: no `param`, the message names reasoning.
    for body in [
        r#"{"error":{"message":"Unsupported parameter: 'reasoning.effort' is not supported with this model.","type":"invalid_request_error","param":null,"code":"unsupported_parameter"}}"#,
        r#"{"error":{"message":"This model does not support the reasoning option.","type":"invalid_request_error","param":null,"code":null}}"#,
        r#"{"error":{"message":"Unknown field 'reasoning'","type":"server_error","param":null,"code":"unsupported_parameter"}}"#,
    ] {
        let mock = start_scripted(vec![
            Resp::json(400, body),
            Resp::sse(&responses_text_stream()),
        ]);
        let (r, _) = run_opts(
            &mock,
            ProviderKind::OpenAi,
            HttpConfig::default(),
            false,
            (true, true),
            &plain(),
            None,
        );
        let r = r.unwrap_or_else(|f| panic!("{body}: {:?}", f.error));
        assert!(r.learned.reasoning_refused, "{body}");
        let reqs = mock.requests.lock().unwrap();
        assert_eq!(reqs.len(), 2, "{body}");
        assert!(body_of(&reqs[1]).get("reasoning").is_none());
    }
    // A 400 that merely says "invalid request" without naming reasoning is not retried.
    let mock = start_scripted(vec![Resp::json(
        400,
        r#"{"error":{"message":"Invalid value for input","type":"invalid_request_error","param":"input"}}"#,
    )]);
    let (r, _) = run_opts(
        &mock,
        ProviderKind::OpenAi,
        HttpConfig::default(),
        false,
        (true, true),
        &plain(),
        None,
    );
    assert!(r.is_err());
    assert_eq!(mock.requests.lock().unwrap().len(), 1);
}

#[test]
fn the_retry_still_happens_when_the_echo_check_drops_the_providers_text() {
    // The message quotes what we sent (so no text is shown), but its param still says reasoning.
    let body = r#"{"error":{"message":"Bad: hello there, this is a request of some size","type":"invalid_request_error","param":"reasoning.effort","code":"unsupported_parameter"}}"#;
    let mock = start_scripted(vec![
        Resp::json(400, body),
        Resp::sse(&responses_text_stream()),
    ]);
    let (r, _) = run_opts(
        &mock,
        ProviderKind::OpenAi,
        HttpConfig::default(),
        false,
        (true, true),
        &plain(),
        None,
    );
    assert!(r.unwrap().learned.reasoning_refused);
    // And a stream refusal likewise.
    let body = r#"{"error":{"message":"Bad: hello there, this is a request of some size","param":"stream"}}"#;
    let mock = start_scripted(vec![
        Resp::json(400, body),
        Resp::json(200, &responses_json()),
    ]);
    let (r, _) = run(&mock, ProviderKind::OpenAi, HttpConfig::default(), false);
    assert!(r.unwrap().learned.stream_refused);
}

#[test]
fn a_malformed_event_books_the_estimate_not_the_output_cap() {
    // Not valid JSON in an event: the provider reported no failure, so no usage is invented.
    let garbage = "event: x\ndata: {not json\n\n".to_owned();
    let mock = start_scripted(vec![Resp::sse(&garbage)]);
    let (r, _) = run(&mock, ProviderKind::OpenAi, HttpConfig::default(), false);
    let f = r.unwrap_err();
    assert_eq!(f.error.kind, ErrorKind::InvalidResponse);
    assert!(f.received);
    assert!(
        f.output_tokens < 64,
        "estimate, not the cap: {}",
        f.output_tokens
    );
    // Chat Completions: an event that is not JSON.
    let mock = start_scripted(vec![Resp::sse(&garbage)]);
    let (r, _) = run(
        &mock,
        ProviderKind::OpenAiCompatible,
        HttpConfig::default(),
        false,
    );
    assert!(r.unwrap_err().output_tokens < 64);
    // A non-streamed malformed 200 from a compatible server is not given the cap either.
    let mock = start_scripted(vec![Resp::json(200, "<html>oops</html>")]);
    let (r, _) = run(
        &mock,
        ProviderKind::OpenAiCompatible,
        HttpConfig::default(),
        false,
    );
    assert!(r.unwrap_err().output_tokens < 64);
}

// ---- thinking (reasoning summary) -----------------------------------------------------------

fn anthropic_thinking_stream() -> String {
    [
        r#"{"type":"message_start","message":{"usage":{"input_tokens":12,"output_tokens":1}}}"#,
        r#"{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}"#,
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"Weigh the "}}"#,
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"options.\u0007"}}"#,
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"SIGSECRET"}}"#,
        r#"{"type":"content_block_stop","index":0}"#,
        r#"{"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}"#,
        r#"{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"Do A."}}"#,
        r#"{"type":"content_block_stop","index":1}"#,
        r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":9}}"#,
        r#"{"type":"message_stop"}"#,
    ]
    .iter()
    .map(|e| ev(e))
    .collect()
}

type Thought = (
    Result<companion::StreamReply, companion::StreamFailure>,
    Vec<String>,
    Vec<String>,
);

fn run_thinking(
    mock: &Mock,
    kind: ProviderKind,
    rounds: &[companion::ToolExchange],
    tools: bool,
) -> Thought {
    let key = Secret::parse(KEY).unwrap();
    let m = msgs();
    let (mut said, mut thought) = (Vec::new(), Vec::new());
    let r = chat_stream(
        &HttpConfig::default(),
        &spec(kind, &mock.base),
        &key,
        &RequestPlan {
            messages: &m,
            rounds,
            tools,
            stream: true,
            reasoning: true,
            legacy_thinking: false,
            no_summary: false,
            max_output_tokens: 64,
            system: None,
        },
        &plain(),
        |d| said.push(d.to_owned()),
        |t| thought.push(t.to_owned()),
        |_| {},
    );
    (r, said, thought)
}

#[test]
fn anthropic_thinking_streams_apart_from_the_reply_and_adaptive_is_requested() {
    let mock = start_scripted(vec![Resp::sse(&anthropic_thinking_stream())]);
    let (r, said, thought) = run_thinking(&mock, ProviderKind::Anthropic, &[], false);
    let r = r.unwrap();
    assert_eq!(r.text, "Do A.");
    assert_eq!(said.concat(), "Do A.");
    assert_eq!(thought.concat(), "Weigh the options.");
    assert_eq!(r.thinking, "Weigh the options.");
    assert!(!r.text.contains("Weigh") && !r.thinking.contains("SIGSECRET"));
    let reqs = mock.requests.lock().unwrap();
    let b = body_of(&reqs[0]);
    assert_eq!(b["thinking"]["type"], "adaptive");
    assert_eq!(b["thinking"]["display"], "summarized");
    assert_eq!(b["output_config"]["effort"], "low");
    assert!(b.get("temperature").is_none() && b.get("top_p").is_none());
    assert_eq!(b["max_tokens"], 64 + 2048);
}

fn thinking_refused() -> Resp {
    Resp::json(
        400,
        r#"{"type":"error","error":{"type":"invalid_request_error","message":"\"thinking.type.adaptive\" is not supported for this model."}}"#,
    )
}

#[test]
fn anthropic_falls_back_from_adaptive_to_enabled_to_none() {
    // adaptive refused -> legacy enabled works.
    let mock = start_scripted(vec![
        thinking_refused(),
        Resp::sse(&anthropic_thinking_stream()),
    ]);
    let (r, _, thought) = run_thinking(&mock, ProviderKind::Anthropic, &[], false);
    let r = r.unwrap();
    assert!(r.learned.adaptive_refused && !r.learned.reasoning_refused);
    assert_eq!(thought.concat(), "Weigh the options.");
    {
        let reqs = mock.requests.lock().unwrap();
        assert_eq!(reqs.len(), 2);
        let b = body_of(&reqs[1]);
        assert_eq!(b["thinking"]["type"], "enabled");
        assert_eq!(b["thinking"]["budget_tokens"], 2048);
        assert!(b.get("output_config").is_none());
        assert!(b["max_tokens"].as_u64().unwrap() > 2048);
    }
    // both refused -> no thinking at all, three requests at most.
    let mock = start_scripted(vec![
        thinking_refused(),
        thinking_refused(),
        Resp::sse(&anthropic_thinking_stream()),
    ]);
    let (r, _, _) = run_thinking(&mock, ProviderKind::Anthropic, &[], false);
    let r = r.unwrap();
    assert!(r.learned.reasoning_refused);
    let reqs = mock.requests.lock().unwrap();
    assert_eq!(reqs.len(), 3);
    let last = body_of(&reqs[2]);
    assert!(last.get("thinking").is_none());
    assert_eq!(last["max_tokens"], 64);
    drop(reqs);
    // A plain 400 that does not mention thinking is not retried.
    let mock = start_scripted(vec![Resp::json(
        400,
        r#"{"type":"error","error":{"type":"invalid_request_error","message":"max_tokens: too large"}}"#,
    )]);
    let (r, _, _) = run_thinking(&mock, ProviderKind::Anthropic, &[], false);
    assert!(r.is_err());
    assert_eq!(mock.requests.lock().unwrap().len(), 1);
}

#[test]
fn anthropic_in_a_tool_round_sends_no_thinking_and_history_never_carries_it() {
    let call = ToolCall {
        id: "toolu_1".to_owned(),
        name: "suggest_break".to_owned(),
        arguments: "{}".to_owned(),
    };
    let rounds = [companion::ToolExchange {
        text: "Looking that up.".to_owned(),
        calls: vec![call],
        results: vec!["ok".to_owned()],
    }];
    let mock = start_scripted(vec![Resp::sse(&anthropic_thinking_stream())]);
    let (r, _, thought) = run_thinking(&mock, ProviderKind::Anthropic, &rounds, true);
    let r = r.unwrap();
    let reqs = mock.requests.lock().unwrap();
    let b = body_of(&reqs[0]);
    assert!(b.get("thinking").is_none());
    assert_eq!(b["max_tokens"], 64);
    // Thinking that arrives anyway is still shown, and still not part of the reply.
    assert_eq!(thought.concat(), "Weigh the options.");
    assert_eq!(r.text, "Do A.");
    drop(reqs);
    // The final no-tools request flattens the round to text; none of it carries thinking.
    let (_, _, _) = run_thinking(&mock, ProviderKind::Anthropic, &rounds, false);
    let reqs = mock.requests.lock().unwrap();
    assert!(!reqs[1].contains("Weigh the options"));
    assert!(!reqs[1].contains("SIGSECRET"));
    assert!(body_of(&reqs[1]).get("thinking").is_some());
}

#[test]
fn responses_reasoning_summary_is_requested_and_streamed_as_thinking() {
    let stream = [
        r#"{"type":"response.reasoning_summary_part.added","output_index":0,"summary_index":0,"part":{"type":"summary_text","text":""}}"#,
        r#"{"type":"response.reasoning_summary_text.delta","output_index":0,"summary_index":0,"delta":"Check the "}"#,
        r#"{"type":"response.reasoning_summary_text.delta","output_index":0,"summary_index":0,"delta":"facts."}"#,
        r#"{"type":"response.output_text.delta","output_index":1,"delta":"Answer."}"#,
        r#"{"type":"response.completed","response":{"usage":{"input_tokens":4,"output_tokens":6}}}"#,
    ]
    .iter()
    .map(|e| ev(e))
    .collect::<String>();
    let mock = start_scripted(vec![Resp::sse(&stream)]);
    let (r, said, thought) = run_thinking(&mock, ProviderKind::OpenAi, &[], false);
    let r = r.unwrap();
    assert_eq!(r.text, "Answer.");
    assert_eq!(said.concat(), "Answer.");
    assert_eq!(thought.concat(), "Check the facts.");
    assert_eq!(r.thinking, "Check the facts.");
    let reqs = mock.requests.lock().unwrap();
    let b = body_of(&reqs[0]);
    assert_eq!(b["reasoning"]["effort"], "low");
    assert_eq!(b["reasoning"]["summary"], "auto");
}

fn summary_refused() -> Resp {
    Resp::json(
        400,
        r#"{"error":{"message":"Your organization must be verified to generate reasoning summaries.","type":"invalid_request_error","param":"reasoning.summary","code":"unsupported_value"}}"#,
    )
}

#[test]
fn responses_summary_refused_retries_with_effort_only_then_drops_reasoning() {
    // Summary refused: the retry keeps `reasoning: {effort}` and drops only the summary.
    let mock = start_scripted(vec![summary_refused(), Resp::sse(&responses_text_stream())]);
    let (r, _, thought) = run_thinking(&mock, ProviderKind::OpenAi, &[], false);
    let r = r.unwrap();
    assert!(r.learned.summary_refused && !r.learned.reasoning_refused);
    assert!(thought.is_empty() && r.thinking.is_empty());
    {
        let reqs = mock.requests.lock().unwrap();
        assert_eq!(reqs.len(), 2);
        assert_eq!(body_of(&reqs[0])["reasoning"]["summary"], "auto");
        let second = body_of(&reqs[1])["reasoning"].clone();
        assert_eq!(second["effort"], "low");
        assert!(second.get("summary").is_none());
    }
    // A further refusal (of `reasoning` itself) drops it entirely: three requests at most.
    let mock = start_scripted(vec![
        summary_refused(),
        reasoning_refused(),
        Resp::sse(&responses_text_stream()),
    ]);
    let (r, _, _) = run_thinking(&mock, ProviderKind::OpenAi, &[], false);
    let r = r.unwrap();
    assert!(r.learned.summary_refused && r.learned.reasoning_refused);
    let reqs = mock.requests.lock().unwrap();
    assert_eq!(reqs.len(), 3);
    assert!(body_of(&reqs[2]).get("reasoning").is_none());
    drop(reqs);
    // A message naming the summary without a param is recognised too.
    let mock = start_scripted(vec![
        Resp::json(
            400,
            r#"{"error":{"message":"Unsupported value: 'reasoning.summary' is not available for this model.","type":"invalid_request_error","param":null,"code":"unsupported_value"}}"#,
        ),
        Resp::sse(&responses_text_stream()),
    ]);
    let (r, _, _) = run_thinking(&mock, ProviderKind::OpenAi, &[], false);
    assert!(r.unwrap().learned.summary_refused);
}

#[test]
fn anthropic_unrelated_400_mentioning_thinking_is_not_retried() {
    for msg in [
        "messages.0.content: I was thinking this request is too large for max_tokens.",
        "Your thinking time budget for this month is used up.",
    ] {
        let body = format!(
            r#"{{"type":"error","error":{{"type":"invalid_request_error","message":"{msg}"}}}}"#
        );
        let mock = start_scripted(vec![Resp::json(400, &body)]);
        let (r, _, _) = run_thinking(&mock, ProviderKind::Anthropic, &[], false);
        assert!(r.is_err(), "{msg}");
        assert_eq!(mock.requests.lock().unwrap().len(), 1, "{msg}");
    }
}

#[test]
fn a_dropped_stream_books_the_thinking_it_received() {
    // 4000 characters of thinking and no reply: the estimate counts them (about 1000 tokens).
    let big = "t".repeat(2000);
    let body = [
        r#"{"type":"message_start","message":{"usage":{"input_tokens":12,"output_tokens":1}}}"#.to_owned(),
        format!(
            r#"{{"type":"content_block_delta","index":0,"delta":{{"type":"thinking_delta","thinking":"{big}"}}}}"#
        ),
        format!(
            r#"{{"type":"content_block_delta","index":0,"delta":{{"type":"thinking_delta","thinking":"{big}"}}}}"#
        ),
    ]
    .iter()
    .map(|e| ev(e))
    .collect::<String>();
    let mock = start_scripted(vec![Resp::sse(&body)]);
    let (r, _, _) = run_thinking(&mock, ProviderKind::Anthropic, &[], false);
    let f = r.unwrap_err();
    assert!(f.received);
    assert!(f.output_tokens >= 1000, "{}", f.output_tokens);
}

#[test]
fn compatible_reasoning_content_is_thinking_and_capped() {
    let big = "x".repeat(4 * 1024);
    let one = |field: &str, v: &str| {
        ev(&format!(
            r#"{{"choices":[{{"delta":{{"{field}":"{v}"}}}}]}}"#
        ))
    };
    let body = one("reasoning_content", "plan ")
        + &(0..20).map(|_| one("reasoning", &big)).collect::<String>()
        + &ev(r#"{"choices":[{"delta":{"content":"Done."},"finish_reason":"stop"}]}"#)
        + "data: [DONE]\n\n";
    let mock = start_scripted(vec![Resp::sse(&body)]);
    let (r, _, thought) = run_thinking(&mock, ProviderKind::OpenAiCompatible, &[], false);
    let r = r.unwrap();
    assert_eq!(r.text, "Done.");
    assert!(thought.concat().starts_with("plan "));
    assert!(r.thinking_cut);
    assert_eq!(r.thinking.len(), companion::MAX_THINKING_BYTES);
    let reqs = mock.requests.lock().unwrap();
    assert!(body_of(&reqs[0]).get("thinking").is_none());
    assert!(body_of(&reqs[0]).get("reasoning").is_none());
}
