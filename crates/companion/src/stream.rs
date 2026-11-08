//! Turns provider stream events into text, tool calls and usage (ADR-020, streaming).
//!
//! Anthropic Messages API streaming: `message_start` (usage.input_tokens), `content_block_start`
//! (`text` or `tool_use` with id and name), `content_block_delta` (`text_delta`,
//! `input_json_delta`), `content_block_stop`, `message_delta` (usage.output_tokens, cumulative),
//! `message_stop`, `ping`, `error`. Unknown event and delta types are ignored so new provider
//! features do not break a reply.
//!
//! OpenAI Chat Completions streaming (also the shape assumed for compatible servers): `data:`
//! chunks with `choices[0].delta.content` and `delta.tool_calls[{index,id,function{name,
//! arguments}}]`, a `finish_reason`, an optional last chunk with `usage` (when
//! `stream_options.include_usage` is set), and `data: [DONE]`.
//!
//! The decoder does not clean text; the caller does, chunk by chunk.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::error::{CompanionError, ErrorKind};
use crate::provider::ProviderKind;
use crate::sse::SseEvent;
use crate::tools::{MAX_ARGUMENT_BYTES, ToolCall, valid_call_id};

/// Most tool calls one reply may contain; more are an invalid response.
pub const MAX_CALLS_PER_REPLY: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    Text(String),
    /// The model's reasoning (summary), shown apart from the reply. Never part of the reply text
    /// and never sent back to a provider.
    Thinking(String),
    Tool(ToolCall),
    /// Token numbers the provider reported. `output_final` is true when the output number is the
    /// provider's final count for the call.
    Usage {
        input: Option<i64>,
        output: Option<i64>,
        output_final: bool,
    },
    /// The provider said the reply is complete.
    Done,
}

fn bad() -> CompanionError {
    CompanionError::invalid_response()
}

fn token(v: Option<&Value>) -> Option<i64> {
    v.and_then(Value::as_i64).filter(|n| *n >= 0)
}

fn in_stream_error() -> CompanionError {
    CompanionError::new(
        ErrorKind::ProviderError,
        "The provider reported a problem while replying.",
    )
}

pub fn ended_early() -> CompanionError {
    CompanionError::new(
        ErrorKind::ProviderError,
        "The reply stopped before it was complete.",
    )
}

#[derive(Default)]
struct PartialCall {
    id: String,
    name: String,
    args: String,
}

pub struct Decoder {
    kind: ProviderKind,
    // Anthropic: open tool_use blocks by index.
    blocks: BTreeMap<u64, PartialCall>,
    // OpenAI: tool calls by index.
    calls: BTreeMap<u64, PartialCall>,
    emitted_calls: usize,
    /// Provider call ids seen in this reply. A repeat is malformed: two calls with one id
    /// would make the answers ambiguous.
    seen_ids: std::collections::HashSet<String>,
    finished: bool,
    /// Responses API: some text was streamed.
    saw_text: bool,
    /// Responses API: some reasoning summary was streamed (a new summary part is then separated
    /// from the last one by a blank line).
    saw_thinking: bool,
    /// Offer the model lookups; if not, any call it makes is ignored.
    tools: bool,
    /// The last usage the provider reported (Responses API: completed, incomplete or failed).
    usage_in: Option<i64>,
    usage_out: Option<i64>,
    /// The key, hidden in provider messages shown to the user.
    key: String,
    /// What we sent, so a provider message quoting it is not shown.
    echoes: std::sync::Arc<crate::reason::Echoes>,
    done: bool,
}

impl Decoder {
    pub fn new(kind: ProviderKind) -> Self {
        Self {
            kind,
            blocks: BTreeMap::new(),
            calls: BTreeMap::new(),
            emitted_calls: 0,
            seen_ids: std::collections::HashSet::new(),
            finished: false,
            saw_text: false,
            saw_thinking: false,
            tools: true,
            usage_in: None,
            usage_out: None,
            key: String::new(),
            echoes: std::sync::Arc::default(),
            done: false,
        }
    }

    /// Sets the key to hide in provider messages shown to the user.
    #[must_use]
    pub fn with_key(mut self, key: &str) -> Self {
        self.key = key.to_owned();
        self
    }

    /// Whether the request offered lookups. If not, calls the model makes are ignored.
    #[must_use]
    pub fn with_tools(mut self, tools: bool) -> Self {
        self.tools = tools;
        self
    }

    /// Sets what we sent, so a provider message quoting it is dropped.
    #[must_use]
    pub fn with_echoes(mut self, echoes: &std::sync::Arc<crate::reason::Echoes>) -> Self {
        self.echoes = std::sync::Arc::clone(echoes);
        self
    }

    /// True once a complete reply was seen.
    pub fn complete(&self) -> bool {
        self.done || self.finished
    }

    pub fn push(&mut self, ev: &SseEvent) -> Result<Vec<Piece>, CompanionError> {
        match self.kind {
            ProviderKind::Anthropic => self.anthropic(ev),
            ProviderKind::OpenAi => self.responses(ev),
            ProviderKind::OpenAiCompatible => self.openai(ev),
        }
    }

