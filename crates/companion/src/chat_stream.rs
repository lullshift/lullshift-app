//! Streaming provider call (Sprint 12, S12-T15, ADR-020).
//!
//! One worker thread makes the HTTP call and reads the stream; the calling thread waits, hands
//! each cleaned chunk to `on_delta`, and enforces the silence limits. Cancelling (or a silence
//! timeout) abandons the worker: the caller returns at once and the worker, on its next read,
//! stops, drops the connection and reports the call's usage through `on_abandoned`. Nothing the
//! worker saw after that point is handed to the caller.
//!
//! Limits (see `HttpConfig`): first-byte wait, idle silence, total time, reply bytes, event size,
//! raw stream bytes. A server that answers 200 with plain JSON instead of a stream is accepted
//! as one chunk. Tool calls are only kept when the request offered tools.

use std::io::Read;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::error::CompanionError;
use crate::http::{
    ABANDONED, CancelToken, DELIVERED, HttpConfig, LateUsage, PENDING, agent, classify,
};
use crate::provider::{
    BuiltRequest, ProviderKind, ProviderSpec, RequestPlan, build_plan_request, clean_chunk,
    clean_reply, estimate, map_status_reason, parse_reply,
};
use crate::secret::Secret;
use crate::sse::SseParser;
use crate::stream::{Decoder, Piece, ended_early, parse_responses_body};
use crate::tools::ToolCall;
use crate::url_check::check_base_url;

/// Appended to a reply that had to be fetched without streaming (see `chat_stream`).
pub const NO_STREAM_NOTICE: &str = "\n\n(OpenAI would not stream this reply for your organisation, so it arrived all at once. Verifying your organisation with OpenAI may allow streaming.)";

/// A finished (or cut) streamed reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamReply {
    /// Cleaned text; may be empty when the model only asked for lookups.
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    /// True when the provider reported the numbers; false when they were estimated.
    pub reported: bool,
    /// The reply hit the byte limit and was cut.
    pub cut: bool,
}

/// A stream that failed. `input_tokens` and `output_tokens` are what to book for it: zero if no
/// stream bytes arrived, otherwise the provider's numbers or an estimate that includes the partial
/// text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamFailure {
    pub error: CompanionError,
    pub input_tokens: i64,
    pub output_tokens: i64,
    /// Stream bytes had arrived.
    pub received: bool,
    /// Text that had been streamed to the caller (cleaned), possibly empty.
    pub partial: String,
    /// The provider refused `stream: true` for this organisation (OpenAI, HTTP 400, `param` is
    /// `stream`). Set only on that refusal; `chat_stream` retries once without streaming.
    pub stream_refused: bool,
}

impl StreamFailure {
    fn bare(error: CompanionError) -> Self {
        Self {
            error,
            input_tokens: 0,
            output_tokens: 0,
            received: false,
            partial: String::new(),
            stream_refused: false,
        }
    }
}

enum Msg {
    Activity,
    Delta(String),
    End(Result<StreamReply, StreamFailure>),
}

/// What the error-body reader needs: the key to hide and what we sent (a reason quoting it is dropped).
struct ErrCtx {
    key: zeroize::Zeroizing<String>,
    echoes: Vec<String>,
}

struct Tally {
    input: Option<i64>,
    output: Option<i64>,
    output_final: bool,
}

fn est_chars(n: usize) -> i64 {
    estimate(n)
}

