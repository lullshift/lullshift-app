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
use std::sync::{Arc, Mutex};

use ::companion::provider::{MAX_OUTPUT_TOKENS, TEST_OUTPUT_TOKENS};
use ::companion::scope::{ItemFacts, Selection, SessionFacts, compose_user_content, trim_history};
use ::companion::url_check::check_base_url;
use ::companion::{
    CancelToken, ChatMessage, CompanionError, HttpConfig, KeyStore, KeyringStore, ProviderKind,
    ProviderSpec, Role, Secret, budget, chat, chat_tracked,
};
use local_store::{
    CompanionMessage, CompanionProfile, CompanionThread, NewUserMessage, Store, StoreError,
    month_of,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use zeroize::Zeroize;

use crate::Core;

/// Most items one scope may pick.
const MAX_SCOPE_ITEMS: usize = 200;

// ---- state -------------------------------------------------------------------------------

/// Usage of a call that was cancelled while its worker was still on the wire. It is queued by the
/// worker (which has no store access) and written to the store by the next Companion command.
struct QueuedUsage {
    profile_id: String,
    at: OffsetDateTime,
    input_tokens: i64,
    output_tokens: i64,
}

type BusyMap = Arc<Mutex<HashMap<i64, CancelToken>>>;

pub struct CompanionState {
    pub(crate) keys: Arc<dyn KeyStore>,
    pub(crate) http: HttpConfig,
    cancels: BusyMap,
    late_usage: Arc<Mutex<Vec<QueuedUsage>>>,
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
            cancels: Arc::new(Mutex::new(HashMap::new())),
            late_usage: Arc::new(Mutex::new(Vec::new())),
        }
    }

    #[cfg(test)]
    pub fn with(keys: Arc<dyn KeyStore>, http: HttpConfig) -> Self {
        Self::with_parts(keys, http)
    }

    /// Marks `thread` as sending, or says it already is. The mark is removed when the last clone
    /// of the guard is dropped; a worker that outlives a cancel holds one, so the thread stays
    /// busy until the request has really ended.
    fn begin(&self, thread: i64) -> Res<SendGuard> {
        let mut m = self.cancels.lock().map_err(|_| CommandError::internal())?;
        if m.contains_key(&thread) {
            return Err(CommandError::from(CompanionError::busy()));
        }
        let token = CancelToken::new();
        m.insert(thread, token.clone());
        Ok(SendGuard(Arc::new(GuardInner {
            map: Arc::clone(&self.cancels),
            thread,
            token,
        })))
    }

    fn cancel(&self, thread: i64) {
        if let Ok(m) = self.cancels.lock()
            && let Some(t) = m.get(&thread)
        {
            t.cancel();
        }
    }
}

impl Default for CompanionState {
    fn default() -> Self {
        Self::new()
    }
}

struct GuardInner {
    map: BusyMap,
    thread: i64,
    token: CancelToken,
}

impl Drop for GuardInner {
    fn drop(&mut self) {
        if let Ok(mut m) = self.map.lock() {
            m.remove(&self.thread);
        }
    }
}

#[derive(Clone)]
struct SendGuard(Arc<GuardInner>);

