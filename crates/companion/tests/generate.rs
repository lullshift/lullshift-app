//! Generation end to end against a scripted mock provider on loopback (S13-T02, S13-T03). No real
//! network, no real key.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use companion::generate::{
    BreathingChoice, BreathingPattern, CallUsage, Difficulty, EngineKind, GenerateErrorKind,
    GenerationRequest, Hooks, generate, generate_with_http, generation_http_config,
    ignore_late_usage,
};
use companion::{CancelToken, CompanionError, ProviderKind, ProviderSpec, Secret};
use serde_json::{Value, json};

const KEY: &str = "sk-test-SUPERSECRETKEY-0123456789";

struct Resp {
    status: u16,
    body: String,
    delay_ms: u64,
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

/// Answers the n-th request with the n-th scripted response (the last one repeats).
fn start(script: Vec<Resp>) -> Mock {
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
            std::thread::sleep(Duration::from_millis(r.delay_ms));
            let _ = write!(
                s,
                "HTTP/1.1 {} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                r.status,
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

fn reply_body(kind: ProviderKind, text: &str, input: i64, output: i64) -> String {
    match kind {
        ProviderKind::Anthropic => json!({"content": [{"type": "text", "text": text}],
            "usage": {"input_tokens": input, "output_tokens": output}}),
        ProviderKind::OpenAi => json!({"id": "r", "status": "completed",
            "output": [{"type": "message", "role": "assistant",
                "content": [{"type": "output_text", "text": text}]}],
            "usage": {"input_tokens": input, "output_tokens": output}}),
        ProviderKind::OpenAiCompatible => json!({"choices": [{"message": {"content": text}}],
            "usage": {"prompt_tokens": input, "completion_tokens": output}}),
    }
    .to_string()
}

fn ok(kind: ProviderKind, text: &str, i: i64, o: i64) -> Resp {
    Resp {
        status: 200,
        body: reply_body(kind, text, i, o),
        delay_ms: 0,
    }
}

fn spec(kind: ProviderKind, base: &str) -> ProviderSpec {
    ProviderSpec {
        kind,
        base_url: base.to_owned(),
        model: "test-model".to_owned(),
    }
}

fn quiz_request() -> GenerationRequest {
    GenerationRequest {
        engine: EngineKind::Quiz,
        topic: "Sea creatures".into(),
        difficulty: Difficulty::Gentle,
        size: 5,
        avoid: vec!["Old question?".into()],
        breathing: None,
    }
}

fn good_quiz() -> String {
    json!({"title": "Sea", "questions": [
        {"prompt": "P1?", "choices": ["a", "b", "c"], "answer": 2, "explanation": "E1"}]})
    .to_string()
}

fn accept(_: &Value) -> Result<(), Vec<String>> {
    Ok(())
}

fn run(
    mock: &Mock,
    kind: ProviderKind,
    req: &GenerationRequest,
    cancel: &CancelToken,
    budget: &mut dyn FnMut(&[CallUsage]) -> Result<(), CompanionError>,
) -> Result<companion::generate::GenerateOutput, companion::generate::GenerateFailure> {
    let key = Secret::parse(KEY).unwrap();
    let mut hooks = Hooks {
        cancel,
        budget_check: budget,
        engine_check: &accept,
        late_usage: ignore_late_usage(),
    };
    generate(&spec(kind, &mock.base), &key, req, &mut hooks)
}

fn body_of(req: &str) -> Value {
    serde_json::from_str(req.split_once("\r\n\r\n").unwrap().1).unwrap()
}

#[test]
fn a_good_reply_comes_back_checked_with_its_usage_for_every_provider_kind() {
    for kind in [
        ProviderKind::Anthropic,
        ProviderKind::OpenAi,
        ProviderKind::OpenAiCompatible,
    ] {
        let mock = start(vec![ok(
            kind,
            &format!("```json\n{}\n```", good_quiz()),
            321,
            45,
        )]);
        let out = run(
            &mock,
            kind,
            &quiz_request(),
            &CancelToken::new(),
            &mut |_| Ok(()),
        )
        .unwrap();
        assert_eq!(out.data["shuffle"], json!(true), "{kind:?}");
        assert_eq!(out.model, "test-model");
        assert_eq!(out.usage.len(), 1);
        assert_eq!(
            (out.usage[0].input_tokens, out.usage[0].output_tokens),
            (321, 45)
        );
        let reqs = mock.requests.lock().unwrap();
        assert_eq!(reqs.len(), 1);
        let b = body_of(&reqs[0]);
        // The same body the consent dialog showed.
        let preview =
            companion::generate::preview_request_text(kind, "test-model", &quiz_request()).unwrap();
        assert_eq!(
            b,
            serde_json::from_str::<Value>(&preview).unwrap(),
            "{kind:?}"
        );
        let text = b.to_string();
        assert!(text.contains("one JSON object"), "host instruction sent");
        assert!(!text.contains("tools") && !text.contains("reasoning"));
        // The key is only in a header.
        assert!(!reqs[0].split_once("\r\n\r\n").unwrap().1.contains(KEY));
    }
}

#[test]
fn first_bad_then_good_sends_one_follow_up_without_the_reply_text() {
    let kind = ProviderKind::OpenAiCompatible;
    let mock = start(vec![
        ok(kind, "SECRET-REPLY-TEXT is not json", 100, 20),
        ok(kind, &good_quiz(), 130, 40),
    ]);
    let out = run(
        &mock,
        kind,
        &quiz_request(),
        &CancelToken::new(),
        &mut |_| Ok(()),
    )
    .unwrap();
    assert_eq!(out.usage.len(), 2);
    assert_eq!(out.usage[0].input_tokens + out.usage[1].input_tokens, 230);
    let reqs = mock.requests.lock().unwrap();
    assert_eq!(reqs.len(), 2);
    let second = body_of(&reqs[1]).to_string();
    assert!(second.contains("The reply was not valid JSON."));
    assert!(!second.contains("SECRET-REPLY-TEXT"));
}

#[test]
fn two_bad_replies_store_nothing_and_book_both_calls() {
    let kind = ProviderKind::OpenAiCompatible;
    let mock = start(vec![
        ok(kind, "nope", 10, 2),
        ok(kind, "{\"title\": \"T\"}", 11, 3),
    ]);
    let fail = run(
        &mock,
        kind,
        &quiz_request(),
        &CancelToken::new(),
        &mut |_| Ok(()),
    )
    .unwrap_err();
    assert_eq!(fail.error.kind, GenerateErrorKind::DidNotFit);
    assert_eq!(fail.usage.len(), 2);
    assert_eq!(mock.requests.lock().unwrap().len(), 2);
}

#[test]
fn a_budget_refusal_before_the_retry_sends_nothing_more() {
    let kind = ProviderKind::OpenAiCompatible;
    let mock = start(vec![ok(kind, "nope", 10, 2)]);
    let mut calls = 0;
    let fail = run(
        &mock,
        kind,
        &quiz_request(),
        &CancelToken::new(),
        &mut |_| {
            calls += 1;
            if calls > 1 {
                Err(CompanionError::budget_reached())
            } else {
                Ok(())
            }
        },
    )
    .unwrap_err();
    assert_eq!(fail.error.kind, GenerateErrorKind::BudgetRefused);
    assert_eq!(fail.usage.len(), 1);
    assert_eq!(mock.requests.lock().unwrap().len(), 1);
}

#[test]
fn an_invalid_request_is_refused_before_anything_is_sent() {
    let kind = ProviderKind::OpenAiCompatible;
    let mock = start(vec![ok(kind, &good_quiz(), 1, 1)]);
    let mut bad = quiz_request();
    bad.size = 99;
    let fail = run(&mock, kind, &bad, &CancelToken::new(), &mut |_| Ok(())).unwrap_err();
    assert_eq!(fail.error.kind, GenerateErrorKind::InvalidRequest);
    assert!(fail.usage.is_empty());
    assert!(mock.requests.lock().unwrap().is_empty());
}

#[test]
fn provider_errors_keep_the_reason_and_are_not_retried() {
    let kind = ProviderKind::OpenAiCompatible;
    let mock = start(vec![Resp {
        status: 401,
        body: r#"{"error":{"message":"Incorrect API key provided."}}"#.into(),
        delay_ms: 0,
    }]);
    let fail = run(
        &mock,
        kind,
        &quiz_request(),
        &CancelToken::new(),
        &mut |_| Ok(()),
    )
    .unwrap_err();
    assert_eq!(fail.error.kind, GenerateErrorKind::KeyRejected);
    assert!(fail.error.reason.is_some());
    assert!(!fail.error.message.contains(KEY));
    assert_eq!(mock.requests.lock().unwrap().len(), 1);
}

#[test]
fn no_first_byte_in_time_is_a_timeout() {
    let kind = ProviderKind::OpenAiCompatible;
    let mock = start(vec![Resp {
        delay_ms: 2500,
        ..ok(kind, &good_quiz(), 1, 1)
    }]);
    let cfg = companion::HttpConfig {
        first_byte_timeout: Duration::from_millis(300),
        timeout: Duration::from_millis(300),
        ..generation_http_config()
    };
    let key = Secret::parse(KEY).unwrap();
    let cancel = CancelToken::new();
    let mut hooks = Hooks {
        cancel: &cancel,
        budget_check: &mut |_| Ok(()),
        engine_check: &accept,
        late_usage: ignore_late_usage(),
    };
    let t = Instant::now();
    let fail = generate_with_http(
        &cfg,
        &spec(kind, &mock.base),
        &key,
        &quiz_request(),
        &mut hooks,
    )
    .unwrap_err();
    assert_eq!(fail.error.kind, GenerateErrorKind::Timeout);
    assert!(t.elapsed() < Duration::from_secs(2));
}

#[test]
fn cancel_returns_at_once_and_the_late_usage_is_handed_to_the_caller() {
    let kind = ProviderKind::OpenAiCompatible;
    let mock = start(vec![Resp {
        delay_ms: 700,
        ..ok(kind, &good_quiz(), 55, 9)
    }]);
    let key = Secret::parse(KEY).unwrap();
    let cancel = CancelToken::new();
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    let late: Arc<dyn Fn(companion::LateUsage) + Send + Sync> = Arc::new(move |u| {
        let _ = tx.lock().unwrap().send(u);
    });
    let c2 = cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(150));
        c2.cancel();
    });
    let mut hooks = Hooks {
        cancel: &cancel,
        budget_check: &mut |_| Ok(()),
        engine_check: &accept,
        late_usage: late,
    };
    let t = Instant::now();
    let fail = generate(&spec(kind, &mock.base), &key, &quiz_request(), &mut hooks).unwrap_err();
    assert_eq!(fail.error.kind, GenerateErrorKind::Cancelled);
    assert!(t.elapsed() < Duration::from_millis(600), "returned at once");
    let u = rx.recv_timeout(Duration::from_secs(5)).expect("late usage");
    assert!(u.input_tokens > 0);
    assert_eq!(
        mock.requests.lock().unwrap().len(),
        1,
        "no retry after a cancel"
    );
}

