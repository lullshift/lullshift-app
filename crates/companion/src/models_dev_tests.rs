//! Tests of the models.dev model list: lenient parsing of untrusted input, and the fetch and
//! cache against a local loopback server.

use std::io::Write as _;
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use super::*;

const SAMPLE: &str = include_str!("../tests/fixtures/models_dev_sample.json");
const MALFORMED: &str = include_str!("../tests/fixtures/models_dev_malformed.json");

// ---- parsing -----------------------------------------------------------------------------

#[test]
fn sample_keeps_chat_models_with_slim_fields() {
    let c = parse_catalog(SAMPLE.as_bytes()).unwrap();
    let ids: Vec<&str> = c.providers.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, ["anthropic", "local-lab", "openai"]);
    let a = c.provider("anthropic").unwrap();
    assert_eq!(a.name, "Anthropic");
    let s = a
        .models
        .iter()
        .find(|m| m.id == "claude-sonnet-5-5")
        .unwrap();
    assert_eq!(
        s,
        &CatalogModel {
            id: "claude-sonnet-5-5".into(),
            name: "Claude Sonnet 5.5".into(),
            family: Some("claude-sonnet".into()),
            tool_call: true,
            reasoning: true,
            release_date: Some("2026-06-10".into()),
            context_tokens: Some(200_000),
            output_tokens: Some(64_000),
            cost_input: Some(3.0),
            cost_output: Some(15.0),
        }
    );
    let lab = &c.provider("local-lab").unwrap().models[0];
    assert_eq!(lab.id, "lab/free-model:latest");
    assert_eq!(lab.release_date.as_deref(), Some("2025-11"));
    assert_eq!((lab.cost_input, lab.cost_output), (None, None));
}

#[test]
fn non_chat_models_are_skipped() {
    let c = parse_catalog(SAMPLE.as_bytes()).unwrap();
    let ids: Vec<&str> = c
        .provider("openai")
        .unwrap()
        .models
        .iter()
        .map(|m| m.id.as_str())
        .collect();
    // An embedding model (type and output) and an image model (output) are dropped.
    assert_eq!(ids, ["gpt-4o-mini"]);
}

#[test]
fn malformed_entries_are_skipped_not_fatal() {
    let c = parse_catalog(MALFORMED.as_bytes()).unwrap();
    // Only "good" has a usable model: providers without models, with a bad id, or that are not
    // objects are dropped.
    let ids: Vec<&str> = c.providers.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, ["good"]);
    let p = &c.providers[0];
    // Bidi override, bell and newline removed; whitespace collapsed.
    assert_eq!(p.name, "Good provider with spaces");
    let mut model_ids: Vec<&str> = p.models.iter().map(|m| m.id.as_str()).collect();
    model_ids.sort_unstable();
    // Ids with spaces or ';' dropped, a non-object dropped, a duplicate id kept once, a
    // non-string type skipped, a "chat" type with text output kept.
    assert_eq!(
        model_ids,
        ["chat-type", "huge-price", "no-name", "ok-model"]
    );
    let ok = p.models.iter().find(|m| m.id == "ok-model").unwrap();
    assert_eq!(ok.name, "OK");
    assert_eq!(
        ok.family.as_deref(),
        Some("family"),
        "zero-width space removed"
    );
    assert!(
        !ok.tool_call && !ok.reasoning,
        "non-boolean flags read as false"
    );
    assert_eq!(ok.release_date, None, "free-text date dropped");
    assert_eq!(
        (ok.cost_input, ok.cost_output),
        (None, None),
        "negative and text prices dropped"
    );
    assert_eq!((ok.context_tokens, ok.output_tokens), (None, None));
    let huge = p.models.iter().find(|m| m.id == "huge-price").unwrap();
    assert_eq!((huge.cost_input, huge.cost_output), (None, Some(2.5)));
    let unnamed = p.models.iter().find(|m| m.id == "no-name").unwrap();
    assert_eq!(unnamed.name, "no-name", "falls back to the id");
}

#[test]
fn bad_costs_are_dropped() {
    assert_eq!(clean_cost(f64::NAN), None);
    assert_eq!(clean_cost(f64::INFINITY), None);
    assert_eq!(clean_cost(-0.5), None);
    assert_eq!(clean_cost(2.0e6), None);
    assert_eq!(clean_cost(0.0), Some(0.0));
    assert_eq!(clean_cost(0.15), Some(0.15));
}