    /// A finished call, validated. With lookups off (`with_tools(false)`) a call the model made
    /// anyway is ignored whole: not validated, not counted, not an error.
    fn finish_call(&mut self, c: PartialCall) -> Result<Option<Piece>, CompanionError> {
        if !self.tools {
            return Ok(None);
        }
        self.emitted_calls += 1;
        if self.emitted_calls > MAX_CALLS_PER_REPLY
            || !valid_call_id(&c.id)
            || c.name.is_empty()
            || c.name.len() > 64
            || c.args.len() > MAX_ARGUMENT_BYTES
            || !self.seen_ids.insert(c.id.clone())
        {
            return Err(bad());
        }
        Ok(Some(Piece::Tool(ToolCall {
            id: c.id,
            name: c.name,
            arguments: c.args,
        })))
    }

    fn anthropic(&mut self, ev: &SseEvent) -> Result<Vec<Piece>, CompanionError> {
        let v: Value = serde_json::from_str(&ev.data).map_err(|_| bad())?;
        let kind = v.get("type").and_then(Value::as_str).unwrap_or("");
        let mut out = Vec::new();
        match kind {
            "message_start" => {
                let u = v.pointer("/message/usage");
                self.note_usage(u);
                out.push(Piece::Usage {
                    input: token(u.and_then(|u| u.get("input_tokens"))),
                    output: token(u.and_then(|u| u.get("output_tokens"))),
                    output_final: false,
                });
            }
            "content_block_start" => {
                let index = v.get("index").and_then(Value::as_u64).ok_or_else(bad)?;
                let b = v.get("content_block").ok_or_else(bad)?;
                match b.get("type").and_then(Value::as_str) {
                    Some("tool_use") => {
                        let s = |k: &str| {
                            b.get(k)
                                .and_then(Value::as_str)
                                .map(str::to_owned)
                                .unwrap_or_default()
                        };
                        self.blocks.insert(
                            index,
                            PartialCall {
                                id: s("id"),
                                name: s("name"),
                                args: String::new(),
                            },
                        );
                    }
                    Some("text") => {
                        if let Some(t) = b.get("text").and_then(Value::as_str)
                            && !t.is_empty()
                        {
                            out.push(Piece::Text(t.to_owned()));
                        }
                    }
                    Some("thinking") => {
                        if let Some(t) = b.get("thinking").and_then(Value::as_str)
                            && !t.is_empty()
                        {
                            out.push(Piece::Thinking(t.to_owned()));
                        }
                    }
                    _ => {}
                }
            }
            "content_block_delta" => {
                let index = v.get("index").and_then(Value::as_u64).ok_or_else(bad)?;
                let d = v.get("delta").ok_or_else(bad)?;
                match d.get("type").and_then(Value::as_str) {
                    Some("text_delta") => {
                        let t = d.get("text").and_then(Value::as_str).ok_or_else(bad)?;
                        if !t.is_empty() {
                            out.push(Piece::Text(t.to_owned()));
                        }
                    }
                    Some("thinking_delta") => {
                        // Empty when the model's `display` is `omitted`; `signature_delta` is
                        // ignored (it is only needed to send thinking back, which is never done).
                        if let Some(t) = d.get("thinking").and_then(Value::as_str)
                            && !t.is_empty()
                        {
                            out.push(Piece::Thinking(t.to_owned()));
                        }
                    }
                    Some("input_json_delta") => {
                        let t = d
                            .get("partial_json")
                            .and_then(Value::as_str)
                            .ok_or_else(bad)?;
                        if let Some(c) = self.blocks.get_mut(&index) {
                            if c.args.len() + t.len() > MAX_ARGUMENT_BYTES {
                                return Err(bad());
                            }
                            c.args.push_str(t);
                        }
                    }
                    _ => {}
                }
            }
            "content_block_stop" => {
                let index = v.get("index").and_then(Value::as_u64).ok_or_else(bad)?;
                if let Some(c) = self.blocks.remove(&index) {
                    out.extend(self.finish_call(c)?);
                }
            }
            "message_delta" => {
                let u = v.get("usage");
                self.note_usage(u);
                out.push(Piece::Usage {
                    input: token(u.and_then(|u| u.get("input_tokens"))),
                    output: token(u.and_then(|u| u.get("output_tokens"))),
                    output_final: true,
                });
            }
            "message_stop" => {
                self.done = true;
                out.push(Piece::Done);
            }
            "error" => return Err(in_stream_error()),
            _ => {} // ping and anything newer
        }
        Ok(out)
    }

    fn flush_openai_calls(&mut self) -> Result<Vec<Piece>, CompanionError> {
        let calls = std::mem::take(&mut self.calls);
        let mut out = Vec::new();
        for c in calls.into_values() {
            out.extend(self.finish_call(c)?);
        }
        Ok(out)
    }

    /// The error for `response.failed`, `response.incomplete` without output and the `error`
    /// event: the fixed sentence plus the provider's sanitised message, if it sent one.
    fn responses_error(&self, message: Option<&str>) -> CompanionError {
        let reason = message.and_then(|m| crate::reason::excerpt(m, &self.key, &self.echoes));
        in_stream_error().with_reason(crate::reason::provider_reason(
            ProviderKind::OpenAi,
            reason.as_ref(),
        ))
    }

