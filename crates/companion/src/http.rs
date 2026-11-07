//! The HTTP call (ADR-020): rustls with certificate verification always on, no redirects, a
//! total timeout, a response size cap read incrementally, no retries, and cancellation.
//!
//! Errors are classified, never printed: the transport error text, response body and headers
//! are dropped here.

use std::io::Read;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use ureq::config::Config;
use ureq::tls::{RootCerts, TlsConfig};

use crate::error::CompanionError;
use zeroize::Zeroizing;

use crate::provider::{
    BuiltRequest, ChatMessage, ProviderSpec, Reply, build_request, estimate, map_status_reason,
    parse_reply_with,
};
use crate::secret::Secret;
use crate::url_check::check_base_url;

pub const TIMEOUT: Duration = Duration::from_secs(60);
pub const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
/// Streaming: the wait for the first byte of a reply (a local model may be reading a long prompt).
pub const STREAM_FIRST_BYTE_TIMEOUT: Duration = Duration::from_secs(60);
/// Streaming: the longest silence once a reply has started. Keep-alive pings reset it.
pub const STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
/// Streaming: hard cap on one provider call, whatever the stream does.
pub const STREAM_TOTAL_TIMEOUT: Duration = Duration::from_secs(300);
/// Streaming: most reply text kept (bytes of cleaned text).
pub const MAX_REPLY_BYTES: usize = 128 * 1024;
/// Streaming: largest single SSE line or event (bytes).
pub const MAX_EVENT_BYTES: usize = 64 * 1024;
/// Streaming: most raw bytes read from one stream.
pub const MAX_STREAM_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy)]
pub struct HttpConfig {
    pub timeout: Duration,
    pub max_response_bytes: usize,
    pub first_byte_timeout: Duration,
    pub idle_timeout: Duration,
    pub total_timeout: Duration,
    pub max_reply_bytes: usize,
    pub max_event_bytes: usize,
    pub max_stream_bytes: usize,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            timeout: TIMEOUT,
            max_response_bytes: MAX_RESPONSE_BYTES,
            first_byte_timeout: STREAM_FIRST_BYTE_TIMEOUT,
            idle_timeout: STREAM_IDLE_TIMEOUT,
            total_timeout: STREAM_TOTAL_TIMEOUT,
            max_reply_bytes: MAX_REPLY_BYTES,
            max_event_bytes: MAX_EVENT_BYTES,
            max_stream_bytes: MAX_STREAM_BYTES,
        }
    }
}

/// Set from another thread to stop a waiting call.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

struct Raw {
    status: u16,
    retry_after: Option<String>,
    body: Vec<u8>,
}

pub(crate) fn agent(cfg: &HttpConfig, loopback: bool) -> ureq::Agent {
    let mut b = Config::builder()
        .http_status_as_error(false)
        .max_redirects(0)
        .max_redirects_will_error(false)
        .timeout_global(Some(cfg.timeout))
        .https_only(!loopback)
        .user_agent("Lullshift-Companion")
        .tls_config(
            TlsConfig::builder()
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        );
    if loopback {
        // A key sent in plain http must never go through a proxy picked up from the environment.
        b = b.proxy(None);
    }
    b.build().new_agent()
}

/// A failed body read. ureq reports its own timeout through the reader as an `io::Error`
/// wrapping it, so convert back before classifying: a stalled body is a timeout, not "offline".
pub(crate) fn classify_io(e: std::io::Error) -> CompanionError {
    classify(&ureq::Error::from(e))
}

pub(crate) fn classify(e: &ureq::Error) -> CompanionError {
    match e {
        ureq::Error::Timeout(_) => CompanionError::timeout(),
        ureq::Error::Io(io)
            if matches!(
                io.kind(),
                std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
            ) =>
        {
            CompanionError::timeout()
        }
        _ => CompanionError::offline(),
    }
}

fn post(
    cfg: &HttpConfig,
    loopback: bool,
    url: &str,
    headers: Vec<(&'static str, Zeroizing<String>)>,
    body: &[u8],
) -> Result<Raw, CompanionError> {
    let agent = agent(cfg, loopback);
    let mut req = agent.post(url).content_type("application/json");
    for (k, v) in &headers {
        req = req.header(*k, v.as_str());
    }
    // The values now live in the request; zeroise our copies before waiting on the network.
    drop(headers);
    let mut resp = req.send(body).map_err(|e| classify(&e))?;
    let status = resp.status().as_u16();
    let retry_after = resp
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    if !(200..300).contains(&status) {
        // Only the standard JSON error shapes are used later; oversize, slow or unreadable
        // bodies give an empty one. The body is never logged.
        let body = crate::reason::read_capped(&mut resp.body_mut().as_reader(), || false)
            .unwrap_or_default();
        return Ok(Raw {
            status,
            retry_after,
            body,
        });
    }
    if resp
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok())
        .is_some_and(|n| n > cfg.max_response_bytes)
    {
        return Err(CompanionError::invalid_response());
    }
    // Read at most cap + 1 bytes: one extra byte proves the body was too large.
    let mut buf = Vec::new();
    let limit = u64::try_from(cfg.max_response_bytes)
        .unwrap_or(u64::MAX)
        .saturating_add(1);
    let mut reader = resp.body_mut().as_reader().take(limit);
    let mut chunk = [0u8; 16 * 1024];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(e) => return Err(classify_io(e)),
        }
    }
    if buf.len() > cfg.max_response_bytes {
        return Err(CompanionError::invalid_response());
    }
    Ok(Raw {
        status,
        retry_after,
        body: buf,
    })
}