#[test]
fn offline_is_reported_as_offline() {
    // A port nothing listens on.
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://127.0.0.1:{}", l.local_addr().unwrap().port());
    drop(l);
    let key = Secret::parse(KEY).unwrap();
    let cancel = CancelToken::new();
    let mut hooks = Hooks {
        cancel: &cancel,
        budget_check: &mut |_| Ok(()),
        engine_check: &accept,
        late_usage: ignore_late_usage(),
    };
    let fail = generate(
        &spec(ProviderKind::OpenAiCompatible, &base),
        &key,
        &quiz_request(),
        &mut hooks,
    )
    .unwrap_err();
    assert_eq!(fail.error.kind, GenerateErrorKind::Offline);
}

#[test]
fn a_breathing_set_is_filled_from_the_users_pattern() {
    let kind = ProviderKind::OpenAiCompatible;
    let reply = json!({"title": "Settle", "phases": [{"cue": "In"}, {"cue": "Hold"},
        {"cue": "Out"}, {"cue": "Hold"}], "closing": "Well done."})
    .to_string();
    let mock = start(vec![ok(kind, &reply, 5, 5)]);
    let req = GenerationRequest {
        engine: EngineKind::Breathing,
        topic: "Settling".into(),
        difficulty: Difficulty::Tricky,
        size: 1,
        avoid: vec![],
        breathing: Some(BreathingChoice {
            pattern: BreathingPattern::BoxGentle,
            rounds: 8,
        }),
    };
    let out = run(&mock, kind, &req, &CancelToken::new(), &mut |_| Ok(())).unwrap();
    assert_eq!(out.data["rounds"], json!(8));
    assert_eq!(
        out.data["phases"][1],
        json!({"kind": "hold", "seconds": 2, "cue": "Hold"})
    );
    let b = body_of(&mock.requests.lock().unwrap()[0]).to_string();
    assert!(
        b.contains("Difficulty: gentle"),
        "body sets are always gentle"
    );
    assert!(b.contains("inhale, hold, exhale, hold"));
}