impl SendGuard {
    fn token(&self) -> &CancelToken {
        &self.0.token
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
    model: String,
    monthly_token_budget: Option<i64>,
    is_default: bool,
    has_key: bool,
    keychain_ok: bool,
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
    status: String,
    error: Option<MessageErrorDto>,
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    created_at: String,
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
            status: m.status,
            error,
            input_tokens: m.input_tokens,
            output_tokens: m.output_tokens,
            created_at: m.created_utc,
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

/// The spec used to call a profile. Fixed providers use their built-in URL whatever the
/// database says; a compatible server's stored URL is checked again.
fn spec_of(p: &CompanionProfile) -> Res<ProviderSpec> {
    let kind = kind_of(p)?;
    let base_url = match kind.fixed_base_url() {
        Some(u) => u.to_owned(),
        None => check_base_url(&p.base_url)?.url,
    };
    Ok(ProviderSpec {
        kind,
        base_url,
        model: p.model.clone(),
    })
}

fn profile_dto(core: &Core, p: CompanionProfile) -> ProviderProfileDto {
    let (has_key, keychain_ok) = match core.companion.keys.has(&p.id) {
        Ok(h) => (h, true),
        Err(_) => (false, false),
    };
    let host = spec_of(&p)
        .ok()
        .and_then(|s| check_base_url(&s.base_url).ok())
        .map(|c| c.host)
        .unwrap_or_default();
    ProviderProfileDto {
        id: p.id,
        kind: p.kind,
        label: p.label,
        base_url: p.base_url,
        host,
        model: p.model,
        monthly_token_budget: p.monthly_token_budget,
        is_default: p.is_default,
        has_key,
        keychain_ok,
    }
}

pub fn profiles(core: &Core) -> Res<Vec<ProviderProfileDto>> {
    let list = with_store(core, |s| s.companion_profiles())?;
    Ok(list.into_iter().map(|p| profile_dto(core, p)).collect())
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
    let ok = !t.is_empty()
        && t.len() <= 200
        && t.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:/@+-".contains(&b));
    if !ok {
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
    Ok(profile_dto(core, saved))
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

// ---- preparing a call --------------------------------------------------------------------

struct Prepared {
    profile: CompanionProfile,
    spec: ProviderSpec,
    key: Secret,
    /// `host` or `host:port` the request goes to.
    host: String,
}

/// Profile, key and budget, in that order. Nothing is stored and nothing is sent here.
fn prepare(core: &Core, profile_id: &str) -> Res<Prepared> {
    // Usage of a call cancelled earlier must count before the budget is checked.
    flush_late_usage(core);
    let profile = with_store(core, |s| s.companion_profile(profile_id))?
        .ok_or_else(|| CompanionError::not_found("That profile no longer exists."))?;
    let spec = spec_of(&profile)?;
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
        spec,
        key,
        host,
    })
}

pub fn test_connection(core: &Core, profile_id: &str) -> Res<TestResultDto> {
    let p = prepare(core, profile_id)?;
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
        model: p.profile.model,
    })
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
    let _guard = core.companion.begin(id)?;
    with_store(core, |s| s.companion_delete_thread(id))
}

