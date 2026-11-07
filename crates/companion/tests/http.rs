//! Provider calls against a local mock server (S09-T04, A01, A03).

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use companion::{
    CancelToken, ChatMessage, ErrorKind, HttpConfig, ProviderKind, ProviderSpec, Role, Secret, chat,
};

const KEY: &str = "sk-test-SUPERSECRETKEY-0123456789";

#[derive(Clone, Copy)]
enum Scenario {
    Ok,
    Status(u16),
    RateLimited,
    Malformed,
    Slow,
    Oversized,
    Redirect,
    EchoAuth,
    Body(u16, &'static str),
    BigErrorBody,
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

fn respond(s: &mut TcpStream, status: &str, extra: &str, body: &[u8]) {
    let head = format!(
        "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n{extra}\r\n",
        body.len()
    );
    let _ = s.write_all(head.as_bytes());
    let _ = s.write_all(body);
    let _ = s.flush();
}

fn start(scn: Scenario, kind: ProviderKind) -> Mock {
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
                log.lock().unwrap().push(req.clone());
                match scn {
                    Scenario::Ok => {
                        let body = match kind {
                            ProviderKind::Anthropic => {
                                r#"{"content":[{"type":"text","text":"hello there"}],"usage":{"input_tokens":11,"output_tokens":4}}"#
                            }
                            _ => {
                                r#"{"choices":[{"message":{"content":"hello there"}}],"usage":{"prompt_tokens":11,"completion_tokens":4,"total_tokens":15}}"#
                            }
                        };
                        respond(&mut s, "200 OK", "", body.as_bytes());
                    }
                    Scenario::Status(c) => respond(
                        &mut s,
                        &format!("{c} Whatever"),
                        "",
                        br#"{"error":{"message":"PROVIDER-BODY-TEXT"}}"#,
                    ),
                    Scenario::Body(c, b) => {
                        respond(&mut s, &format!("{c} Whatever"), "", b.as_bytes())
                    }
                    Scenario::BigErrorBody => {
                        let mut b = br#"{"error":{"message":"HIDDEN-BIG-REASON"}}"#.to_vec();
                        b.resize(64 * 1024, b' ');
                        respond(&mut s, "400 Bad Request", "", &b);
                    }
                    Scenario::RateLimited => respond(
                        &mut s,
                        "429 Too Many Requests",
                        "retry-after: 17\r\n",
                        b"{}",
                    ),
                    Scenario::Malformed => respond(&mut s, "200 OK", "", b"<html>nope</html>"),
                    Scenario::Slow => {
                        std::thread::sleep(Duration::from_secs(3));
                        respond(&mut s, "200 OK", "", b"{}");
                    }
                    Scenario::Oversized => {
                        let big = vec![b'a'; 3 * 1024 * 1024];
                        respond(&mut s, "200 OK", "", &big);
                    }
                    Scenario::Redirect => respond(
                        &mut s,
                        "302 Found",
                        "location: http://127.0.0.1:1/elsewhere\r\n",
                        b"",
                    ),
                    Scenario::EchoAuth => {
                        // A hostile or buggy server echoing the auth header back in an error body.
                        let echoed: Vec<&str> = req
                            .lines()
                            .filter(|l| {
                                let l = l.to_ascii_lowercase();
                                l.starts_with("authorization:") || l.starts_with("x-api-key:")
                            })
                            .collect();
                        let body =
                            serde_json::json!({ "error": { "message": echoed.join(" | ") } });
                        respond(&mut s, "400 Bad Request", "", body.to_string().as_bytes());
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
        model: "test-model".to_owned(),
    }
}

fn msgs() -> Vec<ChatMessage> {
    vec![ChatMessage {
        role: Role::User,
        content: "hello".to_owned(),
    }]
}

fn run(
    scn: Scenario,
    kind: ProviderKind,
    cfg: HttpConfig,
) -> (Result<companion::Reply, companion::CompanionError>, Mock) {
    let mock = start(scn, kind);
    let key = Secret::parse(KEY).unwrap();
    let r = chat(
        &cfg,
        &spec(kind, &mock.base),
        &key,
        &msgs(),
        64,
        &CancelToken::new(),
    );
    (r, mock)
}

const KINDS: [ProviderKind; 3] = [
    ProviderKind::Anthropic,
    ProviderKind::OpenAi,
    ProviderKind::OpenAiCompatible,
];

#[test]
fn ok_reply_with_usage_for_every_provider() {
    for k in KINDS {
        let (r, _) = run(Scenario::Ok, k, HttpConfig::default());
        let r = r.unwrap_or_else(|e| panic!("{k:?}: {e:?}"));
        assert_eq!(
            (r.text.as_str(), r.input_tokens, r.output_tokens),
            ("hello there", 11, 4)
        );
    }
}

#[test]
fn request_has_no_tools_and_key_only_in_the_auth_header() {
    for k in KINDS {
        let (r, mock) = run(Scenario::Ok, k, HttpConfig::default());
        r.unwrap();
        let reqs = mock.requests.lock().unwrap();
        assert_eq!(reqs.len(), 1);
        let req = &reqs[0];
        let (head, body) = req.split_once("\r\n\r\n").unwrap();
        // Body: valid JSON, no tool-ish keys at any depth, no key.
        let v: serde_json::Value = serde_json::from_str(body).unwrap();
        fn has_key(v: &serde_json::Value, names: &[&str]) -> bool {
            match v {
                serde_json::Value::Object(m) => {
                    m.keys().any(|k| names.contains(&k.as_str()))
                        || m.values().any(|c| has_key(c, names))
                }
                serde_json::Value::Array(a) => a.iter().any(|c| has_key(c, names)),
                _ => false,
            }
        }
        assert!(!has_key(
            &v,
            &["tools", "functions", "tool_choice", "function_call"]
        ));
        assert!(!body.contains("SUPERSECRETKEY"));
        // Headers: the key appears in exactly the provider's auth header.
        let with_key: Vec<String> = head
            .lines()
            .filter(|l| l.contains("SUPERSECRETKEY"))
            .map(|l| l.split(':').next().unwrap().to_ascii_lowercase())
            .collect();
        let want = if k == ProviderKind::Anthropic {
            "x-api-key"
        } else {
            "authorization"
        };
        assert_eq!(with_key, vec![want.to_owned()], "{k:?}");
        let first = head.lines().next().unwrap();
        assert!(!first.contains("SUPERSECRETKEY"), "never in the URL");
        if k == ProviderKind::Anthropic {
            assert!(
                head.to_ascii_lowercase()
                    .contains("anthropic-version: 2023-06-01")
            );
        }
    }
}

#[test]
fn auth_failures_map_to_key_rejected() {
    for code in [401, 403] {
        let (r, _) = run(
            Scenario::Status(code),
            ProviderKind::OpenAi,
            HttpConfig::default(),
        );
        assert_eq!(r.unwrap_err().kind, ErrorKind::KeyRejected);
    }
}

#[test]
fn rate_limit_carries_retry_after() {
    let (r, _) = run(
        Scenario::RateLimited,
        ProviderKind::Anthropic,
        HttpConfig::default(),
    );
    let e = r.unwrap_err();
    assert_eq!(e.kind, ErrorKind::RateLimited);
    assert!(e.message.contains("17 seconds"), "{}", e.message);
}

#[test]
fn server_errors_and_odd_statuses_are_provider_errors_with_the_sanitised_reason() {
    for code in [500, 503, 404, 418] {
        let (r, _) = run(
            Scenario::Status(code),
            ProviderKind::OpenAiCompatible,
            HttpConfig::default(),
        );
        let e = r.unwrap_err();
        assert_eq!(e.kind, ErrorKind::ProviderError, "{code}");
        assert!(
            e.message
                .ends_with("The provider said: \"PROVIDER-BODY-TEXT\""),
            "{}",
            e.message
        );
    }
}

#[test]
fn the_reason_is_shown_per_provider_for_openai_and_anthropic_shapes() {
    let (r, _) = run(
        Scenario::Body(
            400,
            r#"{"error":{"message":"Unsupported parameter: 'max_completion_tokens' is not supported with this model.","type":"invalid_request_error","param":"max_completion_tokens","code":"unsupported_parameter"}}"#,
        ),
        ProviderKind::OpenAi,
        HttpConfig::default(),
    );
    let e = r.unwrap_err();
    assert_eq!(e.kind, ErrorKind::ProviderError);
    assert!(
        e.message
            .starts_with("The provider didn't accept the request (HTTP 400).")
    );
    assert!(
        e.message
            .contains("OpenAI said: \"Unsupported parameter: 'max_completion_tokens'"),
        "{}",
        e.message
    );
    let (r, _) = run(
        Scenario::Body(
            400,
            r#"{"type":"error","error":{"type":"invalid_request_error","message":"model: String should have at least 1 character"}}"#,
        ),
        ProviderKind::Anthropic,
        HttpConfig::default(),
    );
    assert!(
        r.unwrap_err()
            .message
            .contains("Anthropic said: \"model: String should have at least 1 character\"")
    );
}

#[test]
fn rejected_keys_and_rate_limits_may_carry_the_reason_too() {
    let (r, _) = run(
        Scenario::Body(
            403,
            r#"{"error":{"message":"Country, region, or territory not supported","code":"unsupported_country_region_territory"}}"#,
        ),
        ProviderKind::OpenAi,
        HttpConfig::default(),
    );
    let e = r.unwrap_err();
    assert_eq!(e.kind, ErrorKind::KeyRejected);
    assert!(
        e.message
            .contains("OpenAI said: \"Country, region, or territory not supported\"")
    );
    let (r, _) = run(
        Scenario::Body(
            401,
            r#"{"error":{"message":"Incorrect API key provided: sk-proj-****wxyz."}}"#,
        ),
        ProviderKind::OpenAi,
        HttpConfig::default(),
    );
    let e = r.unwrap_err();
    assert_eq!(e.kind, ErrorKind::KeyRejected);
    assert!(!e.message.contains("sk-proj"), "{}", e.message);
}

#[test]
fn non_json_and_oversized_error_bodies_give_only_the_fixed_message() {
    for body in ["<html><h1>502 Bad Gateway</h1></html>", "plain words", "[]"] {
        let (r, _) = run(
            Scenario::Body(500, body),
            ProviderKind::OpenAi,
            HttpConfig::default(),
        );
        let e = r.unwrap_err();
        assert_eq!(
            e.message,
            "The provider had a problem (HTTP 500). Try again later."
        );
    }
    let (r, _) = run(
        Scenario::BigErrorBody,
        ProviderKind::OpenAi,
        HttpConfig::default(),
    );
    let e = r.unwrap_err();
    assert_eq!(e.kind, ErrorKind::ProviderError);
    assert!(!e.message.contains("HIDDEN-BIG-REASON"), "{}", e.message);
    assert!(!e.message.contains(" said:"));
}

#[test]
fn malformed_body_is_invalid_response() {
    let (r, _) = run(
        Scenario::Malformed,
        ProviderKind::OpenAi,
        HttpConfig::default(),
    );
    assert_eq!(r.unwrap_err().kind, ErrorKind::InvalidResponse);
}

#[test]
fn slow_response_times_out() {
    let cfg = HttpConfig {
        timeout: Duration::from_millis(400),
        ..HttpConfig::default()
    };
    let t = Instant::now();
    let (r, _) = run(Scenario::Slow, ProviderKind::OpenAi, cfg);
    assert_eq!(r.unwrap_err().kind, ErrorKind::Timeout);
    assert!(t.elapsed() < Duration::from_secs(3));
}

#[test]
fn oversized_body_is_refused() {
    let cfg = HttpConfig {
        max_response_bytes: 64 * 1024,
        ..HttpConfig::default()
    };
    let (r, _) = run(Scenario::Oversized, ProviderKind::OpenAi, cfg);
    assert_eq!(r.unwrap_err().kind, ErrorKind::InvalidResponse);
}

#[test]
fn redirects_are_not_followed() {
    let (r, mock) = run(
        Scenario::Redirect,
        ProviderKind::Anthropic,
        HttpConfig::default(),
    );
    assert_eq!(r.unwrap_err().kind, ErrorKind::ProviderError);
    assert_eq!(mock.requests.lock().unwrap().len(), 1);
}

#[test]
fn echoed_auth_header_never_reaches_the_error() {
    for k in KINDS {
        let (r, mock) = run(Scenario::EchoAuth, k, HttpConfig::default());
        let e = r.unwrap_err();
        assert_eq!(e.kind, ErrorKind::ProviderError);
        assert!(!format!("{e:?}{e}").contains("SUPERSECRETKEY"));
        assert!(e.message.contains("[hidden]"), "{}", e.message);
        assert!(e.message.chars().count() <= 500);
        // The mock really did receive the key, so the test is meaningful.
        assert!(mock.requests.lock().unwrap()[0].contains("SUPERSECRETKEY"));
    }
}

#[test]
fn nothing_listening_is_offline() {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    drop(l);
    let key = Secret::parse(KEY).unwrap();
    let e = chat(
        &HttpConfig::default(),
        &spec(ProviderKind::OpenAi, &format!("http://127.0.0.1:{port}")),
        &key,
        &msgs(),
        64,
        &CancelToken::new(),
    )
    .unwrap_err();
    assert_eq!(e.kind, ErrorKind::Offline);
}

#[test]
fn plain_http_to_a_non_loopback_host_is_refused_before_any_connection() {
    let key = Secret::parse(KEY).unwrap();
    let e = chat(
        &HttpConfig::default(),
        &spec(ProviderKind::OpenAiCompatible, "http://example.com/v1"),
        &key,
        &msgs(),
        64,
        &CancelToken::new(),
    )
    .unwrap_err();
    assert_eq!(e.kind, ErrorKind::InvalidInput);
}

#[test]
fn cancel_returns_at_once_while_the_provider_is_slow() {
    let mock = start(Scenario::Slow, ProviderKind::OpenAi);
    let key = Secret::parse(KEY).unwrap();
    let token = CancelToken::new();
    let t2 = token.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(200));
        t2.cancel();
    });
    let t = Instant::now();
    let e = chat(
        &HttpConfig::default(),
        &spec(ProviderKind::OpenAi, &mock.base),
        &key,
        &msgs(),
        64,
        &token,
    )
    .unwrap_err();
    assert_eq!(e.kind, ErrorKind::Cancelled);
    assert!(t.elapsed() < Duration::from_secs(2));
}

