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
    let key = Secret::parse(KEY).unwrap();
    let m = msgs();
    let plan = RequestPlan {
        messages: &m,
        rounds: &[],
        tools,
        stream: true,
        reasoning: false,
        max_output_tokens: 64,
    };
    let mut deltas = Vec::new();
    let r = chat_stream(
        &cfg,
        &spec(kind, &mock.base),
        &key,
        &plan,
        cancel,
        |d| deltas.push(d.to_owned()),
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
    fn steps() -> Vec<Step> {
        vec![
            Step::Write("data: {\"choices\":[{\"delta\":{\"content\":\"early\"}}]}\n\n"),
            Step::Sleep(700),
            Step::Write("data: {\"choices\":[{\"delta\":{\"content\":\"LATE\"}}]}\n\n"),
            Step::Write(
                "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n",
            ),
        ]
    }
    let mock = start("text/event-stream", steps);
    let cancel = CancelToken::new();
    let c2 = cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(250));
        c2.cancel();
    });
    let (tx, rx) = mpsc::channel();
    let t = Instant::now();
    let (r, deltas) = run_with(
        &mock,
        ProviderKind::OpenAiCompatible,
        HttpConfig::default(),
        false,
        &cancel,
        Some(tx),
    );
    assert_eq!(r.unwrap_err().error.kind, ErrorKind::Cancelled);
    assert!(t.elapsed() < Duration::from_millis(600), "returned at once");
    assert_eq!(deltas.concat(), "early");
    let u = rx
        .recv_timeout(Duration::from_secs(10))
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
}

impl Resp {
    fn json(status: u16, body: &str) -> Self {
        Self {
            status,
            content_type: "application/json",
            body: body.to_owned(),
        }
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
            max_output_tokens: 64,
        },
        &cancel,
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
            max_output_tokens: 64,
        },
        &cancel,
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