    /// The last usage the provider reported (input, output), whatever ended the reply. Used to
    /// book a call that failed after it had been worked on.
    pub fn reported_usage(&self) -> (Option<i64>, Option<i64>) {
        (self.usage_in, self.usage_out)
    }

    fn note_usage(&mut self, u: Option<&Value>) {
        if let Some(u) = u.filter(|u| u.is_object()) {
            if let Some(i) = token(u.get("input_tokens")) {
                self.usage_in = Some(i);
            }
            if let Some(o) = token(u.get("output_tokens")) {
                self.usage_out = Some(o);
            }
        }
    }

    /// OpenAI Responses API events (verified 2026-11-07, ADR-020). Every `data:` payload is JSON
    /// with a `type`. Unknown types are ignored so new features do not break a reply.
    fn responses(&mut self, ev: &SseEvent) -> Result<Vec<Piece>, CompanionError> {
        let data = ev.data.trim();
        if data == "[DONE]" {
            // Not part of the Responses stream; tolerated.
            return Ok(Vec::new());
        }
        let v: Value = serde_json::from_str(data).map_err(|_| bad())?;
        let kind = v.get("type").and_then(Value::as_str).unwrap_or("");
        let index = v.get("output_index").and_then(Value::as_u64).unwrap_or(0);
        if index >= 64 {
            return Err(bad());
        }
        let function_call = |v: &Value| -> bool {
            v.pointer("/item/type").and_then(Value::as_str) == Some("function_call")
        };
        let mut out = Vec::new();
        match kind {
            "response.output_text.delta" => {
                let t = v.get("delta").and_then(Value::as_str).ok_or_else(bad)?;
                if !t.is_empty() {
                    self.saw_text = true;
                    out.push(Piece::Text(t.to_owned()));
                }
            }
            "response.reasoning_summary_text.delta" => {
                if let Some(t) = v.get("delta").and_then(Value::as_str)
                    && !t.is_empty()
                {
                    self.saw_thinking = true;
                    out.push(Piece::Thinking(t.to_owned()));
                }
            }
            "response.reasoning_summary_part.added" if self.saw_thinking => {
                out.push(Piece::Thinking(
                    "

"
                    .to_owned(),
                ));
            }
            "response.output_item.added" if function_call(&v) => {
                let item = v.get("item").ok_or_else(bad)?;
                let s = |k: &str| {
                    item.get(k)
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                        .unwrap_or_default()
                };
                let args = s("arguments");
                if args.len() > MAX_ARGUMENT_BYTES {
                    return Err(bad());
                }
                self.calls.insert(
                    index,
                    PartialCall {
                        id: s("call_id"),
                        name: s("name"),
                        args,
                    },
                );
            }
            "response.function_call_arguments.delta" => {
                let t = v.get("delta").and_then(Value::as_str).ok_or_else(bad)?;
                if let Some(c) = self.calls.get_mut(&index) {
                    if c.args.len() + t.len() > MAX_ARGUMENT_BYTES {
                        return Err(bad());
                    }
                    c.args.push_str(t);
                }
            }
            "response.function_call_arguments.done" => {
                if let (Some(c), Some(a)) = (
                    self.calls.get_mut(&index),
                    v.get("arguments").and_then(Value::as_str),
                ) {
                    if a.len() > MAX_ARGUMENT_BYTES {
                        return Err(bad());
                    }
                    c.args = a.to_owned();
                }
            }
            "response.output_item.done" if function_call(&v) => {
                let item = v.get("item").ok_or_else(bad)?;
                let mut c = self.calls.remove(&index).unwrap_or_default();
                if let Some(id) = item.get("call_id").and_then(Value::as_str) {
                    c.id = id.to_owned();
                }
                if let Some(n) = item.get("name").and_then(Value::as_str) {
                    c.name = n.to_owned();
                }
                if let Some(a) = item.get("arguments").and_then(Value::as_str) {
                    c.args = a.to_owned();
                }
                out.extend(self.finish_call(c)?);
            }
            "response.completed" | "response.incomplete" => {
                let u = v.pointer("/response/usage").filter(|u| u.is_object());
                self.note_usage(u);
                out.push(Piece::Usage {
                    input: token(u.and_then(|u| u.get("input_tokens"))),
                    output: token(u.and_then(|u| u.get("output_tokens"))),
                    output_final: u.is_some(),
                });
                out.extend(self.flush_openai_calls()?);
                if kind == "response.incomplete" && !self.saw_text && self.emitted_calls == 0 {
                    let why = v
                        .pointer("/response/incomplete_details/reason")
                        .and_then(Value::as_str);
                    return Err(incomplete_error(why));
                }
                self.done = true;
                out.push(Piece::Done);
            }
            "response.failed" => {
                self.note_usage(v.pointer("/response/usage"));
                let m = v.pointer("/response/error/message").and_then(Value::as_str);
                return Err(self.responses_error(m));
            }
            "error" => {
                return Err(self.responses_error(v.get("message").and_then(Value::as_str)));
            }
            _ => {} // created, in_progress, content_part.*, reasoning.* and anything newer
        }
        Ok(out)
    }

