//! Companion commands (Sprint 09, ADR-020). The contract with the UI is
//! `docs/COMPANION_COMMANDS.md`.
//!
//! Rules kept here:
//!
//! - Keys go from `companion_set_key` straight into the OS keychain. No command returns one, no
//!   struct that is serialised holds one, and nothing in this module logs.
//! - The store lock is never held across a network call.
//! - The attached context is built by one function (`attached_text`) used by both the preview and
//!   the send, so they cannot differ.
//! - Errors reach the UI as `{ kind, message }` with fixed, friendly text.
//! - Companion has no path to agent IPC, adapters or marketplaces: it only reads the session the
//!   user picked, through the store.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ::companion::models_dev::{self, CatalogCache, CatalogModel};
use ::companion::provider::{MAX_OUTPUT_TOKENS, TEST_OUTPUT_TOKENS};
use ::companion::scope::{ItemFacts, Selection, SessionFacts, compose_user_content, trim_history};
use ::companion::tools::{
    DECLINED_TEXT, LIMIT_TEXT, MAX_TOOL_CALLS, MAX_TOOL_ROUNDS, Rejection, ToolName,
    UNAVAILABLE_TEXT, check_request, clean_result, safe_tool_name,
};
use ::companion::url_check::check_base_url;
use ::companion::{
    CancelToken, ChatMessage, CompanionError, HttpConfig, KeyStore, KeyringStore, ProviderKind,
    ProviderSpec, RequestPlan, Role, Secret, ToolExchange, budget, chat, chat_stream,
};
use local_store::{
    CompanionMessage, CompanionModel, CompanionProfile, CompanionThread, CompanionToolCall,
    CompanionToolNotice, LAST_MODEL_REFUSED, NewToolCall, NewToolNotice, NewUserMessage, Store,
    StoreError, month_of,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use zeroize::Zeroize;

use crate::Core;

mod lookups;
pub mod open_link;

/// Most items one scope may pick.
const MAX_SCOPE_ITEMS: usize = 200;

/// Preference key for Companion's own settings (the lookups switch and the last context choice).
/// Neither is secret; keys never go here.
pub const COMPANION_PREFS_KEY: &str = "companion";

/// How long a lookup waits for the user's decision before the whole request is cancelled.
const APPROVAL_WAIT: Duration = Duration::from_secs(10 * 60);

// ---- state -------------------------------------------------------------------------------

/// Usage of a call that was cancelled while its worker was still on the wire. It is queued by the
/// worker (which has no store access) and written to the store by the next Companion command.
struct QueuedUsage {
    profile_id: String,
    at: OffsetDateTime,
    input_tokens: i64,
    output_tokens: i64,
}

/// The request currently using a thread. Cancelling removes it at once, so the thread is free
/// while the old worker may still be winding down; the request number tells a late cleanup from
/// the old request apart from a newer request on the same thread.
struct Active {
    seq: u64,
    request_id: String,
    token: CancelToken,
}

type BusyMap = Arc<Mutex<HashMap<i64, Active>>>;
type ApprovalMap = Arc<Mutex<HashMap<String, Approval>>>;

/// A lookup waiting for the user's decision, by request id. `approval_id` is made here for each
/// card and sent only to the UI; provider call ids are never used to match an answer, so a stale
/// or repeated Approve cannot resolve another card.
struct Approval {
    approval_id: String,
    tx: mpsc::Sender<bool>,
}

pub struct CompanionState {
    pub(crate) keys: Arc<dyn KeyStore>,
    pub(crate) http: HttpConfig,
    active: BusyMap,
    next_seq: AtomicU64,
    approvals: ApprovalMap,
    late_usage: Arc<Mutex<Vec<QueuedUsage>>>,
    /// The models.dev model list, in memory for this app run only (decision 32).
    pub(crate) catalog: CatalogCache,
}

impl CompanionState {
    /// The real OS keychain and the production HTTP limits.
    pub fn new() -> Self {
        Self::with_parts(Arc::new(KeyringStore), HttpConfig::default())
    }

    fn with_parts(keys: Arc<dyn KeyStore>, http: HttpConfig) -> Self {
        Self {
            keys,
            http,
            active: Arc::new(Mutex::new(HashMap::new())),
            next_seq: AtomicU64::new(1),
            approvals: Arc::new(Mutex::new(HashMap::new())),
            late_usage: Arc::new(Mutex::new(Vec::new())),
            catalog: CatalogCache::new(),
        }
    }

    #[cfg(test)]
    pub fn with(keys: Arc<dyn KeyStore>, http: HttpConfig) -> Self {
        Self::with_parts(keys, http)
    }

    /// Marks `thread` as busy with `request_id`, or says it already is. The mark is removed when
    /// the guard is dropped or the request is cancelled, whichever comes first.
    fn begin(&self, thread: i64, request_id: &str) -> Res<SendGuard> {
        let mut m = self.active.lock().map_err(|_| CommandError::internal())?;
        // A request id must be unique among live requests, so an answer to a lookup can only
        // ever reach the request it was meant for.
        if m.contains_key(&thread) || m.values().any(|a| a.request_id == request_id) {
            return Err(CommandError::from(CompanionError::busy()));
        }
        let token = CancelToken::new();
        let seq = self.next_seq.fetch_add(1, Ordering::SeqCst);
        m.insert(
            thread,
            Active {
                seq,
                request_id: request_id.to_owned(),
                token: token.clone(),
            },
        );
        Ok(SendGuard {
            map: Arc::clone(&self.active),
            approvals: Arc::clone(&self.approvals),
            thread,
            seq,
            request_id: request_id.to_owned(),
            token,
        })
    }

    /// Cancels the request on `thread` and frees the thread at once (S12-T15). Whatever the old
    /// worker still delivers is discarded by the request itself, which checks its token.
    fn cancel(&self, thread: i64) {
        // The token is cancelled while the busy map is still locked: a new request on this
        // thread can only begin after that, so the old worker's store check (below, in
        // `deliver`) always sees the cancel before the new user message can exist.
        let removed = match self.active.lock() {
            Ok(mut m) => {
                let a = m.remove(&thread);
                if let Some(a) = &a {
                    a.token.cancel();
                }
                a
            }
            Err(_) => None,
        };
        if let Some(a) = removed
            && let Ok(mut ap) = self.approvals.lock()
        {
            ap.remove(&a.request_id);
        }
    }

    /// Records the user's answer to a waiting lookup. False if nothing matches (already answered,
    /// cancelled, or another call).
    fn decide(&self, request_id: &str, approval_id: &str, approve: bool) -> bool {
        let Ok(mut ap) = self.approvals.lock() else {
            return false;
        };
        match ap.get(request_id) {
            Some(a) if a.approval_id == approval_id => {}
            _ => return false,
        }
        ap.remove(request_id)
            .is_some_and(|a| a.tx.send(approve).is_ok())
    }
}

impl Default for CompanionState {
    fn default() -> Self {
        Self::new()
    }
}

/// Held by a request for as long as it owns its thread.
struct SendGuard {
    map: BusyMap,
    approvals: ApprovalMap,
    thread: i64,
    seq: u64,
    request_id: String,
    token: CancelToken,
}

impl SendGuard {
    fn token(&self) -> &CancelToken {
        &self.token
    }
}

impl Drop for SendGuard {
    fn drop(&mut self) {
        if let Ok(mut m) = self.map.lock()
            && m.get(&self.thread).is_some_and(|a| a.seq == self.seq)
        {
            m.remove(&self.thread);
        }
        if let Ok(mut ap) = self.approvals.lock() {
            ap.remove(&self.request_id);
        }
    }
}

/// Writes queued usage of cancelled calls to the store. Called before any budget check and
/// before usage is read.
fn flush_late_usage(core: &Core) {
    let items = match core.companion.late_usage.lock() {
        Ok(mut q) => std::mem::take(&mut *q),
        Err(_) => return,
    };
    for u in items {
        let _ = with_store(core, |s| {
            s.companion_record_usage(&u.profile_id, u.at, u.input_tokens, u.output_tokens)
        });
    }
}

// ---- errors ------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommandError {
    pub kind: &'static str,
    pub message: String,
}

