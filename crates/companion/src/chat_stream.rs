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
    clean_reply, estimate, map_status_reason, parse_reply_with,
};
use crate::secret::Secret;
use crate::sse::SseParser;
use crate::stream::{Decoder, Piece, ended_early, incomplete_error, parse_responses_body};
use crate::tools::ToolCall;
use crate::url_check::check_base_url;

/// What a call taught about the model, for the caller to remember for this app run (OpenAI
/// Responses only): it answered HTTP 400 to `reasoning` or to `stream: true`, and the call went on
/// without. Later calls to the same model should start without them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Learned {
    pub reasoning_refused: bool,
    pub stream_refused: bool,
}

/// A finished (or cut) streamed reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamReply {
    /// False when the reply arrived all at once (the request was not streamed, was retried
    /// without streaming, or the server ignored `stream: true`). The caller keeps this as a note
    /// next to the reply; it is never part of the reply text.
    pub streamed: bool,
    pub learned: Learned,
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
    /// The provider refused `reasoning` (OpenAI, HTTP 400, `param` starts with `reasoning`).
    pub reasoning_refused: bool,
    pub learned: Learned,
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
            reasoning_refused: false,
            learned: Learned::default(),
        }
    }
}

enum Msg {
    Activity,
    Delta(String),
    /// The worker is about to send a request that cannot stream: the whole reply is produced
    /// before the first byte, so the caller must wait for the total timeout, not the first-byte one.
    Retrying,
    End(Result<StreamReply, StreamFailure>),
}