pub fn messages(core: &Core, thread_id: i64) -> Res<Vec<MessageDto>> {
    with_store(core, |s| {
        s.companion_thread(thread_id)?.ok_or(StoreError::NotFound)?;
        s.companion_messages(thread_id)
    })
    .map(|v| v.into_iter().map(MessageDto::from).collect())
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
    let status = format!(
        "{} (as last reported at {}; it may have changed since)",
        record.state.as_str(),
        ts(record.last_seen_utc)
    );
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
/// for.
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

/// Sends, then records the outcome on the stored user message.
///
/// If the user cancels, this returns at once, but the HTTP worker may still be running: it keeps
/// a clone of `guard`, so the thread stays busy until the worker has ended, and it queues the
/// call's usage (real if a reply arrived, otherwise estimated from the request size).
fn deliver(
    core: &Core,
    guard: &SendGuard,
    prepared: &Prepared,
    user_message: i64,
    mut messages: Vec<ChatMessage>,
    content: String,
) -> Res<MessageDto> {
    messages.push(ChatMessage {
        role: Role::User,
        content,
    });
    let queue = Arc::clone(&core.companion.late_usage);
    let profile_id = prepared.profile.id.clone();
    let result = chat_tracked(
        &core.companion.http,
        &prepared.spec,
        &prepared.key,
        &messages,
        MAX_OUTPUT_TOKENS,
        guard.token(),
        guard.clone(),
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
        Ok(reply) => {
            let done = with_store(core, |s| {
                s.companion_complete_send(
                    user_message,
                    &reply.text,
                    reply.input_tokens,
                    reply.output_tokens,
                    OffsetDateTime::now_utc(),
                )
            });
            match done {
                Ok(m) => Ok(m.into()),
                Err(e) => {
                    let _ = with_store(core, |s| {
                        s.companion_fail_message(user_message, e.kind, &e.message)
                    });
                    Err(e)
                }
            }
        }
        Err(e) => {
            let _ = with_store(core, |s| {
                s.companion_fail_message(user_message, e.kind.as_str(), &e.message)
            });
            Err(e.into())
        }
    }
}

/// `expected_context_sha256` is the hash the preview returned for the attached text (`None` for
/// scope `none`). If the text built now differs, nothing is stored or sent.
pub fn send(
    core: &Core,
    thread_id: i64,
    profile_id: &str,
    text: &str,
    scope: &Scope,
    expected_context_sha256: Option<&str>,
) -> Res<MessageDto> {
    let guard = core.companion.begin(thread_id)?;
    let text = text.trim();
    if text.is_empty() {
        return Err(CommandError::invalid("Write a message first."));
    }
    with_store(core, |s| {
        s.companion_thread(thread_id)?.ok_or(StoreError::NotFound)
    })?;
    // Nothing is stored unless the profile, key and budget are all in order.
    let prepared = prepare(core, profile_id)?;
    let attached = attached_text(core, scope)?;
    let actual = attached.as_deref().map(context_sha256);
    if actual.as_deref() != expected_context_sha256.map(str::trim) {
        return Err(CommandError::invalid(
            "The context changed since you previewed it. Review it again.",
        ));
    }
    let past = history(core, thread_id, None, &prepared.host)?;
    let id = with_store(core, |s| {
        s.companion_add_user_message(
            &NewUserMessage {
                thread_id,
                text,
                attached_context: attached.as_deref(),
                scope_kind: scope.kind_str(),
                profile_id: &prepared.profile.id,
                model: &prepared.profile.model,
                host: &prepared.host,
            },
            OffsetDateTime::now_utc(),
        )
    })?;
    let content = compose_user_content(attached.as_deref(), text);
    deliver(core, &guard, &prepared, id, past, content)
}

pub fn retry(core: &Core, message_id: i64) -> Res<MessageDto> {
    let msg = with_store(core, |s| s.companion_message(message_id))?
        .ok_or_else(|| CompanionError::not_found("That message no longer exists."))?;
    let guard = core.companion.begin(msg.thread_id)?;
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
    let profile_id = msg.profile_id.clone().ok_or_else(|| {
        CompanionError::not_found(
            "The profile this message used was deleted. Send it again with another profile.",
        )
    })?;
    let prepared = prepare(core, &profile_id)?;
    // The profile's address was edited since this message was written. It must not go to the new
    // host without the user looking at it again.
    if msg.host.as_deref() != Some(prepared.host.as_str()) {
        return Err(CommandError::invalid(
            "This provider's address changed. Send the message again to review it.",
        ));
    }
    let past = history(core, msg.thread_id, Some(msg.id), &prepared.host)?;
    with_store(core, |s| s.companion_mark_pending(msg.id))?;
    let content = compose_user_content(msg.attached_context.as_deref(), &msg.text);
    deliver(core, &guard, &prepared, msg.id, past, content)
}

pub fn cancel(core: &Core, thread_id: i64) {
    core.companion.cancel(thread_id);
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
pub async fn companion_send(
    core: CoreState<'_>,
    thread_id: i64,
    profile_id: String,
    text: String,
    scope: Scope,
    expected_context_sha256: Option<String>,
) -> Res<MessageDto> {
    run(&core, move |c| {
        send(
            c,
            thread_id,
            &profile_id,
            &text,
            &scope,
            expected_context_sha256.as_deref(),
        )
    })
    .await
}

#[tauri::command]
pub async fn companion_retry(core: CoreState<'_>, message_id: i64) -> Res<MessageDto> {
    run(&core, move |c| retry(c, message_id)).await
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
