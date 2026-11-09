//! The generation loop: budget check, one provider call, reply checks, and at most one retry.

use std::sync::Arc;

use serde_json::Value;

use super::reply::{Problem, check_reply};
use super::{
    CallUsage, CheckedRequest, GenerateError, GenerateErrorKind, GenerationRequest, clean_line,
    generation_http_config, plan,
};
use crate::chat_stream::chat_stream;
use crate::error::CompanionError;
use crate::http::{CancelToken, HttpConfig, LateUsage};
use crate::provider::{ChatMessage, ProviderSpec, Role};
use crate::secret::Secret;

/// What the host supplies around a generation call.
pub struct Hooks<'a> {
    /// Stops the request. Checked before each provider call and while one is in flight.
    pub cancel: &'a CancelToken,
    /// Called before each provider call (the first and the retry) with the usage of the calls
    /// already made, which the caller may not have booked yet. `Err` refuses the call.
    pub budget_check: &'a mut dyn FnMut(&[CallUsage]) -> Result<(), CompanionError>,
    /// The host's engine-schema gate and pack text checks, run on the data after the host
    /// fields were added. Only pass or fail is used; the messages are dropped.
    pub engine_check: &'a dyn Fn(&Value) -> Result<(), Vec<String>>,
    /// Receives the real cost of a call that was abandoned (cancelled or timed out) and
    /// finished later, on another thread. The caller books it.
    pub late_usage: Arc<dyn Fn(LateUsage) + Send + Sync>,
}

/// A finished generation.
#[derive(Debug, Clone, PartialEq)]
pub struct GenerateOutput {
    /// The checked data, host fields included, ready to store and to hand to the engine.
    pub data: Value,
    /// The model name sent in the request, cleaned.
    pub model: String,
    /// One entry per provider call made, in order.
    pub usage: Vec<CallUsage>,
}

/// A failed generation. `usage` lists every call made, to book against the budget.
#[derive(Debug, Clone, PartialEq)]
pub struct GenerateFailure {
    pub error: GenerateError,
    pub usage: Vec<CallUsage>,
}

/// One provider answer, as the loop sees it.
pub(super) struct Answer {
    pub text: String,
    pub cut: bool,
    pub usage: CallUsage,
}

/// A provider call that failed, with what to book for it.
pub(super) struct CallFailure {
    pub error: CompanionError,
    pub usage: CallUsage,
}

/// The retry sentence: the kind of problem only.
fn follow_up(problem: Problem) -> String {
    format!(
        "Your last reply did not fit. {} Reply again with only the corrected JSON object.",
        problem.sentence()
    )
}

/// The loop, with the transport injected so it can be tested without a network.
pub(super) fn run_loop(
    req: &CheckedRequest,
    hooks: &mut Hooks<'_>,
    send: &mut dyn FnMut(&[ChatMessage]) -> Result<Answer, CallFailure>,
) -> (Result<Value, GenerateError>, Vec<CallUsage>) {
    let mut usage = Vec::new();
    let mut messages = req.first_messages();
    for attempt in 0..2 {
        if hooks.cancel.is_cancelled() {
            return (Err(GenerateError::cancelled()), usage);
        }
        if let Err(e) = (hooks.budget_check)(&usage) {
            let mut err = GenerateError::from_companion(e);
            err.kind = GenerateErrorKind::BudgetRefused;
            return (Err(err), usage);
        }
        let answer = match send(&messages) {
            Ok(a) => a,
            Err(f) => {
                usage.push(f.usage);
                return (Err(GenerateError::from_companion(f.error)), usage);
            }
        };
        usage.push(answer.usage);
        if hooks.cancel.is_cancelled() {
            return (Err(GenerateError::cancelled()), usage);
        }
        match check_reply(req, &answer.text, answer.cut, hooks.engine_check) {
            Ok(data) => return (Ok(data), usage),
            Err(problem) if attempt == 0 => {
                // The reply text is dropped here and never sent back or kept.
                messages.push(ChatMessage {
                    role: Role::User,
                    content: follow_up(problem),
                });
            }
            Err(_) => break,
        }
    }
    (Err(GenerateError::did_not_fit()), usage)
}

/// Generates one set with the standard limits ([`generation_http_config`]).
pub fn generate(
    spec: &ProviderSpec,
    key: &Secret,
    req: &GenerationRequest,
    hooks: &mut Hooks<'_>,
) -> Result<GenerateOutput, GenerateFailure> {
    generate_with_http(&generation_http_config(), spec, key, req, hooks)
}

/// [`generate`] with explicit HTTP limits. The host uses [`generate`]; this exists so tests can
/// shorten the timeouts.
pub fn generate_with_http(
    cfg: &HttpConfig,
    spec: &ProviderSpec,
    key: &Secret,
    req: &GenerationRequest,
    hooks: &mut Hooks<'_>,
) -> Result<GenerateOutput, GenerateFailure> {
    let checked = req.validate().map_err(|error| GenerateFailure {
        error,
        usage: Vec::new(),
    })?;
    let system = checked.engine.instruction();
    let late = Arc::clone(&hooks.late_usage);
    let cancel = hooks.cancel.clone();
    let mut send = |messages: &[ChatMessage]| -> Result<Answer, CallFailure> {
        let late = Arc::clone(&late);
        // Streamed text is not shown to anyone, and any reasoning summary is dropped unread.
        match chat_stream(
            cfg,
            spec,
            key,
            &plan(messages, &system),
            &cancel,
            |_| {},
            |_| {},
            move |u| late(u),
        ) {
            Ok(r) => Ok(Answer {
                text: r.text,
                cut: r.cut,
                usage: CallUsage {
                    input_tokens: r.input_tokens,
                    output_tokens: r.output_tokens,
                    reported: r.reported,
                },
            }),
            Err(f) => Err(CallFailure {
                error: f.error.scrubbed(key.expose()),
                usage: CallUsage {
                    input_tokens: f.input_tokens,
                    output_tokens: f.output_tokens,
                    reported: false,
                },
            }),
        }
    };
    let (result, usage) = run_loop(&checked, hooks, &mut send);
    match result {
        Ok(data) => Ok(GenerateOutput {
            data,
            model: clean_line(&spec.model),
            usage,
        }),
        Err(error) => Err(GenerateFailure { error, usage }),
    }
}