/// What the error-body reader and the failure bookkeeping need.
struct ErrCtx {
    /// The key, hidden in provider messages.
    key: zeroize::Zeroizing<String>,
    /// What we sent: a provider message quoting it is dropped.
    echoes: Arc<crate::reason::Echoes>,
    /// What to book for a call that failed with no usage reported (the request's output cap).
    max_out: i64,
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
        let param = reason
            .as_ref()
            .and_then(|r| r.param.as_deref())
            .unwrap_or("");
        failure.reasoning_refused =
            status == 400 && kind == ProviderKind::OpenAi && param.starts_with("reasoning");
        failure.stream_refused = status == 400 && kind == ProviderKind::OpenAi && param == "stream";
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
                reasoning_refused: false,
                learned: Learned::default(),
            }
        };

    let mut reader = resp.body_mut().as_reader();
    let mut buf = [0u8; 8 * 1024];

    if !is_stream {
        // A reply that is not streamed (the request said so, or the server ignored
        // `stream: true`): read the whole JSON body and show it as one chunk. The provider has
        // already produced the reply, so a failure from here on books the output cap unless the
        // body says what it cost.
        tally.output = Some(ctx.max_out);
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
            let r = match parse_responses_body(&body, &ctx.key, &ctx.echoes, tools_on) {
                Ok(r) => r,
                Err(f) => {
                    // A reported failure: book what the provider counted, else the output cap.
                    tally.input = f.input.or(tally.input);
                    tally.output = f.output.or(tally.output);
                    return Err(failure(f.error, "", &[], &tally, true));
                }
            };
            tally.input = r.input.or(tally.input);
            tally.output = r.output.or(tally.output);
            let t = clean_reply(&r.text);
            // Calls were already filtered by `tools_on`, so a reply that only made calls the
            // request did not allow counts as empty.
            if t.trim().is_empty() && r.calls.is_empty() {
                let error = match &r.incomplete {
                    Some(why) => incomplete_error(Some(why)),
                    None => CompanionError::invalid_response(),
                };
                return Err(failure(error, "", &[], &tally, true));
            }
            let produced =
                t.chars().count() + r.calls.iter().map(|c| c.arguments.len()).sum::<usize>();
            (
                t,
                r.calls,
                r.input.unwrap_or_else(|| est_chars(sent_chars)),
                r.output.unwrap_or_else(|| est_chars(produced)),
            )
        } else {
            let reply = parse_reply_with(
                kind,
                &body,
                sent_chars,
                u32::try_from(ctx.max_out).unwrap_or(u32::MAX),
                &ctx.key,
                &ctx.echoes,
            )
            .map_err(|e| {
                if let Some((i, o)) = e.usage {
                    tally.input = Some(i);
                    tally.output = Some(o);
                }
                failure(e, "", &[], &tally, true)
            })?;
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
            streamed: false,
            learned: Learned::default(),
            text: t,
            tool_calls,
            input_tokens,
            output_tokens,
            reported: true,
            cut,
        });
    }

    let mut parser = SseParser::new(cfg.max_event_bytes);
    let mut dec = Decoder::new(kind)
        .with_key(&ctx.key)
        .with_echoes(&ctx.echoes)
        .with_tools(tools_on);
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
            let pieces = match dec.push(&ev) {
                Ok(p) => p,
                Err(e) => {
                    // The provider reported a failure (or sent something unusable) after working
                    // on the call: book the last usage it reported, else the output cap.
                    let (i, o) = dec.reported_usage();
                    tally.input = i.or(tally.input);
                    tally.output = Some(o.or(tally.output).unwrap_or(ctx.max_out));
                    return Err(failure(e, &text, &calls, &tally, received));
                }
            };
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
        streamed: true,
        learned: Learned::default(),
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
    // OpenAI Responses only: the request may have to be repeated without `reasoning` (the model
    // has no such setting) and/or without `stream` (the organisation may not stream). Each is
    // tried once, so build the (at most four) variants now.
    let reasoning0 = kind_is_openai(&spec) && plan.reasoning;
    let stream0 = plan.stream;
    let mut variants: Vec<((bool, bool), BuiltRequest)> = Vec::new();
    let mut want = vec![(reasoning0, stream0)];
    if kind_is_openai(&spec) {
        if reasoning0 {
            want.push((false, stream0));
        }
        if stream0 {
            want.push((reasoning0, false));
        }
        if reasoning0 && stream0 {
            want.push((false, false));
        }
    }
    for (r, s) in want {
        variants.push((
            (r, s),
            build_plan_request(
                &spec,
                key.expose(),
                &RequestPlan {
                    reasoning: r,
                    stream: s,
                    ..*plan
                },
            ),
        ));
    }
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
        echoes: Arc::new(crate::reason::Echoes::new(
            plan.messages.iter().map(|m| m.content.clone()).chain(
                plan.rounds
                    .iter()
                    .flat_map(|r| std::iter::once(r.text.clone()).chain(r.results.iter().cloned())),
            ),
        )),
        max_out: i64::from(plan.max_output_tokens),
    };
    let kind = spec.kind;
    let tools_on = plan.tools;
    let cfg = *cfg;
    let loopback = checked.loopback;
    let (tx, rx) = mpsc::channel();
    let handoff = Arc::new(AtomicU8::new(PENDING));
    let worker_handoff = Arc::clone(&handoff);
    std::thread::spawn(move || {
        let (mut reasoning, mut stream) = (reasoning0, stream0);
        let mut learned = Learned::default();
        // A request that cannot stream costs the provider the whole reply before the first byte.
        let mut non_stream_sent = !stream0;
        let mut result;
        loop {
            let Some(pos) = variants.iter().position(|(k, _)| *k == (reasoning, stream)) else {
                result = Err(StreamFailure::bare(CompanionError::internal()));
                break;
            };
            let (_, built) = variants.swap_remove(pos);
            result = stream_call(
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
            // Never start another request for a call the user has already given up on.
            if worker_handoff.load(Ordering::SeqCst) == ABANDONED {
                break;
            }
            match &result {
                // The model has no `reasoning` setting: repeat without it, once.
                Err(f) if f.reasoning_refused && reasoning => {
                    reasoning = false;
                    learned.reasoning_refused = true;
                }
                // OpenAI refuses `stream: true` (organisation not verified): repeat without it,
                // once. The caller is told so it waits for the total time, not the first byte.
                Err(f) if f.stream_refused && stream => {
                    stream = false;
                    learned.stream_refused = true;
                    non_stream_sent = true;
                    let _ = tx.send(Msg::Retrying);
                }
                _ => break,
            }
        }
        match &mut result {
            Ok(r) => r.learned = learned,
            Err(f) => f.learned = learned,
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
                    // A non-streamed call was worked on in full before any byte came back.
                    output_tokens: if non_stream_sent { ctx.max_out } else { 0 },
                    reported: false,
                },
            };
            on_abandoned(usage);
        }
    });

    let mut last = Instant::now();
    let mut seen = false;
    let mut retrying = !stream0;
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
            Ok(Msg::Retrying) => {
                last = Instant::now();
                retrying = true;
            }
            Ok(Msg::End(r)) => return r,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let limit = if retrying {
                    cfg.total_timeout
                } else if seen {
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

fn kind_is_openai(spec: &ProviderSpec) -> bool {
    spec.kind == ProviderKind::OpenAi
}