#[test]
fn model_ids_follow_the_profile_rule() {
    for ok in ["gpt-4o", "a", "org/model:tag@v1+x_y.z", &"m".repeat(200)] {
        assert!(valid_model_id(ok), "{ok}");
    }
    for bad in [
        "",
        " gpt",
        "gpt 4",
        "gpt;rm",
        "modèle",
        "a\u{202e}b",
        &"m".repeat(201),
    ] {
        assert!(!valid_model_id(bad), "{bad}");
    }
}

#[test]
fn names_are_plain_one_line_text_with_a_cap() {
    assert_eq!(clean_text("  a\tb\r\nc ", 50).as_deref(), Some("a b c"));
    assert_eq!(clean_text("\u{200b}\u{0007}", 50), None);
    let long = "x".repeat(500);
    assert_eq!(
        clean_text(&long, MAX_NAME_CHARS).unwrap().chars().count(),
        MAX_NAME_CHARS
    );
}

#[test]
fn not_json_or_not_an_object_is_refused() {
    for body in [&b"<html>nope</html>"[..], b"[1,2]", b"", b"\"text\""] {
        let e = parse_catalog(body).unwrap_err();
        assert_eq!(e.kind, ErrorKind::InvalidResponse);
        assert!(!e.message.contains("nope"));
    }
}

#[test]
fn provider_and_model_counts_are_capped() {
    let mut root = serde_json::Map::new();
    for i in 0..(MAX_PROVIDERS + 50) {
        root.insert(
            format!("p{i:05}"),
            serde_json::json!({ "name": "P", "models": { "m": { "id": "m" } } }),
        );
    }
    let mut models = serde_json::Map::new();
    for i in 0..(MAX_MODELS_PER_PROVIDER + 50) {
        models.insert(format!("m{i:05}"), serde_json::json!({ "name": "M" }));
    }
    root.insert("a-big".into(), serde_json::json!({ "models": models }));
    let bytes = serde_json::to_vec(&Value::Object(root)).unwrap();
    let c = parse_catalog(&bytes).unwrap();
    assert_eq!(c.providers.len(), MAX_PROVIDERS);
    let big = c.provider("a-big").expect("first in key order, so kept");
    assert_eq!(big.models.len(), MAX_MODELS_PER_PROVIDER);
}

// ---- fetching and the cache --------------------------------------------------------------

#[derive(Clone, Copy)]
enum Sc {
    /// 200 with the sample and ETag "v1"; 304 when If-None-Match is "v1".
    Sample,
    /// 200 with the sample and no ETag.
    SampleNoEtag,
    /// Like `Sample` but waits first.
    SlowSample(u64),
    Status(u16),
    NotJson,
    /// A body larger than the cap, with a content-length header.
    OversizeDeclared,
    /// A body larger than the cap, without content-length (read until close).
    OversizeStreamed,
    Redirect,
    /// 304 even without If-None-Match.
    Always304,
}

struct Mock {
    url: String,
    hits: Arc<AtomicUsize>,
    requests: Arc<Mutex<Vec<String>>>,
}

fn read_head(s: &mut TcpStream) -> Option<String> {
    s.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let n = s.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            return Some(String::from_utf8_lossy(&buf).into_owned());
        }
    }
}

fn reply(s: &mut TcpStream, status: &str, extra: &str, body: &[u8], with_len: bool) {
    let len = if with_len {
        format!("content-length: {}\r\n", body.len())
    } else {
        String::new()
    };
    let head = format!(
        "HTTP/1.1 {status}\r\ncontent-type: application/json\r\n{len}connection: close\r\n{extra}\r\n"
    );
    let _ = s.write_all(head.as_bytes());
    let _ = s.write_all(body);
    let _ = s.flush();
}