/// Tokens to book for a call the user cancelled while the worker was still running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LateUsage {
    pub input_tokens: i64,
    pub output_tokens: i64,
    /// True when the provider reported these numbers; false when they were estimated from the
    /// size of the request.
    pub reported: bool,
}

pub(crate) const PENDING: u8 = 0;
pub(crate) const DELIVERED: u8 = 1;
pub(crate) const ABANDONED: u8 = 2;

/// Sends one non-streaming chat request and returns the reply. Waits on a worker thread so
/// `cancel` takes effect at once; a cancelled worker finishes (or times out) on its own.
pub fn chat(
    cfg: &HttpConfig,
    spec: &ProviderSpec,
    key: &Secret,
    messages: &[ChatMessage],
    max_output_tokens: u32,
    cancel: &CancelToken,
) -> Result<Reply, CompanionError> {
    chat_tracked(
        cfg,
        spec,
        key,
        messages,
        max_output_tokens,
        cancel,
        (),
        |_| {},
    )
}

/// Like [`chat`], for callers that must outlive a cancel.
///
/// - `hold` is moved into the worker and dropped only when the worker has really finished, so a
///   busy lock kept in it stays locked while the request is still on the wire.
/// - If the call is cancelled, `on_abandoned` runs on the worker thread, after the HTTP call
///   ended and before `hold` is dropped, with the real usage if a reply arrived or an estimate
///   from the request size otherwise. It does not run when the result is returned normally.
#[allow(clippy::too_many_arguments)]
pub fn chat_tracked<H, F>(
    cfg: &HttpConfig,
    spec: &ProviderSpec,
    key: &Secret,
    messages: &[ChatMessage],
    max_output_tokens: u32,
    cancel: &CancelToken,
    hold: H,
    on_abandoned: F,
) -> Result<Reply, CompanionError>
where
    H: Send + 'static,
    F: FnOnce(LateUsage) + Send + 'static,
{
    // Re-checked here as well as when the profile was saved.
    let checked = check_base_url(&spec.base_url)?;
    let spec = ProviderSpec {
        base_url: checked.url,
        ..spec.clone()
    };
    let built = build_request(&spec, key.expose(), messages, max_output_tokens);
    let scrub_key = Zeroizing::new(key.expose().to_owned());
    let sent_chars: usize = messages.iter().map(|m| m.content.chars().count()).sum();
    let echoes = std::sync::Arc::new(crate::reason::Echoes::new(
        messages.iter().map(|m| m.content.clone()),
    ));
    let max_output = max_output_tokens;
    let kind = spec.kind;
    let cfg = *cfg;
    let loopback = checked.loopback;
    let (tx, rx) = mpsc::channel();
    let handoff = Arc::new(AtomicU8::new(PENDING));
    let worker_handoff = Arc::clone(&handoff);
    std::thread::spawn(move || {
        let BuiltRequest { url, headers, body } = built;
        let raw = post(&cfg, loopback, &url, headers, &body);
        let result = raw
            .and_then(|raw| {
                if (200..300).contains(&raw.status) {
                    parse_reply_with(kind, &raw.body, sent_chars, max_output, &scrub_key, &echoes)
                } else {
                    let reason = crate::reason::parse(&raw.body, &scrub_key, &echoes);
                    Err(map_status_reason(
                        raw.status,
                        raw.retry_after.as_deref(),
                        kind,
                        reason.as_ref(),
                    ))
                }
            })
            .map_err(|e| e.scrubbed(&scrub_key));
        drop(scrub_key);
        if worker_handoff
            .compare_exchange(PENDING, DELIVERED, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            let _ = tx.send(result);
        } else {
            // The caller gave up. Book what this call cost, then release `hold`.
            let usage = match &result {
                Ok(r) => LateUsage {
                    input_tokens: r.input_tokens,
                    output_tokens: r.output_tokens,
                    reported: true,
                },
                Err(e) => match e.usage {
                    Some((i, o)) => LateUsage {
                        input_tokens: i,
                        output_tokens: o,
                        reported: false,
                    },
                    None => LateUsage {
                        input_tokens: estimate(sent_chars),
                        output_tokens: 0,
                        reported: false,
                    },
                },
            };
            on_abandoned(usage);
        }
        drop(hold);
    });
    // The worker is bounded by the HTTP timeout; the wait is bounded here too, so a body that
    // trickles in or stalls can never keep the caller past that time plus a short grace.
    let started = Instant::now();
    let bound = cfg.timeout + Duration::from_secs(2);
    loop {
        if (cancel.is_cancelled() || started.elapsed() > bound)
            && handoff
                .compare_exchange(PENDING, ABANDONED, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
        {
            return Err(if cancel.is_cancelled() {
                CompanionError::cancelled()
            } else {
                CompanionError::timeout()
            });
        }
        // If the cancel lost the race, the worker already delivered: use the result.
        match rx.recv_timeout(Duration::from_millis(40)) {
            Ok(r) => return r,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return Err(CompanionError::internal()),
        }
    }
}