#[allow(clippy::too_many_arguments)]
fn stream_call(
    cfg: &HttpConfig,
    loopback: bool,
    kind: ProviderKind,
    tools_on: bool,
    sent_chars: usize,
    built: BuiltRequest,
    ctx: &ErrCtx,
    handoff: &AtomicU8,
    tx: &mpsc::Sender<Msg>,
) -> Result<StreamReply, StreamFailure> {
    let BuiltRequest { url, headers, body } = built;
    let mut call_cfg = *cfg;
    call_cfg.timeout = cfg.total_timeout;
    let agent = agent(&call_cfg, loopback);
    let mut req = agent.post(&url).content_type("application/json");
    for (k, v) in &headers {
        req = req.header(*k, v.as_str());
    }
    drop(headers);
    let mut resp = req
        .send(&body[..])
        .map_err(|e| StreamFailure::bare(classify(&e)))?;
    drop(body);
    let status = resp.status().as_u16();
    if !(200..300).contains(&status) {
        let retry = resp
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        // The provider's stated reason, from a capped, time-limited read of standard JSON error
        // shapes only. Never logged.
        let reason = crate::reason::read_capped(&mut resp.body_mut().as_reader(), || {
            handoff.load(Ordering::SeqCst) == ABANDONED
        })
        .and_then(|b| crate::reason::parse(&b, &ctx.key, &ctx.echoes));
        let mut failure = StreamFailure::bare(map_status_reason(
            status,
            retry.as_deref(),
            kind,
            reason.as_ref(),
        ));
        failure.stream_refused = status == 400
            && kind == ProviderKind::OpenAi
            && reason.as_ref().and_then(|r| r.param.as_deref()) == Some("stream");
        return Err(failure);
    }
    let is_stream = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.to_ascii_lowercase().starts_with("text/event-stream"));

    let mut text = String::new();
    let mut calls: Vec<ToolCall> = Vec::new();
    let mut tally = Tally {
        input: None,
        output: None,
        output_final: false,
    };
    let mut received = false;
    let mut cut = false;

    let failure =
        |error: CompanionError, text: &str, calls: &[ToolCall], tally: &Tally, received: bool| {
            let produced =
                text.chars().count() + calls.iter().map(|c| c.arguments.len()).sum::<usize>();
            let (input, output) = if received {
                (
                    tally.input.unwrap_or_else(|| est_chars(sent_chars)),
                    if produced == 0 {
                        tally.output.unwrap_or(0)
                    } else {
                        tally.output.unwrap_or(0).max(est_chars(produced))
                    },
                )
            } else {
                (0, 0)
            };
            StreamFailure {
                error,
                input_tokens: input,
                output_tokens: output,
                received,
                partial: text.to_owned(),
                stream_refused: false,
            }
        };

    let mut reader = resp.body_mut().as_reader();
    let mut buf = [0u8; 8 * 1024];

    if !is_stream {
        // A server that ignored `stream: true`: read the whole JSON body and show it as one chunk.
        let mut body = Vec::new();
        let limit = u64::try_from(cfg.max_response_bytes)
            .unwrap_or(u64::MAX)
            .saturating_add(1);
        let mut limited = (&mut reader).take(limit);
        loop {
            if handoff.load(Ordering::SeqCst) == ABANDONED {
                return Err(failure(CompanionError::cancelled(), "", &[], &tally, true));
            }
            match limited.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    body.extend_from_slice(&buf[..n]);
                    let _ = tx.send(Msg::Activity);
                }
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                    return Err(failure(CompanionError::timeout(), "", &[], &tally, true));
                }
                Err(_) => return Err(failure(CompanionError::offline(), "", &[], &tally, true)),
            }
        }
        if body.len() > cfg.max_response_bytes {
            return Err(failure(
                CompanionError::invalid_response(),
                "",
                &[],
                &tally,
                true,
            ));
        }
        let (mut t, tool_calls, input_tokens, output_tokens) = if kind == ProviderKind::OpenAi {
            // A Responses body (also the answer to the non-streaming retry).
            let r = parse_responses_body(&body, &ctx.key)
                .map_err(|e| failure(e, "", &[], &tally, true))?;
            let t = clean_reply(&r.text);
            if t.trim().is_empty() && r.calls.is_empty() {
                return Err(failure(
                    CompanionError::invalid_response(),
                    "",
                    &[],
                    &tally,
                    true,
                ));
            }
            let calls = if tools_on { r.calls } else { Vec::new() };
            let produced =
                t.chars().count() + calls.iter().map(|c| c.arguments.len()).sum::<usize>();
            (
                t,
                calls,
                r.input.unwrap_or_else(|| est_chars(sent_chars)),
                r.output.unwrap_or_else(|| est_chars(produced)),
            )
        } else {
            let reply = parse_reply(kind, &body, sent_chars)
                .map_err(|e| failure(e, "", &[], &tally, true))?;
            (
                reply.text,
                Vec::new(),
                reply.input_tokens,
                reply.output_tokens,
            )
        };
        if t.len() > cfg.max_reply_bytes {
            let mut end = cfg.max_reply_bytes;
            while !t.is_char_boundary(end) {
                end -= 1;
            }
            t.truncate(end);
            cut = true;
        }
        if !t.is_empty() {
            let _ = tx.send(Msg::Delta(t.clone()));
        }
        return Ok(StreamReply {
            text: t,
            tool_calls,
            input_tokens,
            output_tokens,
            reported: true,
            cut,
        });
    }

    let mut parser = SseParser::new(cfg.max_event_bytes);
    let mut dec = Decoder::new(kind).with_key(&ctx.key);
    let mut total = 0usize;
    let mut done = false;
    'read: loop {
        if handoff.load(Ordering::SeqCst) == ABANDONED {
            return Err(failure(
                CompanionError::cancelled(),
                &text,
                &calls,
                &tally,
                received,
            ));
        }
        let n = match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                return Err(failure(
                    CompanionError::timeout(),
                    &text,
                    &calls,
                    &tally,
                    received,
                ));
            }
            Err(_) => {
                return Err(failure(
                    CompanionError::offline(),
                    &text,
                    &calls,
                    &tally,
                    received,
                ));
            }
        };
        received = true;
        total += n;
        let _ = tx.send(Msg::Activity);
        if total > cfg.max_stream_bytes {
            return Err(failure(
                CompanionError::invalid_response(),
                &text,
                &calls,
                &tally,
                received,
            ));
        }
        let events = parser.feed(&buf[..n]).map_err(|_| {
            failure(
                CompanionError::invalid_response(),
                &text,
                &calls,
                &tally,
                received,
            )
        })?;
        for ev in events {
            let pieces = dec
                .push(&ev)
                .map_err(|e| failure(e, &text, &calls, &tally, received))?;
            for piece in pieces {
                match piece {
                    Piece::Text(t) => {
                        let mut clean = clean_chunk(&t);
                        if clean.is_empty() {
                            continue;
                        }
                        let room = cfg.max_reply_bytes.saturating_sub(text.len());
                        if clean.len() > room {
                            let mut end = room;
                            while !clean.is_char_boundary(end) {
                                end -= 1;
                            }
                            clean.truncate(end);
                            cut = true;
                        }
                        if !clean.is_empty() {
                            text.push_str(&clean);
                            if handoff.load(Ordering::SeqCst) != ABANDONED {
                                let _ = tx.send(Msg::Delta(clean));
                            }
                        }
                        if cut {
                            break 'read;
                        }
                    }
                    Piece::Tool(c) => {
                        if tools_on {
                            calls.push(c);
                        }
                    }
                    Piece::Usage {
                        input,
                        output,
                        output_final,
                    } => {
                        if input.is_some() {
                            tally.input = input;
                        }
                        if let Some(o) = output {
                            tally.output = Some(o);
                            tally.output_final = output_final;
                        }
                    }
                    Piece::Done => done = true,
                }
            }
            if done {
                break 'read;
            }
        }
    }
    if !done && !cut && !dec.complete() {
        return Err(failure(ended_early(), &text, &calls, &tally, received));
    }
    if text.trim().is_empty() && calls.is_empty() {
        return Err(failure(
            CompanionError::invalid_response(),
            &text,
            &calls,
            &tally,
            received,
        ));
    }
    let produced = text.chars().count() + calls.iter().map(|c| c.arguments.len()).sum::<usize>();
    let input_tokens = tally.input.unwrap_or_else(|| est_chars(sent_chars));
    let output_tokens = if tally.output_final {
        tally.output.unwrap_or(0)
    } else {
        tally.output.unwrap_or(0).max(est_chars(produced))
    };
    Ok(StreamReply {
        text,
        tool_calls: calls,
        input_tokens,
        output_tokens,
        reported: tally.input.is_some() && tally.output_final,
        cut,
    })
}