fn start(sc: Sc) -> Mock {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let hits = Arc::new(AtomicUsize::new(0));
    let requests = Arc::new(Mutex::new(Vec::new()));
    let (h, r) = (Arc::clone(&hits), Arc::clone(&requests));
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(mut s) = conn else { continue };
            let (h, r) = (Arc::clone(&h), Arc::clone(&r));
            std::thread::spawn(move || {
                let Some(req) = read_head(&mut s) else {
                    return;
                };
                h.fetch_add(1, Ordering::SeqCst);
                r.lock().unwrap().push(req.clone());
                let has_v1 = req.to_ascii_lowercase().contains("if-none-match: \"v1\"");
                match sc {
                    Sc::Sample | Sc::SlowSample(_) => {
                        if let Sc::SlowSample(ms) = sc {
                            std::thread::sleep(Duration::from_millis(ms));
                        }
                        if has_v1 {
                            reply(&mut s, "304 Not Modified", "etag: \"v1\"\r\n", b"", false);
                        } else {
                            reply(
                                &mut s,
                                "200 OK",
                                "etag: \"v1\"\r\n",
                                SAMPLE.as_bytes(),
                                true,
                            );
                        }
                    }
                    Sc::SampleNoEtag => reply(&mut s, "200 OK", "", SAMPLE.as_bytes(), true),
                    Sc::Status(c) => reply(
                        &mut s,
                        &format!("{c} Whatever"),
                        "",
                        b"{\"error\":\"BODY-TEXT\"}",
                        true,
                    ),
                    Sc::NotJson => reply(&mut s, "200 OK", "", b"<html>BODY-TEXT</html>", true),
                    Sc::OversizeDeclared => {
                        reply(&mut s, "200 OK", "", &vec![b' '; 70 * 1024], true);
                    }
                    Sc::OversizeStreamed => {
                        reply(&mut s, "200 OK", "", &vec![b' '; 70 * 1024], false);
                    }
                    Sc::Redirect => reply(
                        &mut s,
                        "302 Found",
                        "location: http://127.0.0.1:1/elsewhere\r\n",
                        b"",
                        true,
                    ),
                    Sc::Always304 => reply(&mut s, "304 Not Modified", "", b"", false),
                }
            });
        }
    });
    Mock {
        url: format!("http://127.0.0.1:{port}/api.json"),
        hits,
        requests,
    }
}

fn small() -> CatalogLimits {
    CatalogLimits {
        timeout: Duration::from_secs(10),
        max_bytes: 64 * 1024,
    }
}

fn cache(m: &Mock) -> CatalogCache {
    CatalogCache::with_url(&m.url, small())
}

#[test]
fn production_url_is_the_fixed_models_dev_address() {
    let c = CatalogCache::new();
    assert_eq!(c.url, "https://models.dev/api.json");
    assert_eq!(c.url, MODELS_DEV_URL);
    assert_eq!(c.limits.max_bytes, 16 * 1024 * 1024);
    assert!(c.cached().unwrap().is_none());
}

#[test]
fn nothing_is_requested_until_load_and_then_memory_answers() {
    let m = start(Sc::Sample);
    let c = cache(&m);
    assert!(c.cached().unwrap().is_none());
    assert_eq!(m.hits.load(Ordering::SeqCst), 0);
    let r = c.load(false).unwrap();
    assert!(!r.unchanged && !r.from_memory);
    assert_eq!(r.loaded.etag.as_deref(), Some("\"v1\""));
    assert_eq!(r.loaded.catalog.providers.len(), 3);
    assert_eq!(m.hits.load(Ordering::SeqCst), 1);
    // A second load without refresh does not go to the network.
    let again = c.load(false).unwrap();
    assert!(again.from_memory);
    assert!(c.cached().unwrap().is_some());
    assert_eq!(m.hits.load(Ordering::SeqCst), 1);
    let req = m.requests.lock().unwrap()[0].to_ascii_lowercase();
    assert!(req.starts_with("get /api.json "), "{req}");
    assert!(req.contains("user-agent: lullshift-companion"));
    assert!(!req.contains("if-none-match"), "first load sends no etag");
    assert!(!req.contains("authorization") && !req.contains("x-api-key"));
}

#[test]
fn refresh_sends_the_etag_and_a_304_keeps_the_list() {
    let m = start(Sc::Sample);
    let c = cache(&m);
    let first = c.load(false).unwrap();
    let r = c.load(true).unwrap();
    assert!(r.unchanged && !r.from_memory);
    assert!(Arc::ptr_eq(&first.loaded.catalog, &r.loaded.catalog));
    assert!(r.loaded.fetched_at >= first.loaded.fetched_at);
    assert_eq!(m.hits.load(Ordering::SeqCst), 2);
    let req = m.requests.lock().unwrap()[1].to_ascii_lowercase();
    assert!(req.contains("if-none-match: \"v1\""), "{req}");
}

#[test]
fn refresh_without_an_etag_fetches_the_whole_list() {
    let m = start(Sc::SampleNoEtag);
    let c = cache(&m);
    c.load(false).unwrap();
    let r = c.load(true).unwrap();
    assert!(!r.unchanged);
    assert!(
        !m.requests.lock().unwrap()[1]
            .to_ascii_lowercase()
            .contains("if-none-match")
    );
}