impl CommandError {
    fn internal() -> Self {
        CompanionError::internal().into()
    }

    fn invalid(message: &str) -> Self {
        CompanionError::invalid_input(message.to_owned()).into()
    }
}

impl From<CompanionError> for CommandError {
    fn from(e: CompanionError) -> Self {
        Self {
            kind: e.kind.as_str(),
            message: e.message,
        }
    }
}

impl From<StoreError> for CommandError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::NotFound => CompanionError::not_found("That item no longer exists.").into(),
            StoreError::InvalidInput(_) => CommandError::invalid("That value isn't allowed."),
            StoreError::TooMany { limit } => {
                CommandError::invalid(&format!("The limit of {limit} has been reached."))
            }
            // Database errors can carry SQL text; they never reach the UI.
            _ => CommandError::internal(),
        }
    }
}

type Res<T> = Result<T, CommandError>;

// ---- DTOs --------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderProfileDto {
    id: String,
    kind: String,
    label: String,
    base_url: String,
    host: String,
    /// The provider's default model.
    model: String,
    /// Every model of this provider, default first (decision 36).
    models: Vec<ModelDto>,
    monthly_token_budget: Option<i64>,
    is_default: bool,
    has_key: bool,
    keychain_ok: bool,
}

/// One model a provider offers. `label` is an optional display name.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDto {
    model: String,
    label: Option<String>,
    is_default: bool,
}