#[test]
fn nothing_in_the_crate_logs() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap();
        // Test modules inside the files may print; look only at production code.
        let prod = text.split("#[cfg(test)]").next().unwrap();
        for banned in [
            "println!",
            "eprintln!",
            "dbg!",
            "log::",
            "tracing::",
            "print!(",
        ] {
            assert!(!prod.contains(banned), "{} uses {banned}", path.display());
        }
    }
}

/// Sets a flag when dropped, to see when the worker lets go of what it holds.
struct DropFlag(Arc<std::sync::atomic::AtomicBool>);
impl Drop for DropFlag {
    fn drop(&mut self) {
        self.0.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

#[test]
fn cancelled_worker_keeps_its_hold_until_it_finishes_and_books_an_estimate() {
    use std::sync::atomic::Ordering;
    let mock = start(Scenario::Slow, ProviderKind::OpenAi);
    let key = Secret::parse(KEY).unwrap();
    let token = CancelToken::new();
    let t2 = token.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(200));
        t2.cancel();
    });
    let released = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let hold = DropFlag(Arc::clone(&released));
    let booked: Arc<Mutex<Option<companion::LateUsage>>> = Arc::new(Mutex::new(None));
    let booked2 = Arc::clone(&booked);
    let e = companion::chat_tracked(
        &HttpConfig::default(),
        &spec(ProviderKind::OpenAi, &mock.base),
        &key,
        &msgs(),
        64,
        &token,
        hold,
        move |u| *booked2.lock().unwrap() = Some(u),
    )
    .unwrap_err();
    assert_eq!(e.kind, ErrorKind::Cancelled);
    // The caller has its answer, but the worker is still waiting on the slow provider.
    assert!(!released.load(Ordering::SeqCst), "hold released too early");
    assert!(booked.lock().unwrap().is_none());
    let start_wait = Instant::now();
    while !released.load(Ordering::SeqCst) {
        assert!(start_wait.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(20));
    }
    // The slow mock answers `{}`, which is not a reply, so the usage is an estimate.
    let u = booked.lock().unwrap().expect("usage booked before release");
    assert!(!u.reported);
    assert!(u.input_tokens > 0);
}