#[test]
fn a_304_without_an_etag_sent_is_refused() {
    let m = start(Sc::Always304);
    let e = cache(&m).load(false).unwrap_err();
    assert_eq!(e.kind, ErrorKind::InvalidResponse);
}

#[test]
fn http_errors_name_the_status_without_the_body() {
    for code in [404, 500, 503] {
        let m = start(Sc::Status(code));
        let e = cache(&m).load(false).unwrap_err();
        assert_eq!(e.kind, ErrorKind::ProviderError);
        assert!(e.message.contains(&code.to_string()));
        assert!(!e.message.contains("BODY-TEXT"));
    }
}

#[test]
fn non_json_is_refused_without_the_body() {
    let m = start(Sc::NotJson);
    let e = cache(&m).load(false).unwrap_err();
    assert_eq!(e.kind, ErrorKind::InvalidResponse);
    assert!(!e.message.contains("BODY-TEXT"));
}

#[test]
fn oversize_is_refused_with_or_without_content_length() {
    for sc in [Sc::OversizeDeclared, Sc::OversizeStreamed] {
        let m = start(sc);
        let e = cache(&m).load(false).unwrap_err();
        assert_eq!(e.kind, ErrorKind::InvalidResponse);
        assert!(e.message.contains("larger than expected"), "{}", e.message);
    }
}

#[test]
fn redirects_are_not_followed() {
    let m = start(Sc::Redirect);
    let e = cache(&m).load(false).unwrap_err();
    assert_eq!(e.kind, ErrorKind::ProviderError);
    assert!(e.message.contains("302"));
    assert_eq!(m.hits.load(Ordering::SeqCst), 1);
}

#[test]
fn slow_answer_times_out() {
    let m = start(Sc::SlowSample(3_000));
    let c = CatalogCache::with_url(
        &m.url,
        CatalogLimits {
            timeout: Duration::from_millis(400),
            max_bytes: 64 * 1024,
        },
    );
    let t = Instant::now();
    let e = c.load(false).unwrap_err();
    assert_eq!(e.kind, ErrorKind::Timeout);
    assert!(e.message.contains("models.dev"));
    assert!(t.elapsed() < Duration::from_secs(3));
}

#[test]
fn nothing_listening_is_offline() {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    drop(l);
    let c = CatalogCache::with_url(&format!("http://127.0.0.1:{port}/api.json"), small());
    let e = c.load(false).unwrap_err();
    assert_eq!(e.kind, ErrorKind::Offline);
    assert!(e.message.contains("models.dev"));
}

#[test]
fn a_failed_refresh_keeps_the_list_in_memory() {
    let m = start(Sc::Sample);
    let c = cache(&m);
    c.load(false).unwrap();
    // Point a second cache's state at a dead server by swapping the URL.
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    drop(l);
    let mut c = c;
    c.url = format!("http://127.0.0.1:{port}/api.json");
    assert_eq!(c.load(true).unwrap_err().kind, ErrorKind::Offline);
    assert_eq!(c.cached().unwrap().unwrap().catalog.providers.len(), 3);
}

#[test]
fn concurrent_loads_share_one_request() {
    let m = start(Sc::SlowSample(400));
    let c = Arc::new(cache(&m));
    let handles: Vec<_> = (0..6)
        .map(|i| {
            let c = Arc::clone(&c);
            // Mix plain loads and refreshes: they all join the one request in flight.
            std::thread::spawn(move || c.load(i % 2 == 0))
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    for r in &results {
        assert_eq!(r.as_ref().unwrap().loaded.catalog.providers.len(), 3);
    }
    assert_eq!(m.hits.load(Ordering::SeqCst), 1);
}

#[test]
fn concurrent_loads_share_one_failure_too() {
    let m = start(Sc::Status(503));
    let c = Arc::new(cache(&m));
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let c = Arc::clone(&c);
            std::thread::spawn(move || c.load(false))
        })
        .collect();
    for h in handles {
        assert_eq!(
            h.join().unwrap().unwrap_err().kind,
            ErrorKind::ProviderError
        );
    }
    // Each failure may be its own flight if the threads did not overlap, but never more
    // requests than callers, and the cache is free again afterwards.
    assert!(m.hits.load(Ordering::SeqCst) <= 4);
    assert!(!c.state.lock().unwrap().in_flight);
}