/// Streams one provider call.
///
/// - `on_delta` gets each cleaned chunk on the calling thread, never after a cancel.
/// - On cancel or silence timeout this returns at once; `on_abandoned` later runs on the worker
///   with what the call cost (the provider's numbers if it gave them, otherwise an estimate from
///   the request and the text received). It does not run when this returns normally.
pub fn chat_stream<F, A>(
    cfg: &HttpConfig,
    spec: &ProviderSpec,
    key: &Secret,
    plan: &RequestPlan<'_>,
    cancel: &CancelToken,
    mut on_delta: F,
    on_abandoned: A,
) -> Result<StreamReply, StreamFailure>
where
    F: FnMut(&str),
    A: FnOnce(LateUsage) + Send + 'static,
{
    let checked = check_base_url(&spec.base_url).map_err(StreamFailure::bare)?;
    let spec = ProviderSpec {
        base_url: checked.url,
        ..spec.clone()
    };
    let built = build_plan_request(&spec, key.expose(), plan);
    // Built up front, sent only if OpenAI refuses streaming. Never built for other providers.
    let fallback = (spec.kind == ProviderKind::OpenAi && plan.stream).then(|| {
        build_plan_request(
            &spec,
            key.expose(),
            &RequestPlan {
                stream: false,
                ..*plan
            },
        )
    });
    let sent_chars: usize = plan
        .messages
        .iter()
        .map(|m| m.content.chars().count())
        .sum::<usize>()
        + plan
            .rounds
            .iter()
            .map(|r| {
                r.text.chars().count()
                    + r.results.iter().map(|s| s.chars().count()).sum::<usize>()
                    + r.calls.iter().map(|c| c.arguments.len()).sum::<usize>()
            })
            .sum::<usize>();
    let ctx = ErrCtx {
        key: zeroize::Zeroizing::new(key.expose().to_owned()),
        echoes: plan
            .messages
            .iter()
            .map(|m| m.content.clone())
            .chain(
                plan.rounds
                    .iter()
                    .flat_map(|r| std::iter::once(r.text.clone()).chain(r.results.iter().cloned())),
            )
            .collect(),
    };
    let kind = spec.kind;
    let tools_on = plan.tools;
    let cfg = *cfg;
    let loopback = checked.loopback;
    let (tx, rx) = mpsc::channel();
    let handoff = Arc::new(AtomicU8::new(PENDING));
    let worker_handoff = Arc::clone(&handoff);
    std::thread::spawn(move || {
        let mut result = stream_call(
            &cfg,
            loopback,
            kind,
            tools_on,
            sent_chars,
            built,
            &ctx,
            &worker_handoff,
            &tx,
        );
        // OpenAI refuses `stream: true` for organisations that are not verified (HTTP 400, `param`
        // is `stream`). Try once more without streaming and tell the user.
        let refused = matches!(&result, Err(f) if f.stream_refused);
        if let (true, Some(fb)) = (refused, fallback) {
            let mut retried = stream_call(
                &cfg,
                loopback,
                kind,
                tools_on,
                sent_chars,
                fb,
                &ctx,
                &worker_handoff,
                &tx,
            );
            if let Ok(reply) = &mut retried
                && !reply.text.is_empty()
            {
                reply.text.push_str(NO_STREAM_NOTICE);
                if worker_handoff.load(Ordering::SeqCst) != ABANDONED {
                    let _ = tx.send(Msg::Delta(NO_STREAM_NOTICE.to_owned()));
                }
            }
            result = retried;
        }
        if worker_handoff
            .compare_exchange(PENDING, DELIVERED, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            let _ = tx.send(Msg::End(result));
        } else {
            let usage = match &result {
                Ok(r) => LateUsage {
                    input_tokens: r.input_tokens,
                    output_tokens: r.output_tokens,
                    reported: r.reported,
                },
                Err(f) if f.received => LateUsage {
                    input_tokens: f.input_tokens,
                    output_tokens: f.output_tokens,
                    reported: false,
                },
                Err(_) => LateUsage {
                    input_tokens: est_chars(sent_chars),
                    output_tokens: 0,
                    reported: false,
                },
            };
            on_abandoned(usage);
        }
    });

    let mut last = Instant::now();
    let mut seen = false;
    loop {
        if cancel.is_cancelled()
            && handoff
                .compare_exchange(PENDING, ABANDONED, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
        {
            return Err(StreamFailure::bare(CompanionError::cancelled()));
        }
        match rx.recv_timeout(Duration::from_millis(40)) {
            Ok(Msg::Delta(t)) => {
                last = Instant::now();
                seen = true;
                if !cancel.is_cancelled() {
                    on_delta(&t);
                }
            }
            Ok(Msg::Activity) => {
                last = Instant::now();
                seen = true;
            }
            Ok(Msg::End(r)) => return r,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let limit = if seen {
                    cfg.idle_timeout
                } else {
                    cfg.first_byte_timeout
                };
                if last.elapsed() > limit
                    && handoff
                        .compare_exchange(PENDING, ABANDONED, Ordering::SeqCst, Ordering::SeqCst)
                        .is_ok()
                {
                    return Err(StreamFailure::bare(CompanionError::timeout()));
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(StreamFailure::bare(CompanionError::internal()));
            }
        }
    }
}