    fn openai(&mut self, ev: &SseEvent) -> Result<Vec<Piece>, CompanionError> {
        let data = ev.data.trim();
        if data == "[DONE]" {
            let mut out = self.flush_openai_calls()?;
            self.done = true;
            out.push(Piece::Done);
            return Ok(out);
        }
        let v: Value = serde_json::from_str(data).map_err(|_| bad())?;
        if v.get("error").is_some_and(|e| !e.is_null()) {
            return Err(in_stream_error());
        }
        let mut out = Vec::new();
        if let Some(u) = v.get("usage").filter(|u| u.is_object()) {
            self.usage_in = token(u.get("prompt_tokens")).or(self.usage_in);
            self.usage_out = token(u.get("completion_tokens")).or(self.usage_out);
            out.push(Piece::Usage {
                input: token(u.get("prompt_tokens")),
                output: token(u.get("completion_tokens")),
                output_final: true,
            });
        }
        let Some(choice) = v.pointer("/choices/0") else {
            return Ok(out); // the usage-only chunk has no choices
        };
        if let Some(d) = choice.get("delta") {
            // Reasoning servers (DeepSeek-style `reasoning_content`, others `reasoning`).
            for field in ["reasoning_content", "reasoning"] {
                if let Some(t) = d.get(field).and_then(Value::as_str)
                    && !t.is_empty()
                {
                    out.push(Piece::Thinking(t.to_owned()));
                    break;
                }
            }
            if let Some(t) = d.get("content").and_then(Value::as_str)
                && !t.is_empty()
            {
                out.push(Piece::Text(t.to_owned()));
            }
            if let Some(tcs) = d.get("tool_calls").and_then(Value::as_array) {
                for tc in tcs {
                    let index = tc.get("index").and_then(Value::as_u64).unwrap_or(0);
                    if index >= 64 {
                        return Err(bad());
                    }
                    let c = self.calls.entry(index).or_default();
                    if let Some(id) = tc.get("id").and_then(Value::as_str) {
                        c.id = id.to_owned();
                    }
                    if let Some(n) = tc.pointer("/function/name").and_then(Value::as_str) {
                        c.name.push_str(n);
                    }
                    if let Some(a) = tc.pointer("/function/arguments").and_then(Value::as_str) {
                        if c.args.len() + a.len() > MAX_ARGUMENT_BYTES {
                            return Err(bad());
                        }
                        c.args.push_str(a);
                    }
                }
            }
        }
        if choice
            .get("finish_reason")
            .is_some_and(|f| f.as_str().is_some())
        {
            self.finished = true;
            out.extend(self.flush_openai_calls()?);
        }
        Ok(out)
    }
}

/// The error for a Responses reply that stopped before it wrote anything (`status: incomplete`
/// or `response.incomplete`). `why` is `incomplete_details.reason`; only a short plain identifier
/// is ever repeated.
pub fn incomplete_error(why: Option<&str>) -> CompanionError {
    let plain = why.filter(|w| {
        !w.is_empty() && w.len() <= 40 && w.chars().all(|c| c.is_ascii_lowercase() || c == '_')
    });
    let message = match plain {
        Some("max_output_tokens") => {
            "The model reached its output limit before it wrote a reply (it used the allowance thinking). Try again, ask for something shorter, or choose another model.".to_owned()
        }
        Some(other) => {
            format!("The model stopped before it wrote a reply (reason: {other}).")
        }
        None => "The model stopped before it wrote a reply.".to_owned(),
    };
    CompanionError::new(ErrorKind::ProviderError, message)
}

/// A whole (non-streamed) OpenAI Responses body, read the same way as the stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponsesBody {
    pub text: String,
    pub calls: Vec<ToolCall>,
    pub input: Option<i64>,
    pub output: Option<i64>,
    /// `status` was `incomplete`: the `incomplete_details.reason` (or `unknown`). The text and
    /// calls may be empty; the caller decides whether that is an error.
    pub incomplete: Option<String>,
}

/// A body that is an error: the error, and the usage the provider reported for it (if any).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BodyFailure {
    pub error: CompanionError,
    pub input: Option<i64>,
    pub output: Option<i64>,
}

impl BodyFailure {
    fn bare(error: CompanionError) -> Self {
        Self {
            error,
            input: None,
            output: None,
        }
    }
}