impl From<CompanionModel> for ModelDto {
    fn from(m: CompanionModel) -> Self {
        Self {
            model: m.model,
            label: m.label,
            is_default: m.is_default,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadDto {
    id: i64,
    title: String,
    profile_id: Option<String>,
    model: Option<String>,
    created_at: String,
    updated_at: String,
}

impl From<CompanionThread> for ThreadDto {
    fn from(t: CompanionThread) -> Self {
        Self {
            id: t.id,
            title: t.title,
            profile_id: t.profile_id,
            model: t.model,
            created_at: t.created_utc,
            updated_at: t.updated_utc,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MessageErrorDto {
    kind: String,
    message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageDto {
    id: i64,
    thread_id: i64,
    role: String,
    text: String,
    attached_context: Option<String>,
    scope_kind: String,
    host: Option<String>,
    /// The provider this message went to (yours) or came from (replies). Empty for rows whose
    /// provider was deleted.
    profile_id: Option<String>,
    /// The model it was sent to or answered by. Empty for rows stored without one.
    model: Option<String>,
    status: String,
    error: Option<MessageErrorDto>,
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    created_at: String,
    /// Lookups made while answering this (user) message, oldest first.
    tool_calls: Vec<ToolCallDto>,
    /// Calls the model made that Lullshift did not run, oldest first.
    tool_notices: Vec<ToolNoticeDto>,
}

/// A call that was not run: why, and the tool name only if it was a short safe identifier.
/// Never the model's arguments (owner decision 27.6).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolNoticeDto {
    id: i64,
    round: i64,
    /// `unknown`, `malformed` or `limit`.
    reason: String,
    tool: Option<String>,
    created_at: String,
}

impl From<CompanionToolNotice> for ToolNoticeDto {
    fn from(n: CompanionToolNotice) -> Self {
        Self {
            id: n.id,
            round: n.round,
            reason: n.reason,
            tool: n.tool,
            created_at: n.created_utc,
        }
    }
}

/// A suggested activity the user can click to start. The lookup never starts it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardDto {
    activity_id: String,
    name: String,
    minutes: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallDto {
    id: i64,
    round: i64,
    tool: String,
    label: String,
    arguments: String,
    /// Exactly what was sent to the provider.
    result_text: String,
    /// `approved` or `declined`.
    status: String,
    card: Option<CardDto>,
    host: Option<String>,
    created_at: String,
}

impl From<CompanionToolCall> for ToolCallDto {
    fn from(c: CompanionToolCall) -> Self {
        let label = ToolName::parse(&c.tool).map_or("Lookup", ToolName::label);
        let card = match (c.card_activity_id, c.card_name, c.card_minutes) {
            (Some(activity_id), Some(name), Some(m)) => Some(CardDto {
                activity_id,
                name,
                minutes: u32::try_from(m).unwrap_or(0),
            }),
            _ => None,
        };
        Self {
            id: c.id,
            round: c.round,
            tool: c.tool,
            label: label.to_owned(),
            arguments: c.arguments,
            result_text: c.result_text,
            status: c.status,
            card,
            host: c.host,
            created_at: c.created_utc,
        }
    }
}

/// Settings Companion keeps for itself.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompanionPrefs {
    /// "Let the Companion look things up". Off unless the user turns it on.
    #[serde(default)]
    pub tools_enabled: bool,
    /// The context choice of the last message sent, used to start the next composer. The preview
    /// is still shown and required before every send.
    #[serde(default)]
    pub last_scope: Option<Scope>,
}

/// What a streaming reply tells the UI. Every event carries the request id it belongs to; the UI
/// ignores events for any other id. Text is already cleaned.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum StreamEvent {
    Started {
        request_id: String,
    },
    /// A new provider call begins (after a lookup); earlier streamed text belongs to the call
    /// before.
    Round {
        request_id: String,
        round: u32,
    },
    Delta {
        request_id: String,
        text: String,
    },
    /// The model asked for a lookup. `result_text` is exactly what would be sent. Nothing is sent
    /// until the UI answers with `companion_tool_decision`.
    ToolRequest {
        request_id: String,
        approval_id: String,
        tool: String,
        label: String,
        arguments: String,
        result_text: String,
        host: String,
    },
    /// A lookup that was not offered or went over the limit got a fixed sentence and no data.
    ToolSkipped {
        request_id: String,
        tool: String,
        text: String,
    },
    ToolResolved {
        request_id: String,
        approval_id: String,
        approved: bool,
    },
    /// The model asked for a lookup that was not run (unknown, malformed or over the limit). The
    /// UI shows a small notice with no button. Carries only the reason (`unknown`, `malformed`,
    /// `limit`) and the tool name if it is a short safe identifier; never the model's arguments.
    ToolNotice {
        request_id: String,
        reason: String,
        tool: Option<String>,
    },
}

impl From<CompanionMessage> for MessageDto {
    fn from(m: CompanionMessage) -> Self {
        let error = match (m.error_kind, m.error_message) {
            (Some(kind), Some(message)) => Some(MessageErrorDto { kind, message }),
            (Some(kind), None) => Some(MessageErrorDto {
                kind,
                message: String::new(),
            }),
            _ => None,
        };
        Self {
            id: m.id,
            thread_id: m.thread_id,
            role: m.role,
            text: m.text,
            attached_context: m.attached_context,
            scope_kind: m.scope_kind,
            host: m.host,
            profile_id: m.profile_id,
            model: m.model,
            status: m.status,
            error,
            input_tokens: m.input_tokens,
            output_tokens: m.output_tokens,
            created_at: m.created_utc,
            tool_calls: Vec::new(),
            tool_notices: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageDto {
    profile_id: String,
    month: String,
    input_tokens: i64,
    output_tokens: i64,
    budget: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ContextPreviewDto {
    scope: Scope,
    text: String,
    chars: usize,
    /// Lowercase hex SHA-256 of `text`, to pass back to `companion_send` as
    /// `expectedContextSha256`. Null for scope `none`.
    sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TestResultDto {
    ok: bool,
    model: String,
}

/// What `companion_model_catalog` should do. Only `load` and `refresh` may use the network.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CatalogMode {
    /// Memory only: the list if it was loaded in this app run, otherwise nothing.
    Cached,
    /// The list from memory, or one request to models.dev if it isn't loaded yet.
    Load,
    /// One request to models.dev with the stored ETag; a 304 keeps the list.
    Refresh,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogProviderDto {
    id: String,
    name: String,
    model_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogDto {
    /// When models.dev sent the list in memory (RFC 3339, UTC).
    loaded_at: String,
    /// When models.dev last sent or confirmed it (a 304 moves only this one).
    checked_at: String,
    /// This call's refresh got a 304: the list in memory is current.
    unchanged: bool,
    /// Nothing was sent: the list was already in memory.
    from_memory: bool,
    providers: Vec<CatalogProviderDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogModelDto {
    id: String,
    /// `None` when models.dev gives no name; the UI shows the id.
    name: Option<String>,
    family: Option<String>,
    tool_call: bool,
    reasoning: bool,
    release_date: Option<String>,
    context_tokens: Option<u64>,
    output_tokens: Option<u64>,
    /// USD per million input tokens.
    cost_input: Option<f64>,
    /// USD per million output tokens.
    cost_output: Option<f64>,
}

impl From<&CatalogModel> for CatalogModelDto {
    fn from(m: &CatalogModel) -> Self {
        Self {
            id: m.id.clone(),
            name: m.name.clone(),
            family: m.family.clone(),
            tool_call: m.tool_call,
            reasoning: m.reasoning,
            release_date: m.release_date.clone(),
            context_tokens: m.context_tokens,
            output_tokens: m.output_tokens,
            cost_input: m.cost_input,
            cost_output: m.cost_output,
        }
    }
}

/// Which Context Keeper data goes with a message. `none` is the default; nothing is attached
/// unless the user picks one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Scope {
    None,
    SessionSummary {
        provider: String,
        session_id: String,
    },
    SelectedNotes {
        provider: String,
        session_id: String,
        notes: bool,
        item_ids: Vec<i64>,
    },
}

impl Scope {
    fn kind_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::SessionSummary { .. } => "sessionSummary",
            Self::SelectedNotes { .. } => "selectedNotes",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfileInput {
    id: Option<String>,
    kind: String,
    label: String,
    base_url: Option<String>,
    model: String,
    monthly_token_budget: Option<i64>,
    #[serde(default)]
    is_default: bool,
}

// ---- profiles and keys -------------------------------------------------------------------

fn with_store<T>(core: &Core, f: impl FnOnce(&mut Store) -> Result<T, StoreError>) -> Res<T> {
    let mut guard = core.store.lock().map_err(|_| CommandError::internal())?;
    let store = guard.as_mut().ok_or_else(CommandError::internal)?;
    f(store).map_err(CommandError::from)
}

fn kind_of(p: &CompanionProfile) -> Res<ProviderKind> {
    ProviderKind::parse(&p.kind).ok_or_else(CommandError::internal)
}

/// The spec used to call a profile with one of its models. Fixed providers use their built-in URL
/// whatever the database says; a compatible server's stored URL is checked again.
fn spec_of(p: &CompanionProfile, model: &str) -> Res<ProviderSpec> {
    let kind = kind_of(p)?;
    let base_url = match kind.fixed_base_url() {
        Some(u) => u.to_owned(),
        None => check_base_url(&p.base_url)?.url,
    };
    Ok(ProviderSpec {
        kind,
        base_url,
        model: model.to_owned(),
    })
}

/// A profile for the UI. A store error reading its models is returned, not hidden behind an
/// empty list (security review F5): a provider with no models would look broken in a
/// different, misleading way.
fn profile_dto(core: &Core, p: CompanionProfile) -> Res<ProviderProfileDto> {
    let models = with_store(core, |s| s.companion_models(Some(&p.id)))?
        .into_iter()
        .map(ModelDto::from)
        .collect();
    let (has_key, keychain_ok) = match core.companion.keys.has(&p.id) {
        Ok(h) => (h, true),
        Err(_) => (false, false),
    };
    let host = spec_of(&p, &p.model)
        .ok()
        .and_then(|s| check_base_url(&s.base_url).ok())
        .map(|c| c.host)
        .unwrap_or_default();
    Ok(ProviderProfileDto {
        id: p.id,
        kind: p.kind,
        label: p.label,
        base_url: p.base_url,
        host,
        model: p.model,
        models,
        monthly_token_budget: p.monthly_token_budget,
        is_default: p.is_default,
        has_key,
        keychain_ok,
    })
}

pub fn profiles(core: &Core) -> Res<Vec<ProviderProfileDto>> {
    let list = with_store(core, |s| s.companion_profiles())?;
    list.into_iter().map(|p| profile_dto(core, p)).collect()
}

/// A fresh unguessable id for one approval card.
fn new_approval_id() -> Res<String> {
    let mut buf = [0u8; 16];
    getrandom::fill(&mut buf).map_err(|_| CommandError::internal())?;
    let mut id = String::from("a-");
    for b in buf {
        id.push_str(&format!("{b:02x}"));
    }
    Ok(id)
}

fn new_profile_id() -> Res<String> {
    let mut buf = [0u8; 16];
    getrandom::fill(&mut buf).map_err(|_| CommandError::internal())?;
    let mut id = String::from("p-");
    for b in buf {
        id.push_str(&format!("{b:02x}"));
    }
    Ok(id)
}

fn clean_line(s: &str, max: usize, what: &str) -> Res<String> {
    let t = s.trim();
    if t.is_empty() || t.chars().count() > max || t.chars().any(char::is_control) {
        return Err(CommandError::invalid(&format!(
            "{what} must be 1 to {max} characters, on one line."
        )));
    }
    Ok(t.to_owned())
}

fn check_model(s: &str) -> Res<String> {
    let t = s.trim();
    // The same rule decides which models.dev ids are offered, so a chosen one always saves.
    if !models_dev::valid_model_id(t) {
        return Err(CommandError::invalid(
            "The model name may use letters, digits and . _ : / @ + - only (up to 200 characters).",
        ));
    }
    Ok(t.to_owned())
}

pub fn save_profile(core: &Core, input: ProfileInput) -> Res<ProviderProfileDto> {
    let kind = ProviderKind::parse(&input.kind).ok_or_else(|| {
        CommandError::invalid("Choose Anthropic, OpenAI or an OpenAI-compatible server.")
    })?;
    let label = clean_line(&input.label, 80, "The label")?;
    let model = check_model(&input.model)?;
    let base_url = match kind.fixed_base_url() {
        Some(u) => u.to_owned(),
        None => {
            let u = input
                .base_url
                .as_deref()
                .ok_or_else(|| CommandError::invalid("Enter the server's base URL."))?;
            check_base_url(u)?.url
        }
    };
    if input
        .monthly_token_budget
        .is_some_and(|b| !(1..=1_000_000_000_000).contains(&b))
    {
        return Err(CommandError::invalid(
            "The monthly token budget must be a positive whole number, or empty for no limit.",
        ));
    }
    let id = match input.id {
        Some(id) => {
            if with_store(core, |s| s.companion_profile(&id))?.is_none() {
                return Err(CompanionError::not_found("That profile no longer exists.").into());
            }
            id
        }
        None => new_profile_id()?,
    };
    let profile = CompanionProfile {
        id,
        kind: kind.as_str().to_owned(),
        label,
        base_url,
        model,
        monthly_token_budget: input.monthly_token_budget,
        is_default: input.is_default,
    };
    let saved = with_store(core, |s| {
        s.companion_save_profile(&profile, OffsetDateTime::now_utc())
    })?;
    profile_dto(core, saved)
}

pub fn delete_profile(core: &Core, id: &str) -> Res<()> {
    if with_store(core, |s| s.companion_profile(id))?.is_none() {
        return Err(CompanionError::not_found("That profile no longer exists.").into());
    }
    // Key first: if the keychain refuses, the profile stays so the user can try again, and no
    // key is left behind without a profile.
    core.companion
        .keys
        .delete(id)
        .map_err(|_| CompanionError::keychain_unavailable())?;
    with_store(core, |s| s.companion_delete_profile(id))
}

pub fn set_key(core: &Core, profile_id: &str, mut key: String) -> Res<()> {
    let parsed = Secret::parse(&key);
    key.zeroize();
    let secret = parsed?;
    if with_store(core, |s| s.companion_profile(profile_id))?.is_none() {
        return Err(CompanionError::not_found("That profile no longer exists.").into());
    }
    core.companion
        .keys
        .set(profile_id, &secret)
        .map_err(|_| CompanionError::keychain_unavailable())?;
    Ok(())
}

pub fn clear_key(core: &Core, profile_id: &str) -> Res<()> {
    if with_store(core, |s| s.companion_profile(profile_id))?.is_none() {
        return Err(CompanionError::not_found("That profile no longer exists.").into());
    }
    core.companion
        .keys
        .delete(profile_id)
        .map_err(|_| CompanionError::keychain_unavailable())?;
    Ok(())
}

// ---- models of a provider (decision 36) ---------------------------------------------------

fn known_profile(core: &Core, profile_id: &str) -> Res<CompanionProfile> {
    with_store(core, |s| s.companion_profile(profile_id))?
        .ok_or_else(|| CompanionError::not_found("That provider no longer exists.").into())
}

fn clean_model_label(label: Option<&str>) -> Res<Option<String>> {
    match label.map(str::trim) {
        None | Some("") => Ok(None),
        Some(l) => clean_line(l, 80, "The model's display name").map(Some),
    }
}

/// Adds a model to a provider (or renames a listed one). The id follows the same rule as a typed
/// model name; nothing is sent anywhere.
pub fn add_model(
    core: &Core,
    profile_id: &str,
    model: &str,
    label: Option<&str>,
    make_default: bool,
) -> Res<ProviderProfileDto> {
    known_profile(core, profile_id)?;
    let model = check_model(model)?;
    let label = clean_model_label(label)?;
    with_store(core, |s| {
        s.companion_add_model(
            profile_id,
            &model,
            Some(label.as_deref().unwrap_or("")),
            make_default,
            OffsetDateTime::now_utc(),
        )
    })?;
    profile_dto(core, known_profile(core, profile_id)?)
}

/// Only the store's own refusal to remove a provider's last model gets that sentence; any other
/// error keeps its usual mapping (security review F4).
fn remove_model_error(e: StoreError) -> CommandError {
    match e {
        StoreError::InvalidInput(reason) if reason == LAST_MODEL_REFUSED => CommandError::invalid(
            "A provider keeps at least one model. Add another one before removing this one.",
        ),
        other => CommandError::from(other),
    }
}

/// Removes a model from a provider. The last model of a provider can't be removed.
pub fn remove_model(core: &Core, profile_id: &str, model: &str) -> Res<ProviderProfileDto> {
    known_profile(core, profile_id)?;
    {
        let mut guard = core.store.lock().map_err(|_| CommandError::internal())?;
        let store = guard.as_mut().ok_or_else(CommandError::internal)?;
        store
            .companion_remove_model(profile_id, model)
            .map_err(remove_model_error)?;
    }
    profile_dto(core, known_profile(core, profile_id)?)
}

pub fn set_default_model(core: &Core, profile_id: &str, model: &str) -> Res<ProviderProfileDto> {
    known_profile(core, profile_id)?;
    with_store(core, |s| s.companion_set_default_model(profile_id, model))?;
    profile_dto(core, known_profile(core, profile_id)?)
}

// ---- preparing a call --------------------------------------------------------------------

struct Prepared {
    profile: CompanionProfile,
    /// The model this call uses; always one configured for `profile`.
    model: String,
    spec: ProviderSpec,
    key: Secret,
    /// `host` or `host:port` the request goes to.
    host: String,
}

/// Profile, model, key and budget, in that order. `model` must be one configured for the profile
/// (decision 36); `None` uses the profile's default model. Nothing is stored or sent here.
fn prepare(core: &Core, profile_id: &str, model: Option<&str>) -> Res<Prepared> {
    // Usage of a call cancelled earlier must count before the budget is checked.
    flush_late_usage(core);
    let profile = with_store(core, |s| s.companion_profile(profile_id))?
        .ok_or_else(|| CompanionError::not_found("That profile no longer exists."))?;
    let model = match model {
        None => profile.model.clone(),
        Some(m) => {
            let configured = models_dev::valid_model_id(m)
                && with_store(core, |s| s.companion_model(&profile.id, m))?.is_some();
            if !configured {
                return Err(CommandError::invalid(
                    "That model isn't set up for this provider. Add it in Settings, then try again.",
                ));
            }
            m.to_owned()
        }
    };
    let spec = spec_of(&profile, &model)?;
    let host = check_base_url(&spec.base_url)?.host;
    let key = core
        .companion
        .keys
        .get(&profile.id)
        .map_err(|_| CompanionError::keychain_unavailable())?
        .ok_or_else(CompanionError::no_key)?;
    let month = month_of(OffsetDateTime::now_utc());
    let used = with_store(core, |s| s.companion_usage(&month))?
        .into_iter()
        .find(|u| u.profile_id == profile.id);
    let (i, o) = used.map_or((0, 0), |u| (u.input_tokens, u.output_tokens));
    budget::check(profile.monthly_token_budget, i, o)?;
    Ok(Prepared {
        profile,
        model,
        spec,
        key,
        host,
    })
}

pub fn test_connection(core: &Core, profile_id: &str) -> Res<TestResultDto> {
    let p = prepare(core, profile_id, None)?;
    let messages = [ChatMessage {
        role: Role::User,
        content: "Reply with the single word: ok".to_owned(),
    }];
    let reply = chat(
        &core.companion.http,
        &p.spec,
        &p.key,
        &messages,
        TEST_OUTPUT_TOKENS,
        &CancelToken::new(),
    )?;
    with_store(core, |s| {
        s.companion_record_usage(
            &p.profile.id,
            OffsetDateTime::now_utc(),
            reply.input_tokens,
            reply.output_tokens,
        )
    })?;
    Ok(TestResultDto {
        ok: true,
        model: p.model,
    })
}

// ---- the models.dev model list (decision 32) ----------------------------------------------

fn catalog_dto(
    loaded: &models_dev::LoadedCatalog,
    unchanged: bool,
    from_memory: bool,
) -> CatalogDto {
    CatalogDto {
        loaded_at: ts(OffsetDateTime::from(loaded.loaded_at)),
        checked_at: ts(OffsetDateTime::from(loaded.checked_at)),
        unchanged,
        from_memory,
        providers: loaded
            .catalog
            .providers
            .iter()
            .map(|p| CatalogProviderDto {
                id: p.id.clone(),
                name: p.name.clone(),
                model_count: p.models.len(),
            })
            .collect(),
    }
}

/// The provider list. `cached` never touches the network and returns `None` before the first
/// load; `load` and `refresh` are the only paths that contact models.dev.
pub fn model_catalog(core: &Core, mode: CatalogMode) -> Res<Option<CatalogDto>> {
    let cache = &core.companion.catalog;
    match mode {
        CatalogMode::Cached => Ok(cache.cached()?.map(|l| catalog_dto(&l, false, true))),
        CatalogMode::Load | CatalogMode::Refresh => {
            let r = cache.load(mode == CatalogMode::Refresh)?;
            Ok(Some(catalog_dto(&r.loaded, r.unchanged, r.from_memory)))
        }
    }
}

/// The models of one provider from the list in memory. Never touches the network.
pub fn catalog_models(core: &Core, provider_id: &str) -> Res<Vec<CatalogModelDto>> {
    if provider_id.is_empty() || provider_id.len() > models_dev::MAX_PROVIDER_ID_BYTES {
        return Err(models_dev::unknown_provider().into());
    }
    let loaded = core
        .companion
        .catalog
        .cached()?
        .ok_or_else(models_dev::not_loaded)?;
    let provider = loaded
        .catalog
        .provider(provider_id)
        .ok_or_else(models_dev::unknown_provider)?;
    Ok(provider.models.iter().map(CatalogModelDto::from).collect())
}

// ---- threads and messages ----------------------------------------------------------------

pub fn threads(core: &Core) -> Res<Vec<ThreadDto>> {
    Ok(with_store(core, |s| s.companion_threads())?
        .into_iter()
        .map(ThreadDto::from)
        .collect())
}

pub fn new_thread(core: &Core, title: Option<&str>) -> Res<ThreadDto> {
    with_store(core, |s| {
        s.companion_new_thread(title, OffsetDateTime::now_utc())
    })
    .map(ThreadDto::from)
}

pub fn rename_thread(core: &Core, id: i64, title: &str) -> Res<()> {
    with_store(core, |s| s.companion_rename_thread(id, title))
}

pub fn delete_thread(core: &Core, id: i64) -> Res<()> {
    // A thread being sent to cannot be deleted under the call.
    let _guard = core.companion.begin(id, "delete-thread")?;
    with_store(core, |s| s.companion_delete_thread(id))
}

pub fn messages(core: &Core, thread_id: i64) -> Res<Vec<MessageDto>> {
    let (msgs, calls, notices) = with_store(core, |s| {
        s.companion_thread(thread_id)?.ok_or(StoreError::NotFound)?;
        Ok((
            s.companion_messages(thread_id)?,
            s.companion_tool_calls(thread_id)?,
            s.companion_tool_notices(thread_id)?,
        ))
    })?;
    let mut notes_by_message: HashMap<i64, Vec<ToolNoticeDto>> = HashMap::new();
    for n in notices {
        notes_by_message
            .entry(n.message_id)
            .or_default()
            .push(ToolNoticeDto::from(n));
    }
    let mut by_message: HashMap<i64, Vec<ToolCallDto>> = HashMap::new();
    for c in calls {
        by_message
            .entry(c.message_id)
            .or_default()
            .push(ToolCallDto::from(c));
    }
    Ok(msgs
        .into_iter()
        .map(|m| {
            let id = m.id;
            let mut dto = MessageDto::from(m);
            dto.tool_calls = by_message.remove(&id).unwrap_or_default();
            dto.tool_notices = notes_by_message.remove(&id).unwrap_or_default();
            dto
        })
        .collect())
}

pub fn usage(core: &Core) -> Res<Vec<UsageDto>> {
    flush_late_usage(core);
    let month = month_of(OffsetDateTime::now_utc());
    with_store(core, |s| {
        let budgets: HashMap<String, Option<i64>> = s
            .companion_profiles()?
            .into_iter()
            .map(|p| (p.id, p.monthly_token_budget))
            .collect();
        Ok(s.companion_usage(&month)?
            .into_iter()
            .map(|u| UsageDto {
                budget: budgets.get(&u.profile_id).copied().flatten(),
                profile_id: u.profile_id,
                month: month.clone(),
                input_tokens: u.input_tokens,
                output_tokens: u.output_tokens,
            })
            .collect())
    })
}

// ---- context scopes ----------------------------------------------------------------------

fn ts(t: OffsetDateTime) -> String {
    t.format(&Rfc3339).unwrap_or_default()
}

fn check_session_ref(provider: &str, session_id: &str) -> Res<()> {
    let ok = |s: &str| !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control);
    if ok(provider) && ok(session_id) {
        Ok(())
    } else {
        Err(CommandError::invalid("Choose a session to share from."))
    }
}

/// The text that goes with a message for `scope`, or `None` for `none`. Used by BOTH the preview
/// and the send.
fn attached_text(core: &Core, scope: &Scope) -> Res<Option<String>> {
    let (provider, session_id, selection) = match scope {
        Scope::None => return Ok(None),
        Scope::SessionSummary {
            provider,
            session_id,
        } => (provider, session_id, Selection::SessionSummary),
        Scope::SelectedNotes {
            provider,
            session_id,
            notes,
            item_ids,
        } => {
            if item_ids.len() > MAX_SCOPE_ITEMS {
                return Err(CommandError::invalid("Too many items are selected."));
            }
            if !*notes && item_ids.is_empty() {
                return Err(CommandError::invalid(
                    "Choose the notes or at least one item to share.",
                ));
            }
            (
                provider,
                session_id,
                Selection::SelectedNotes {
                    notes: *notes,
                    item_ids: item_ids.clone(),
                },
            )
        }
    };
    check_session_ref(provider, session_id)?;
    let facts = with_store(core, |s| session_facts(s, provider, session_id))?;
    if let Selection::SelectedNotes { item_ids, .. } = &selection
        && item_ids
            .iter()
            .any(|id| !facts.items.iter().any(|i| i.id == *id))
    {
        return Err(CommandError::invalid(
            "One of the selected items no longer exists. Refresh and choose again.",
        ));
    }
    Ok(::companion::scope::build(&selection, &facts))
}

fn session_facts(store: &Store, provider: &str, id: &str) -> Result<SessionFacts, StoreError> {
    let record = store.session(provider, id)?.ok_or(StoreError::NotFound)?;
    let ctx = store.context(provider, id)?;
    let items = store.items(provider, id)?;
    let key = (provider.to_owned(), id.to_owned());
    let imported = store.imported_sessions()?.contains_key(&key);
    let hinted = store.log_hinted_sessions()?.contains_key(&key);
    let inferred_end = record.state == agent_core::SessionState::Ended
        && store
            .ended_reasons()?
            .get(&key)
            .is_some_and(|r| r == agent_core::reducer::ENDED_PROCESS_GONE);
    let status = if hinted {
        format!(
            "{} (guessed from the agent's log, filled in at start-up at {}; the agent has not reported a state since)",
            record.state.as_str(),
            ts(record.last_seen_utc)
        )
    } else if imported {
        format!(
            "{} (guessed from the agent's log when imported at {}; the agent has not reported a state since)",
            record.state.as_str(),
            ts(record.last_seen_utc)
        )
    } else if inferred_end {
        format!(
            "ended (inferred by Lullshift: the agent's program is no longer running; the agent did not report this; marked at {})",
            ts(record.state_timestamp_utc.unwrap_or(record.last_seen_utc))
        )
    } else {
        format!(
            "{} (as last reported at {}; it may have changed since)",
            record.state.as_str(),
            ts(record.last_seen_utc)
        )
    };
    let mut facts = SessionFacts {
        status,
        items: items
            .into_iter()
            .map(|i| ItemFacts {
                id: i.id,
                kind: i.kind,
                category: i.category,
                text: i.text,
                done: i.done,
            })
            .collect(),
        ..SessionFacts::default()
    };
    if let Some(c) = ctx {
        facts.title = c.user_title.or(c.agent_title.map(|t| t.text));
        facts.intent = c.user_intent.or(c.captured_intent.map(|t| t.text));
        facts.ask = c.ask.map(|t| t.text);
        facts.notes = c.notes;
    }
    Ok(facts)
}

pub fn preview_context(core: &Core, scope: Scope) -> Res<ContextPreviewDto> {
    let attached = attached_text(core, &scope)?;
    let sha256 = attached.as_deref().map(context_sha256);
    let text = attached.unwrap_or_default();
    Ok(ContextPreviewDto {
        chars: text.chars().count(),
        sha256,
        scope,
        text,
    })
}

// ---- settings kept by Companion ------------------------------------------------------------

fn load_prefs(core: &Core) -> CompanionPrefs {
    with_store(core, |s| s.preference(COMPANION_PREFS_KEY))
        .ok()
        .flatten()
        .and_then(|j| serde_json::from_str::<CompanionPrefs>(&j).ok())
        .unwrap_or_default()
}

fn save_prefs(core: &Core, prefs: &CompanionPrefs) -> Res<()> {
    let json = serde_json::to_string(prefs).map_err(|_| CommandError::internal())?;
    with_store(core, |s| s.set_preference(COMPANION_PREFS_KEY, &json))
}

pub fn prefs(core: &Core) -> CompanionPrefs {
    load_prefs(core)
}

pub fn set_tools_enabled(core: &Core, enabled: bool) -> Res<CompanionPrefs> {
    let mut p = load_prefs(core);
    p.tools_enabled = enabled;
    save_prefs(core, &p)?;
    Ok(p)
}

/// Remembers the context choice of a message that is about to be sent. Best effort: a failure to
/// save it must not stop the message.
fn remember_scope(core: &Core, scope: &Scope) {
    let mut p = load_prefs(core);
    if p.last_scope.as_ref() != Some(scope) {
        p.last_scope = Some(scope.clone());
        let _ = save_prefs(core, &p);
    }
}

// ---- sending -----------------------------------------------------------------------------

/// Lowercase hex SHA-256 of the exact attached text.
fn context_sha256(text: &str) -> String {
    let digest = Sha256::digest(text.as_bytes());
    let mut out = String::with_capacity(64);
    for b in digest.iter() {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// Plain text of the thread's delivered messages before `before` (or all), oldest first, that
/// went to (or came from) `host`. Messages sent to another host are left out, so a conversation
/// is never replayed to a provider it was not sent to; a message with no recorded host is left
/// out too. Old attached context is not repeated: it goes only with the message it was chosen
/// for. Lookups made for earlier messages are not replayed either; the reply that used them is.
fn history(core: &Core, thread_id: i64, before: Option<i64>, host: &str) -> Res<Vec<ChatMessage>> {
    let all = with_store(core, |s| s.companion_messages(thread_id))?;
    let list = all
        .into_iter()
        .filter(|m| {
            m.status == "sent" && before.is_none_or(|b| m.id < b) && m.host.as_deref() == Some(host)
        })
        .map(|m| ChatMessage {
            role: if m.role == "user" {
                Role::User
            } else {
                Role::Assistant
            },
            content: m.text,
        })
        .collect();
    Ok(trim_history(list))
}

/// A request id is chosen by the UI so it can ignore events that are not its own.
fn check_request_id(id: &str) -> Res<()> {
    let ok = (8..=64).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if ok {
        Ok(())
    } else {
        Err(CommandError::invalid("That request id isn't valid."))
    }
}

/// Where stream events go. The Tauri command wraps a `Channel`; tests collect them.
pub type Emit<'a> = &'a (dyn Fn(StreamEvent) + Send + Sync);

fn book(core: &Core, profile_id: &str, input: i64, output: i64) {
    if input > 0 || output > 0 {
        let _ = with_store(core, |s| {
            s.companion_record_usage(profile_id, OffsetDateTime::now_utc(), input, output)
        });
    }
}

/// Re-checks the monthly budget between provider calls of one reply, so lookups cannot run past
/// it.
fn budget_still_ok(core: &Core, profile: &CompanionProfile) -> Result<(), CompanionError> {
    flush_late_usage(core);
    let month = month_of(OffsetDateTime::now_utc());
    let used = with_store(core, |s| s.companion_usage(&month))
        .map_err(|_| CompanionError::internal())?
        .into_iter()
        .find(|u| u.profile_id == profile.id);
    let (i, o) = used.map_or((0, 0), |u| (u.input_tokens, u.output_tokens));
    budget::check(profile.monthly_token_budget, i, o)?;
    Ok(())
}

enum Decision {
    Approved,
    Declined,
}

/// Waits for the user's answer to a lookup. Cancelling, a dropped answer channel or a very long
/// wait end the request instead, and nothing is sent.
fn await_decision(
    rx: &mpsc::Receiver<bool>,
    token: &CancelToken,
) -> Result<Decision, CompanionError> {
    let start = Instant::now();
    loop {
        if token.is_cancelled() {
            return Err(CompanionError::cancelled());
        }
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(true) => return Ok(Decision::Approved),
            Ok(false) => return Ok(Decision::Declined),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if start.elapsed() > APPROVAL_WAIT {
                    return Err(CompanionError::cancelled());
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(CompanionError::cancelled());
            }
        }
    }
}

fn fail_message(core: &Core, user_message: i64, e: &CompanionError) {
    let _ = with_store(core, |s| {
        s.companion_fail_message(user_message, e.kind.as_str(), &e.message)
    });
}

struct Turn<'a> {
    core: &'a Core,
    guard: &'a SendGuard,
    prepared: &'a Prepared,
    user_message: i64,
    request_id: &'a str,
    emit: Emit<'a>,
}

impl Turn<'_> {
    /// Answers the lookups of one reply, one by one. Each valid lookup is shown with its exact
    /// result and waits for the user. Returns the answer text for every call, in order.
    fn answer_calls(
        &self,
        round: u32,
        calls: &[::companion::tools::ToolCall],
        used: &mut usize,
    ) -> Result<Vec<String>, CompanionError> {
        let mut results = Vec::new();
        for call in calls {
            let checked = check_request(&call.name, &call.arguments);
            let request = match checked {
                Ok(r) if *used < MAX_TOOL_CALLS => r,
                other => {
                    let (reason, text) = match other {
                        Ok(_) => ("limit", LIMIT_TEXT),
                        Err(Rejection::Unknown) => ("unknown", UNAVAILABLE_TEXT),
                        Err(Rejection::Malformed) => ("malformed", UNAVAILABLE_TEXT),
                    };
                    (self.emit)(StreamEvent::ToolSkipped {
                        request_id: self.request_id.to_owned(),
                        tool: ToolName::parse(&call.name)
                            .map_or("unknown", ToolName::as_str)
                            .to_owned(),
                        text: text.to_owned(),
                    });
                    // Only a short safe identifier is shown or stored, never the arguments.
                    let tool = safe_tool_name(&call.name);
                    with_store(self.core, |s| {
                        s.companion_add_tool_notice(
                            &NewToolNotice {
                                message_id: self.user_message,
                                round: i64::from(round),
                                reason,
                                tool,
                            },
                            OffsetDateTime::now_utc(),
                        )
                    })
                    .map_err(|_| CompanionError::internal())?;
                    (self.emit)(StreamEvent::ToolNotice {
                        request_id: self.request_id.to_owned(),
                        reason: reason.to_owned(),
                        tool: tool.map(str::to_owned),
                    });
                    results.push(text.to_owned());
                    continue;
                }
            };
            *used += 1;
            let approval_id = new_approval_id().map_err(|_| CompanionError::internal())?;
            let lookup = lookups::run(self.core, &request, OffsetDateTime::now_utc())
                .map_err(|_| CompanionError::internal())?;
            let text = clean_result(&lookup.text);
            let tool = request.name();
            // Register before showing, so a fast answer cannot arrive too early.
            let (tx, rx) = mpsc::channel();
            {
                let mut ap = self
                    .core
                    .companion
                    .approvals
                    .lock()
                    .map_err(|_| CompanionError::internal())?;
                ap.insert(
                    self.request_id.to_owned(),
                    Approval {
                        approval_id: approval_id.clone(),
                        tx,
                    },
                );
            }
            (self.emit)(StreamEvent::ToolRequest {
                request_id: self.request_id.to_owned(),
                approval_id: approval_id.clone(),
                tool: tool.as_str().to_owned(),
                label: tool.label().to_owned(),
                arguments: clean_result(&call.arguments),
                result_text: text.clone(),
                host: self.prepared.host.clone(),
            });
            let decision = await_decision(&rx, self.guard.token())?;
            let approved = matches!(decision, Decision::Approved);
            let sent = if approved {
                text
            } else {
                DECLINED_TEXT.to_owned()
            };
            let card = lookup.card.as_ref().filter(|_| approved);
            let args = clean_result(&call.arguments);
            with_store(self.core, |s| {
                s.companion_add_tool_call(
                    &NewToolCall {
                        message_id: self.user_message,
                        round: i64::from(round),
                        tool: tool.as_str(),
                        arguments: &args,
                        result_text: &sent,
                        status: if approved { "approved" } else { "declined" },
                        card_activity_id: card.map(|c| c.activity_id.as_str()),
                        card_name: card.map(|c| c.name.as_str()),
                        card_minutes: card.map(|c| i64::from(c.minutes)),
                        host: &self.prepared.host,
                    },
                    OffsetDateTime::now_utc(),
                )
            })
            .map_err(|_| CompanionError::internal())?;
            (self.emit)(StreamEvent::ToolResolved {
                request_id: self.request_id.to_owned(),
                approval_id: approval_id.clone(),
                approved,
            });
            results.push(sent);
        }
        Ok(results)
    }
}

/// Streams the reply, serves any lookups, then records the outcome on the stored user message.
///
/// If the user cancels, this returns at once: the thread was already freed by `cancel`, the HTTP
/// worker winds down on its own and queues the call's usage, and nothing it delivers later is
/// written anywhere (only this function writes, and it checks the token first).
#[allow(clippy::too_many_arguments)]
fn deliver(
    core: &Core,
    guard: &SendGuard,
    prepared: &Prepared,
    user_message: i64,
    mut messages: Vec<ChatMessage>,
    content: String,
    request_id: &str,
    emit: Emit<'_>,
) -> Res<MessageDto> {
    messages.push(ChatMessage {
        role: Role::User,
        content,
    });
    let tools_on = load_prefs(core).tools_enabled;
    let turn = Turn {
        core,
        guard,
        prepared,
        user_message,
        request_id,
        emit,
    };
    let mut rounds: Vec<ToolExchange> = Vec::new();
    let mut calls_used = 0usize;
    let (mut total_in, mut total_out) = (0i64, 0i64);
    let mut round_no: u32 = 0;
    let err: CompanionError = loop {
        round_no += 1;
        if round_no > 1 {
            if let Err(e) = budget_still_ok(core, &prepared.profile) {
                break e;
            }
            emit(StreamEvent::Round {
                request_id: request_id.to_owned(),
                round: round_no,
            });
        }
        let offer = tools_on && rounds.len() < MAX_TOOL_ROUNDS && calls_used < MAX_TOOL_CALLS;
        let plan = RequestPlan {
            messages: &messages,
            rounds: &rounds,
            tools: offer,
            stream: true,
            max_output_tokens: MAX_OUTPUT_TOKENS,
        };
        let queue = Arc::clone(&core.companion.late_usage);
        let profile_id = prepared.profile.id.clone();
        let result = chat_stream(
            &core.companion.http,
            &prepared.spec,
            &prepared.key,
            &plan,
            guard.token(),
            |d| {
                emit(StreamEvent::Delta {
                    request_id: request_id.to_owned(),
                    text: d.to_owned(),
                });
            },
            move |u| {
                if let Ok(mut q) = queue.lock() {
                    q.push(QueuedUsage {
                        profile_id,
                        at: OffsetDateTime::now_utc(),
                        input_tokens: u.input_tokens,
                        output_tokens: u.output_tokens,
                    });
                }
            },
        );
        match result {
            Err(f) => {
                // Zero for a call the worker will book itself (cancelled or silent) or one that
                // never produced bytes; otherwise the provider's numbers or an estimate that
                // includes the partial reply.
                book(core, &prepared.profile.id, f.input_tokens, f.output_tokens);
                break f.error;
            }
            Ok(reply) => {
                // A cancel that raced the end of the stream: discard the reply, keep the cost.
                if guard.token().is_cancelled() {
                    book(
                        core,
                        &prepared.profile.id,
                        reply.input_tokens,
                        reply.output_tokens,
                    );
                    break CompanionError::cancelled();
                }
                total_in = total_in.saturating_add(reply.input_tokens);
                total_out = total_out.saturating_add(reply.output_tokens);
                if reply.tool_calls.is_empty() || !offer {
                    if reply.text.trim().is_empty() {
                        book(
                            core,
                            &prepared.profile.id,
                            reply.input_tokens,
                            reply.output_tokens,
                        );
                        break CompanionError::invalid_response();
                    }
                    // The token is asked again inside the store transaction, which runs under
                    // the store lock: a cancel that came after the check above, followed by a
                    // newer message, can no longer let this reply land after that message.
                    let done = with_store(core, |s| {
                        s.companion_complete_send_if(
                            user_message,
                            &reply.text,
                            total_in,
                            total_out,
                            reply.input_tokens,
                            reply.output_tokens,
                            OffsetDateTime::now_utc(),
                            &|| !guard.token().is_cancelled(),
                        )
                    });
                    return match done {
                        Ok(Some(m)) => Ok(m.into()),
                        Ok(None) => {
                            // Cancelled at the last moment: the reply is dropped, its cost kept.
                            book(
                                core,
                                &prepared.profile.id,
                                reply.input_tokens,
                                reply.output_tokens,
                            );
                            fail_message(core, user_message, &CompanionError::cancelled());
                            Err(CompanionError::cancelled().into())
                        }
                        Err(e) => {
                            book(
                                core,
                                &prepared.profile.id,
                                reply.input_tokens,
                                reply.output_tokens,
                            );
                            let _ = with_store(core, |s| {
                                s.companion_fail_message(user_message, e.kind, &e.message)
                            });
                            Err(e)
                        }
                    };
                }
                // The model asked for lookups: book this call now, answer them, go round again.
                book(
                    core,
                    &prepared.profile.id,
                    reply.input_tokens,
                    reply.output_tokens,
                );
                match turn.answer_calls(round_no, &reply.tool_calls, &mut calls_used) {
                    Ok(results) => rounds.push(ToolExchange {
                        text: reply.text,
                        calls: reply.tool_calls,
                        results,
                    }),
                    Err(e) => break e,
                }
            }
        }
    };
    fail_message(core, user_message, &err);
    Err(err.into())
}

/// `expected_context_sha256` is the hash the preview returned for the attached text (`None` for
/// scope `none`). If the text built now differs, nothing is stored or sent.
#[allow(clippy::too_many_arguments)]
pub fn send(
    core: &Core,
    thread_id: i64,
    profile_id: &str,
    model: &str,
    text: &str,
    scope: &Scope,
    expected_context_sha256: Option<&str>,
    request_id: &str,
    emit: Emit<'_>,
) -> Res<MessageDto> {
    check_request_id(request_id)?;
    let guard = core.companion.begin(thread_id, request_id)?;
    let text = text.trim();
    if text.is_empty() {
        return Err(CommandError::invalid("Write a message first."));
    }
    with_store(core, |s| {
        s.companion_thread(thread_id)?.ok_or(StoreError::NotFound)
    })?;
    // Nothing is stored unless the profile, model, key and budget are all in order.
    let prepared = prepare(core, profile_id, Some(model))?;
    let attached = attached_text(core, scope)?;
    let actual = attached.as_deref().map(context_sha256);
    if actual.as_deref() != expected_context_sha256.map(str::trim) {
        return Err(CommandError::invalid(
            "The context changed since you previewed it. Review it again.",
        ));
    }
    remember_scope(core, scope);
    let past = history(core, thread_id, None, &prepared.host)?;
    let id = with_store(core, |s| {
        s.companion_add_user_message(
            &NewUserMessage {
                thread_id,
                text,
                attached_context: attached.as_deref(),
                scope_kind: scope.kind_str(),
                profile_id: &prepared.profile.id,
                model: &prepared.model,
                host: &prepared.host,
            },
            OffsetDateTime::now_utc(),
        )
    })?;
    let content = compose_user_content(attached.as_deref(), text);
    deliver(core, &guard, &prepared, id, past, content, request_id, emit)
}

/// Sends a failed message again, to `profile_id` and `model` (decision 36: the user picks them,
/// usually the ones it was first sent with). The address must be the one it was first sent to.
pub fn retry(
    core: &Core,
    message_id: i64,
    profile_id: &str,
    model: &str,
    request_id: &str,
    emit: Emit<'_>,
) -> Res<MessageDto> {
    check_request_id(request_id)?;
    let msg = with_store(core, |s| s.companion_message(message_id))?
        .ok_or_else(|| CompanionError::not_found("That message no longer exists."))?;
    let guard = core.companion.begin(msg.thread_id, request_id)?;
    // Re-read under the thread guard.
    let msg = with_store(core, |s| s.companion_message(message_id))?
        .ok_or_else(|| CompanionError::not_found("That message no longer exists."))?;
    if msg.role != "user" || msg.status != "failed" {
        return Err(CommandError::invalid(
            "Only a message that failed can be tried again.",
        ));
    }
    let last = with_store(core, |s| s.companion_messages(msg.thread_id))?
        .last()
        .map(|m| m.id);
    if last != Some(msg.id) {
        return Err(CommandError::invalid(
            "Only the latest message in a conversation can be tried again.",
        ));
    }
    let prepared = prepare(core, profile_id, Some(model))?;
    // The profile's address was edited since this message was written. It must not go to the new
    // host without the user looking at it again.
    if msg.host.as_deref() != Some(prepared.host.as_str()) {
        return Err(CommandError::invalid(
            "This provider's address changed. Send the message again to review it.",
        ));
    }
    let past = history(core, msg.thread_id, Some(msg.id), &prepared.host)?;
    with_store(core, |s| {
        s.companion_mark_pending_for(msg.id, &prepared.profile.id, &prepared.model)
    })?;
    let content = compose_user_content(msg.attached_context.as_deref(), &msg.text);
    deliver(
        core, &guard, &prepared, msg.id, past, content, request_id, emit,
    )
}

pub fn cancel(core: &Core, thread_id: i64) {
    core.companion.cancel(thread_id);
}

pub fn tool_decision(core: &Core, request_id: &str, approval_id: &str, approve: bool) -> Res<()> {
    check_request_id(request_id)?;
    if core.companion.decide(request_id, approval_id, approve) {
        Ok(())
    } else {
        Err(CompanionError::not_found("That lookup is no longer waiting.").into())
    }
}

/// At start-up: messages a crash or quit left `pending` become retryable `failed` ones.
pub fn recover_pending(core: &Core) {
    let _ = with_store(core, |s| s.companion_fail_stale_pending());
}

// ---- commands ----------------------------------------------------------------------------

type CoreState<'a> = tauri::State<'a, Arc<Core>>;

async fn run<T: Send + 'static>(
    core: &Arc<Core>,
    f: impl FnOnce(&Core) -> Res<T> + Send + 'static,
) -> Res<T> {
    let core = Arc::clone(core);
    tauri::async_runtime::spawn_blocking(move || f(&core))
        .await
        .map_err(|_| CommandError::internal())?
}

#[tauri::command]
pub async fn companion_profiles(core: CoreState<'_>) -> Res<Vec<ProviderProfileDto>> {
    run(&core, profiles).await
}

#[tauri::command]
pub async fn companion_save_profile(
    core: CoreState<'_>,
    profile: ProfileInput,
) -> Res<ProviderProfileDto> {
    run(&core, move |c| save_profile(c, profile)).await
}

#[tauri::command]
pub async fn companion_delete_profile(core: CoreState<'_>, id: String) -> Res<()> {
    run(&core, move |c| delete_profile(c, &id)).await
}

#[tauri::command]
pub async fn companion_set_key(core: CoreState<'_>, profile_id: String, key: String) -> Res<()> {
    run(&core, move |c| set_key(c, &profile_id, key)).await
}

#[tauri::command]
pub async fn companion_clear_key(core: CoreState<'_>, profile_id: String) -> Res<()> {
    run(&core, move |c| clear_key(c, &profile_id)).await
}

#[tauri::command]
pub async fn companion_test(core: CoreState<'_>, profile_id: String) -> Res<TestResultDto> {
    run(&core, move |c| test_connection(c, &profile_id)).await
}

#[tauri::command]
pub async fn companion_add_model(
    core: CoreState<'_>,
    profile_id: String,
    model: String,
    label: Option<String>,
    make_default: bool,
) -> Res<ProviderProfileDto> {
    run(&core, move |c| {
        add_model(c, &profile_id, &model, label.as_deref(), make_default)
    })
    .await
}

#[tauri::command]
pub async fn companion_remove_model(
    core: CoreState<'_>,
    profile_id: String,
    model: String,
) -> Res<ProviderProfileDto> {
    run(&core, move |c| remove_model(c, &profile_id, &model)).await
}

#[tauri::command]
pub async fn companion_set_default_model(
    core: CoreState<'_>,
    profile_id: String,
    model: String,
) -> Res<ProviderProfileDto> {
    run(&core, move |c| set_default_model(c, &profile_id, &model)).await
}

#[tauri::command]
pub async fn companion_open_link(core: CoreState<'_>, url: String) -> Res<()> {
    run(&core, move |_| open_link::open(&url)).await
}

#[tauri::command]
pub async fn companion_model_catalog(
    core: CoreState<'_>,
    mode: CatalogMode,
) -> Res<Option<CatalogDto>> {
    run(&core, move |c| model_catalog(c, mode)).await
}

#[tauri::command]
pub async fn companion_catalog_models(
    core: CoreState<'_>,
    provider_id: String,
) -> Res<Vec<CatalogModelDto>> {
    run(&core, move |c| catalog_models(c, &provider_id)).await
}

#[tauri::command]
pub async fn companion_threads(core: CoreState<'_>) -> Res<Vec<ThreadDto>> {
    run(&core, threads).await
}

#[tauri::command]
pub async fn companion_new_thread(core: CoreState<'_>, title: Option<String>) -> Res<ThreadDto> {
    run(&core, move |c| new_thread(c, title.as_deref())).await
}

#[tauri::command]
pub async fn companion_rename_thread(core: CoreState<'_>, id: i64, title: String) -> Res<()> {
    run(&core, move |c| rename_thread(c, id, &title)).await
}

#[tauri::command]
pub async fn companion_delete_thread(core: CoreState<'_>, id: i64) -> Res<()> {
    run(&core, move |c| delete_thread(c, id)).await
}

#[tauri::command]
pub async fn companion_messages(core: CoreState<'_>, thread_id: i64) -> Res<Vec<MessageDto>> {
    run(&core, move |c| messages(c, thread_id)).await
}

#[tauri::command]
pub async fn companion_preview_context(
    core: CoreState<'_>,
    scope: Scope,
) -> Res<ContextPreviewDto> {
    run(&core, move |c| preview_context(c, scope)).await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn companion_send(
    core: CoreState<'_>,
    thread_id: i64,
    profile_id: String,
    model: String,
    text: String,
    scope: Scope,
    expected_context_sha256: Option<String>,
    request_id: String,
    on_event: tauri::ipc::Channel<StreamEvent>,
) -> Res<MessageDto> {
    run(&core, move |c| {
        let emit = channel_emitter(on_event);
        emit(StreamEvent::Started {
            request_id: request_id.clone(),
        });
        send(
            c,
            thread_id,
            &profile_id,
            &model,
            &text,
            &scope,
            expected_context_sha256.as_deref(),
            &request_id,
            &emit,
        )
    })
    .await
}

#[tauri::command]
pub async fn companion_retry(
    core: CoreState<'_>,
    message_id: i64,
    profile_id: String,
    model: String,
    request_id: String,
    on_event: tauri::ipc::Channel<StreamEvent>,
) -> Res<MessageDto> {
    run(&core, move |c| {
        let emit = channel_emitter(on_event);
        emit(StreamEvent::Started {
            request_id: request_id.clone(),
        });
        retry(c, message_id, &profile_id, &model, &request_id, &emit)
    })
    .await
}

/// Events go to the UI over the channel the command was called with, so they reach only that
/// call. A closed channel (the UI went away) just drops them.
fn channel_emitter(
    on_event: tauri::ipc::Channel<StreamEvent>,
) -> impl Fn(StreamEvent) + Send + Sync {
    move |e| {
        let _ = on_event.send(e);
    }
}

#[tauri::command]
pub async fn companion_tool_decision(
    core: CoreState<'_>,
    request_id: String,
    approval_id: String,
    approve: bool,
) -> Res<()> {
    tool_decision(&core, &request_id, &approval_id, approve)
}

#[tauri::command]
pub async fn companion_prefs(core: CoreState<'_>) -> Res<CompanionPrefs> {
    run(&core, |c| Ok(prefs(c))).await
}

#[tauri::command]
pub async fn companion_set_tools_enabled(
    core: CoreState<'_>,
    enabled: bool,
) -> Res<CompanionPrefs> {
    run(&core, move |c| set_tools_enabled(c, enabled)).await
}

#[tauri::command]
pub async fn companion_cancel(core: CoreState<'_>, thread_id: i64) -> Res<()> {
    cancel(&core, thread_id);
    Ok(())
}

#[tauri::command]
pub async fn companion_usage(core: CoreState<'_>) -> Res<Vec<UsageDto>> {
    run(&core, usage).await
}

#[cfg(test)]
mod tests;
