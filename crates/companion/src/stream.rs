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
    finished: bool,
    done: bool,
}

impl Decoder {
    pub fn new(kind: ProviderKind) -> Self {
        Self {
            kind,
            blocks: BTreeMap::new(),
            calls: BTreeMap::new(),
            emitted_calls: 0,
            finished: false,
            done: false,
        }
    }

    /// True once a complete reply was seen.
    pub fn complete(&self) -> bool {
        self.done || self.finished
    }

    pub fn push(&mut self, ev: &SseEvent) -> Result<Vec<Piece>, CompanionError> {
        match self.kind {
            ProviderKind::Anthropic => self.anthropic(ev),
            ProviderKind::OpenAi | ProviderKind::OpenAiCompatible => self.openai(ev),
        }
    }

    fn finish_call(&mut self, c: PartialCall) -> Result<Piece, CompanionError> {
        self.emitted_calls += 1;
        if self.emitted_calls > MAX_CALLS_PER_REPLY
            || !valid_call_id(&c.id)
            || c.name.is_empty()
            || c.name.len() > 64
            || c.args.len() > MAX_ARGUMENT_BYTES
        {
            return Err(bad());
        }
        Ok(Piece::Tool(ToolCall {
            id: c.id,
            name: c.name,
            arguments: c.args,
        }))
    }

    fn anthropic(&mut self, ev: &SseEvent) -> Result<Vec<Piece>, CompanionError> {
        let v: Value = serde_json::from_str(&ev.data).map_err(|_| bad())?;
        let kind = v.get("type").and_then(Value::as_str).unwrap_or("");
        let mut out = Vec::new();
        match kind {
            "message_start" => {
                let u = v.pointer("/message/usage");
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
                    out.push(self.finish_call(c)?);
                }
            }
            "message_delta" => {
                let u = v.get("usage");
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
        calls
            .into_values()
            .map(|c| self.finish_call(c))
            .collect::<Result<_, _>>()
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
            ProviderKind::OpenAi,
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
            ProviderKind::OpenAi,
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
            run(ProviderKind::OpenAi, &[&ev]).unwrap_err().kind,
            ErrorKind::InvalidResponse
        );
    }

    #[test]
    fn completion_is_tracked() {
        let mut d = Decoder::new(ProviderKind::OpenAi);
        assert!(!d.complete());
        d.push(&ev(r#"{"choices":[{"delta":{"content":"x"}}]}"#))
            .unwrap();
        assert!(!d.complete());
        d.push(&ev(r#"{"choices":[{"delta":{},"finish_reason":"stop"}]}"#))
            .unwrap();
        assert!(d.complete());
    }
}