/// Parses a 2xx Responses body: `output[]` message items (`content[]` parts of type
/// `output_text`) and `function_call` items, `usage.input_tokens` / `usage.output_tokens`, and a
/// `status` of `failed` (an error, with its usage) or `incomplete`. Text is not cleaned here.
/// With `tools` false any function call is ignored before anything else looks at it.
pub fn parse_responses_body(
    body: &[u8],
    key: &str,
    echoes: &std::sync::Arc<crate::reason::Echoes>,
    tools: bool,
) -> Result<ResponsesBody, BodyFailure> {
    let v: Value = serde_json::from_slice(body).map_err(|_| BodyFailure::bare(bad()))?;
    let mut d = Decoder::new(ProviderKind::OpenAi)
        .with_key(key)
        .with_echoes(echoes)
        .with_tools(tools);
    let u = v.get("usage");
    d.note_usage(u);
    let (input, output) = d.reported_usage();
    let status = v
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("completed");
    if status == "failed" {
        return Err(BodyFailure {
            error: d.responses_error(v.pointer("/error/message").and_then(Value::as_str)),
            input,
            output,
        });
    }
    let items = v
        .get("output")
        .and_then(Value::as_array)
        .ok_or_else(|| BodyFailure::bare(bad()))?;
    let mut text = String::new();
    let mut calls = Vec::new();
    for item in items {
        match item.get("type").and_then(Value::as_str) {
            Some("message") => {
                for part in item
                    .get("content")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    if part.get("type").and_then(Value::as_str) == Some("output_text")
                        && let Some(t) = part.get("text").and_then(Value::as_str)
                    {
                        text.push_str(t);
                    }
                }
            }
            Some("function_call") => {
                let s = |k: &str| {
                    item.get(k)
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                        .unwrap_or_default()
                };
                let piece = d
                    .finish_call(PartialCall {
                        id: s("call_id"),
                        name: s("name"),
                        args: s("arguments"),
                    })
                    .map_err(|e| BodyFailure {
                        error: e,
                        input,
                        output,
                    })?;
                if let Some(Piece::Tool(c)) = piece {
                    calls.push(c);
                }
            }
            _ => {}
        }
    }
    let incomplete = (status == "incomplete").then(|| {
        v.pointer("/incomplete_details/reason")
            .and_then(Value::as_str)
            .map_or_else(|| "unknown".to_owned(), str::to_owned)
    });
    Ok(ResponsesBody {
        text,
        calls,
        input,
        output,
        incomplete,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(data: &str) -> SseEvent {
        SseEvent {
            event: None,
            data: data.to_owned(),
        }
    }

    fn run(kind: ProviderKind, events: &[&str]) -> Result<Vec<Piece>, CompanionError> {
        let mut d = Decoder::new(kind);
        let mut out = Vec::new();
        for e in events {
            out.extend(d.push(&ev(e))?);
        }
        Ok(out)
    }

    fn text(p: &[Piece]) -> String {
        p.iter()
            .filter_map(|p| match p {
                Piece::Text(t) => Some(t.as_str()),
                _ => None,
            })
            .collect()
    }

    fn thinking(p: &[Piece]) -> String {
        p.iter()
            .filter_map(|p| match p {
                Piece::Thinking(t) => Some(t.as_str()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn anthropic_thinking_is_separate_from_text() {
        let p = run(
            ProviderKind::Anthropic,
            &[
                r#"{"type":"message_start","message":{"usage":{"input_tokens":5,"output_tokens":1}}}"#,
                r#"{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}"#,
                r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"Let me "}}"#,
                r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"think."}}"#,
                r#"{"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"SIGNATURE"}}"#,
                r#"{"type":"content_block_stop","index":0}"#,
                r#"{"type":"content_block_start","index":1,"content_block":{"type":"redacted_thinking","data":"XXXX"}}"#,
                r#"{"type":"content_block_stop","index":1}"#,
                r#"{"type":"content_block_start","index":2,"content_block":{"type":"text","text":""}}"#,
                r#"{"type":"content_block_delta","index":2,"delta":{"type":"text_delta","text":"Answer"}}"#,
                r#"{"type":"content_block_stop","index":2}"#,
                r#"{"type":"message_stop"}"#,
            ],
        )
        .unwrap();
        assert_eq!(thinking(&p), "Let me think.");
        assert_eq!(text(&p), "Answer");
        let all = format!("{p:?}");
        assert!(!all.contains("SIGNATURE") && !all.contains("XXXX"));
    }

    #[test]
    fn anthropic_omitted_thinking_streams_nothing() {
        let p = run(
            ProviderKind::Anthropic,
            &[
                r#"{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}"#,
                r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":""}}"#,
                r#"{"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"s"}}"#,
                r#"{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"hi"}}"#,
                r#"{"type":"message_stop"}"#,
            ],
        )
        .unwrap();
        assert_eq!(thinking(&p), "");
        assert_eq!(text(&p), "hi");
    }

    #[test]
    fn responses_reasoning_summary_is_thinking_with_part_breaks() {
        let p = run(
            ProviderKind::OpenAi,
            &[
                r#"{"type":"response.reasoning_summary_part.added","output_index":0,"summary_index":0,"part":{"type":"summary_text","text":""}}"#,
                r#"{"type":"response.reasoning_summary_text.delta","output_index":0,"summary_index":0,"delta":"**First**"}"#,
                r#"{"type":"response.reasoning_summary_text.done","output_index":0,"summary_index":0,"text":"**First**"}"#,
                r#"{"type":"response.reasoning_summary_part.added","output_index":0,"summary_index":1,"part":{"type":"summary_text","text":""}}"#,
                r#"{"type":"response.reasoning_summary_text.delta","output_index":0,"summary_index":1,"delta":"Second"}"#,
                r#"{"type":"response.output_text.delta","output_index":1,"delta":"ok"}"#,
                r#"{"type":"response.completed","response":{}}"#,
            ],
        )
        .unwrap();
        assert_eq!(thinking(&p), "**First**\n\nSecond");
        assert_eq!(text(&p), "ok");
    }

    #[test]
    fn chat_completions_reasoning_fields_are_thinking() {
        let p = run(
            ProviderKind::OpenAiCompatible,
            &[
                r#"{"choices":[{"delta":{"reasoning_content":"hm "}}]}"#,
                r#"{"choices":[{"delta":{"reasoning":"ok"}}]}"#,
                r#"{"choices":[{"delta":{"reasoning_content":null,"content":"Hi"}}]}"#,
                r#"{"choices":[{"delta":{},"finish_reason":"stop"}]}"#,
                "[DONE]",
            ],
        )
        .unwrap();
        assert_eq!(thinking(&p), "hm ok");
        assert_eq!(text(&p), "Hi");
    }

    #[test]
    fn anthropic_text_and_usage() {
        let p = run(
            ProviderKind::Anthropic,
            &[
                r#"{"type":"message_start","message":{"usage":{"input_tokens":25,"output_tokens":1}}}"#,
                r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
                r#"{"type":"ping"}"#,
                r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hel"}}"#,
                r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"lo"}}"#,
                r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"x"}}"#,
                r#"{"type":"content_block_stop","index":0}"#,
                r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":15}}"#,
                r#"{"type":"message_stop"}"#,
            ],
        )
        .unwrap();
        assert_eq!(text(&p), "Hello");
        assert!(p.contains(&Piece::Usage {
            input: Some(25),
            output: Some(1),
            output_final: false
        }));
        assert!(p.contains(&Piece::Usage {
            input: None,
            output: Some(15),
            output_final: true
        }));
        assert_eq!(p.last(), Some(&Piece::Done));
    }

    #[test]
    fn anthropic_tool_use_assembles_partial_json() {
        let p = run(
            ProviderKind::Anthropic,
            &[
                r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
                r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Checking."}}"#,
                r#"{"type":"content_block_stop","index":0}"#,
                r#"{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"toolu_01A","name":"suggest_break","input":{}}}"#,
                r#"{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":""}}"#,
                r#"{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"available_"}}"#,
                r#"{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"minutes\": 5}"}}"#,
                r#"{"type":"content_block_stop","index":1}"#,
                r#"{"type":"message_stop"}"#,
            ],
        )
        .unwrap();
        assert_eq!(text(&p), "Checking.");
        assert!(p.contains(&Piece::Tool(ToolCall {
            id: "toolu_01A".into(),
            name: "suggest_break".into(),
            arguments: "{\"available_minutes\": 5}".into(),
        })));
    }

    #[test]
    fn duplicate_call_ids_in_one_reply_are_malformed() {
        let e = run(
            ProviderKind::Anthropic,
            &[
                r#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"dup","name":"suggest_break","input":{}}}"#,
                r#"{"type":"content_block_stop","index":0}"#,
                r#"{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"dup","name":"suggest_break","input":{}}}"#,
                r#"{"type":"content_block_stop","index":1}"#,
            ],
        )
        .unwrap_err();
        assert_eq!(e.kind, ErrorKind::InvalidResponse);
        let call = |i: u32, id: &str| {
            format!(
                r#"{{"choices":[{{"index":0,"delta":{{"tool_calls":[{{"index":{i},"id":"{id}","type":"function","function":{{"name":"suggest_break","arguments":"{{}}"}}}}]}},"finish_reason":null}}]}}"#
            )
        };
        let fin = r#"{"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}"#;
        let (a, b) = (call(0, "same"), call(1, "same"));
        let e = run(
            ProviderKind::OpenAiCompatible,
            &[a.as_str(), b.as_str(), fin],
        )
        .unwrap_err();
        assert_eq!(e.kind, ErrorKind::InvalidResponse);
        // Different ids are fine.
        let (c, d) = (call(0, "one"), call(1, "two"));
        let ok = run(
            ProviderKind::OpenAiCompatible,
            &[c.as_str(), d.as_str(), fin],
        )
        .unwrap();
        assert_eq!(ok.iter().filter(|p| matches!(p, Piece::Tool(_))).count(), 2);
    }

    #[test]
    fn anthropic_errors_and_garbage() {
        let e = run(
            ProviderKind::Anthropic,
            &[r#"{"type":"error","error":{"type":"overloaded_error","message":"SECRET-BODY"}}"#],
        )
        .unwrap_err();
        assert_eq!(e.kind, ErrorKind::ProviderError);
        assert!(!e.message.contains("SECRET-BODY"));
        assert_eq!(
            run(ProviderKind::Anthropic, &["not json"])
                .unwrap_err()
                .kind,
            ErrorKind::InvalidResponse
        );
        // A tool call with an unsafe id is refused.
        let e = run(
            ProviderKind::Anthropic,
            &[
                r#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"a b","name":"x"}}"#,
                r#"{"type":"content_block_stop","index":0}"#,
            ],
        )
        .unwrap_err();
        assert_eq!(e.kind, ErrorKind::InvalidResponse);
    }

    #[test]
    fn openai_text_usage_and_done() {
        let p = run(
            ProviderKind::OpenAiCompatible,
            &[
                r#"{"choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null}]}"#,
                r#"{"choices":[{"index":0,"delta":{"content":"Hi"},"finish_reason":null}]}"#,
                r#"{"choices":[{"index":0,"delta":{"content":" there"},"finish_reason":null}]}"#,
                r#"{"choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":null}"#,
                r#"{"choices":[],"usage":{"prompt_tokens":9,"completion_tokens":3,"total_tokens":12}}"#,
                "[DONE]",
            ],
        )
        .unwrap();
        assert_eq!(text(&p), "Hi there");
        assert!(p.contains(&Piece::Usage {
            input: Some(9),
            output: Some(3),
            output_final: true
        }));
        assert_eq!(p.last(), Some(&Piece::Done));
    }

    #[test]
    fn openai_tool_calls_arrive_in_pieces() {
        let p = run(
            ProviderKind::OpenAiCompatible,
            &[
                r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_abc","type":"function","function":{"name":"activity_help","arguments":""}}]}}]}"#,
                r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"activity_"}}]}}]}"#,
                r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"id\":\"x.y\"}"}}]}}]}"#,
                r#"{"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#,
                "[DONE]",
            ],
        )
        .unwrap();
        assert!(p.contains(&Piece::Tool(ToolCall {
            id: "call_abc".into(),
            name: "activity_help".into(),
            arguments: "{\"activity_id\":\"x.y\"}".into(),
        })));
    }

    #[test]
    fn openai_in_stream_error_and_limits() {
        let e = run(
            ProviderKind::OpenAiCompatible,
            &[r#"{"error":{"message":"SECRET-BODY"}}"#],
        )
        .unwrap_err();
        assert_eq!(e.kind, ErrorKind::ProviderError);
        assert!(!e.message.contains("SECRET-BODY"));
        let big = "a".repeat(MAX_ARGUMENT_BYTES + 1);
        let ev = format!(
            r#"{{"choices":[{{"delta":{{"tool_calls":[{{"index":0,"id":"c","function":{{"name":"n","arguments":"{big}"}}}}]}}}}]}}"#
        );
        assert_eq!(
            run(ProviderKind::OpenAiCompatible, &[&ev])
                .unwrap_err()
                .kind,
            ErrorKind::InvalidResponse
        );
    }

    #[test]
    fn completion_is_tracked() {
        let mut d = Decoder::new(ProviderKind::OpenAiCompatible);
        assert!(!d.complete());
        d.push(&ev(r#"{"choices":[{"delta":{"content":"x"}}]}"#))
            .unwrap();
        assert!(!d.complete());
        d.push(&ev(r#"{"choices":[{"delta":{},"finish_reason":"stop"}]}"#))
            .unwrap();
        assert!(d.complete());
    }

    // Recorded-shape fixtures for the Responses API (event names and fields as in the OpenAI
    // API reference, checked 2026-11-07).
    const R_TEXT: [&str; 8] = [
        r#"{"type":"response.created","response":{"id":"resp_1","status":"in_progress"}}"#,
        r#"{"type":"response.in_progress","response":{"id":"resp_1"}}"#,
        r#"{"type":"response.output_item.added","output_index":0,"item":{"id":"msg_1","type":"message","status":"in_progress","role":"assistant","content":[]}}"#,
        r#"{"type":"response.content_part.added","item_id":"msg_1","output_index":0,"content_index":0,"part":{"type":"output_text","text":""}}"#,
        r#"{"type":"response.output_text.delta","item_id":"msg_1","output_index":0,"content_index":0,"delta":"Hel"}"#,
        r#"{"type":"response.output_text.delta","item_id":"msg_1","output_index":0,"content_index":0,"delta":"lo"}"#,
        r#"{"type":"response.output_text.done","item_id":"msg_1","output_index":0,"content_index":0,"text":"Hello"}"#,
        r#"{"type":"response.completed","response":{"id":"resp_1","status":"completed","usage":{"input_tokens":21,"input_tokens_details":{"cached_tokens":0},"output_tokens":5,"output_tokens_details":{"reasoning_tokens":0},"total_tokens":26}}}"#,
    ];

    #[test]
    fn responses_text_and_usage() {
        let p = run(ProviderKind::OpenAi, &R_TEXT).unwrap();
        assert_eq!(text(&p), "Hello");
        assert!(p.contains(&Piece::Usage {
            input: Some(21),
            output: Some(5),
            output_final: true
        }));
        assert_eq!(p.last(), Some(&Piece::Done));
    }

    #[test]
    fn responses_function_call_arrives_in_pieces() {
        let p = run(
            ProviderKind::OpenAi,
            &[
                r#"{"type":"response.output_item.added","output_index":0,"item":{"id":"fc_1","type":"function_call","status":"in_progress","call_id":"call_abc","name":"suggest_break","arguments":""}}"#,
                r#"{"type":"response.function_call_arguments.delta","item_id":"fc_1","output_index":0,"delta":"{\"available_"}"#,
                r#"{"type":"response.function_call_arguments.delta","item_id":"fc_1","output_index":0,"delta":"minutes\":5}"}"#,
                r#"{"type":"response.function_call_arguments.done","item_id":"fc_1","output_index":0,"arguments":"{\"available_minutes\":5}"}"#,
                r#"{"type":"response.output_item.done","output_index":0,"item":{"id":"fc_1","type":"function_call","status":"completed","call_id":"call_abc","name":"suggest_break","arguments":"{\"available_minutes\":5}"}}"#,
                r#"{"type":"response.completed","response":{"usage":{"input_tokens":30,"output_tokens":9}}}"#,
            ],
        )
        .unwrap();
        let calls: Vec<&ToolCall> = p
            .iter()
            .filter_map(|x| match x {
                Piece::Tool(c) => Some(c),
                _ => None,
            })
            .collect();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_abc");
        assert_eq!(calls[0].name, "suggest_break");
        assert_eq!(calls[0].arguments, "{\"available_minutes\":5}");
        assert_eq!(p.last(), Some(&Piece::Done));
    }

    #[test]
    fn responses_call_without_done_item_is_flushed_on_completion_and_duplicates_refused() {
        let added = r#"{"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","call_id":"call_1","name":"suggest_break","arguments":"{}"}}"#;
        let completed = r#"{"type":"response.completed","response":{}}"#;
        let p = run(ProviderKind::OpenAi, &[added, completed]).unwrap();
        assert!(p.iter().any(|x| matches!(x, Piece::Tool(_))));
        // Completed without usage: not final, so the caller estimates.
        assert!(p.contains(&Piece::Usage {
            input: None,
            output: None,
            output_final: false
        }));
        let second = added.replace("\"output_index\":0", "\"output_index\":1");
        let e = run(ProviderKind::OpenAi, &[added, &second, completed]).unwrap_err();
        assert_eq!(e.kind, ErrorKind::InvalidResponse);
    }

    #[test]
    fn responses_failed_error_and_incomplete() {
        let e = run(
            ProviderKind::OpenAi,
            &[r#"{"type":"response.failed","response":{"status":"failed","error":{"code":"server_error","message":"The model failed to generate a response."}}}"#],
        )
        .unwrap_err();
        assert_eq!(e.kind, ErrorKind::ProviderError);
        assert_eq!(e.message, "The provider reported a problem while replying.");
        let r = e.reason.expect("the reason travels apart from the message");
        assert_eq!(
            (r.speaker, r.text.as_str()),
            ("OpenAI", "The model failed to generate a response.")
        );
        let e = run(
            ProviderKind::OpenAi,
            &["{\"type\":\"error\",\"code\":\"ERR\",\"message\":\"bad\\u202e sk-proj-abcdefgh12345678\",\"param\":null}"],
        )
        .unwrap_err();
        let r = e.reason.expect("reason");
        assert!(!r.text.contains("sk-proj") && !r.text.contains('\u{202e}'));
        assert!(!e.message.contains("sk-proj"));
        // Incomplete with some text is a (cut) reply; without any output it is an error.
        let p = run(
            ProviderKind::OpenAi,
            &[
                r#"{"type":"response.output_text.delta","delta":"partial"}"#,
                r#"{"type":"response.incomplete","response":{"status":"incomplete","incomplete_details":{"reason":"max_output_tokens"},"usage":{"input_tokens":3,"output_tokens":1024}}}"#,
            ],
        )
        .unwrap();
        assert_eq!(text(&p), "partial");
        assert_eq!(p.last(), Some(&Piece::Done));
        let e = run(
            ProviderKind::OpenAi,
            &[r#"{"type":"response.incomplete","response":{"status":"incomplete"}}"#],
        )
        .unwrap_err();
        assert_eq!(e.kind, ErrorKind::ProviderError);
    }

    #[test]
    fn responses_ignore_unknown_events_and_refuse_garbage() {
        let p = run(
            ProviderKind::OpenAi,
            &[
                r#"{"type":"response.reasoning_summary_text.delta","delta":"thinking"}"#,
                r#"{"type":"response.something_new","x":1}"#,
                r#"{"type":"response.output_text.delta","delta":"ok"}"#,
                r#"{"type":"response.completed","response":{}}"#,
            ],
        )
        .unwrap();
        assert_eq!(text(&p), "ok");
        assert_eq!(
            run(ProviderKind::OpenAi, &["not json"]).unwrap_err().kind,
            ErrorKind::InvalidResponse
        );
        assert_eq!(
            run(
                ProviderKind::OpenAi,
                &[r#"{"type":"response.output_text.delta","output_index":99,"delta":"x"}"#]
            )
            .unwrap_err()
            .kind,
            ErrorKind::InvalidResponse
        );
    }

    #[test]
    fn responses_body_round_trip_with_a_tool_call() {
        let b = br#"{"id":"resp_9","status":"completed","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Let me check."}]},{"type":"function_call","id":"fc_9","call_id":"call_9","name":"suggest_break","arguments":"{\"available_minutes\":5}","status":"completed"}],"usage":{"input_tokens":40,"output_tokens":12}}"#;
        let r = parse_responses_body(b, "", &Default::default(), true).unwrap();
        assert_eq!(r.text, "Let me check.");
        assert_eq!(r.calls.len(), 1);
        assert_eq!(r.calls[0].id, "call_9");
        assert_eq!((r.input, r.output), (Some(40), Some(12)));
    }
}
