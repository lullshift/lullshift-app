//! Context Keeper and Next Move commands (Sprint 04, ADR-013). The UI contract is
//! `apps/desktop/src/context/api.ts`; change both together.
//!
//! Rules this module enforces:
//! - Task text is stored only if the user turned capture on for that adapter (second gate; the
//!   forwarder's marker file is the first), is cleaned again here, and never reaches the event
//!   log, logs or notifications.
//! - Errors are short codes (`not_found`, `invalid_input`, `too_many`, `store_unavailable`,
//!   `internal`), never raw text.
//! - Export saves through a native dialog opened from Rust; the webview never sees a path.
//! - Event retention runs at start-up and hourly; Context Keeper data stays until deleted.

use std::collections::{BTreeMap, HashMap};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use agent_adapters::claude_code::ADAPTER_ID;
use agent_adapters::forward::CAPTURE_DIR;
use agent_adapters::redact;
use agent_core::{AgentEvent, SessionRecord, SessionState, SessionView};
use agent_ipc::{EventContext, MAX_ASK_CHARS, MAX_INTENT_CHARS};
use local_store::{ContextRecord, ContextSummary, IngestOutcome, Item, Store, StoreError};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::DialogExt;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::{Core, SESSIONS_CHANGED};

/// Preference keys in the store.
pub const CONTEXT_PREFS_KEY: &str = "context";
pub const UI_PREFS_KEY: &str = "ui";

/// Days agent events are kept: the only choices the UI offers.
pub const RETENTION_CHOICES: [u32; 4] = [7, 30, 90, 365];
pub const DEFAULT_RETENTION_DAYS: u32 = 30;
/// Adapters that can capture task text. Marker file names come only from this list.
pub const CAPTURE_ADAPTERS: [&str; 1] = [ADAPTER_ID];
/// Timeline entries returned to the panel (the newest).
pub const TIMELINE_LIMIT: usize = 500;
const PURGE_EVERY: Duration = Duration::from_secs(60 * 60);

// ---- preferences -------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextPrefs {
    pub retention_days: u32,
    /// Opt-in capture per adapter id. Missing means off.
    #[serde(default)]
    pub capture: BTreeMap<String, bool>,
}

impl Default for ContextPrefs {
    fn default() -> Self {
        Self {
            retention_days: DEFAULT_RETENTION_DAYS,
            capture: BTreeMap::new(),
        }
    }
}

impl ContextPrefs {
    /// Checks what serde can't. Invalid preferences are never stored.
    pub fn validate(&self) -> Result<(), &'static str> {
        if !RETENTION_CHOICES.contains(&self.retention_days) {
            return Err("invalid_input");
        }
        if self
            .capture
            .keys()
            .any(|k| !CAPTURE_ADAPTERS.contains(&k.as_str()))
        {
            return Err("invalid_input");
        }
        Ok(())
    }

    /// Stored JSON, or defaults (capture off, 30 days) if missing or unreadable.
    pub fn from_stored(json: Option<&str>) -> Self {
        json.and_then(|j| serde_json::from_str::<Self>(j).ok())
            .filter(|p| p.validate().is_ok())
            .unwrap_or_default()
    }

    pub fn capture_on(&self, adapter: &str) -> bool {
        self.capture.get(adapter).copied().unwrap_or(false)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UiPrefs {
    pub detail_panel_pinned: bool,
}

impl Default for UiPrefs {
    fn default() -> Self {
        Self {
            detail_panel_pinned: true,
        }
    }
}

impl UiPrefs {
    pub fn from_stored(json: Option<&str>) -> Self {
        json.and_then(|j| serde_json::from_str(j).ok())
            .unwrap_or_default()
    }
}

// ---- consent marker files ----------------------------------------------------------------

pub fn marker_path(data_dir: &Path, adapter: &str) -> PathBuf {
    data_dir.join(CAPTURE_DIR).join(adapter)
}

/// Deletes the marker of every adapter whose capture is off.
pub fn remove_markers(data_dir: &Path, prefs: &ContextPrefs) -> std::io::Result<()> {
    for adapter in CAPTURE_ADAPTERS {
        if !prefs.capture_on(adapter) {
            match std::fs::remove_file(marker_path(data_dir, adapter)) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
    }
    Ok(())
}

/// Creates the marker of every adapter whose capture is on.
pub fn create_markers(data_dir: &Path, prefs: &ContextPrefs) -> std::io::Result<()> {
    for adapter in CAPTURE_ADAPTERS {
        if prefs.capture_on(adapter) {
            let path = marker_path(data_dir, adapter);
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            if !path.is_file() {
                std::fs::write(&path, b"")?;
            }
        }
    }
    Ok(())
}

// ---- errors and helpers ------------------------------------------------------------------

fn code(e: &StoreError) -> &'static str {
    match e {
        StoreError::NotFound => "not_found",
        StoreError::InvalidInput(_) => "invalid_input",
        StoreError::TooMany { .. } => "too_many",
        StoreError::QuotaExceeded { .. } => "invalid_input",
        _ => "store_unavailable",
    }
}

pub async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|_| "internal".to_owned())?
}

/// Runs `f` with the store locked.
fn with_store<T>(
    core: &Core,
    f: impl FnOnce(&mut Store) -> Result<T, StoreError>,
) -> Result<T, String> {
    let mut guard = core.store.lock().map_err(|_| "internal")?;
    let store = guard.as_mut().ok_or("store_unavailable")?;
    f(store).map_err(|e| code(&e).to_owned())
}

fn ts(t: OffsetDateTime) -> String {
    t.format(&Rfc3339).unwrap_or_default()
}

fn notify_changed(app: &AppHandle) {
    let _ = app.emit(SESSIONS_CHANGED, ());
}

// ---- capture (the sink side) -------------------------------------------------------------

/// Stores an event and, if the user opted in for its provider, the task text that came with
/// it. The text is cleaned and limited again here. It goes to `session_context` only; the event
/// log never sees it. A failure to store the text never loses the event.
pub fn ingest_with_context(
    store: &mut Store,
    event: &AgentEvent,
    context: Option<EventContext>,
    received: OffsetDateTime,
) -> Result<IngestOutcome, StoreError> {
    let outcome = store.ingest(event, received)?;
    if matches!(outcome, IngestOutcome::Stored { .. })
        && let Some(ctx) = context
    {
        let _ = capture(
            store,
            &event.provider,
            &event.provider_session_id,
            &ctx,
            received,
        );
    }
    Ok(outcome)
}

/// Second consent gate: discards the text unless capture is on for `provider` in the stored
/// settings. Returns whether anything was stored.
pub fn capture(
    store: &mut Store,
    provider: &str,
    session: &str,
    ctx: &EventContext,
    now: OffsetDateTime,
) -> Result<bool, StoreError> {
    let prefs = ContextPrefs::from_stored(store.preference(CONTEXT_PREFS_KEY)?.as_deref());
    if !prefs.capture_on(provider) {
        return Ok(false);
    }
    let mut stored = false;
    if let Some(intent) = ctx.intent.as_deref() {
        let text = redact::clean(intent, MAX_INTENT_CHARS);
        if !text.is_empty() {
            store.record_prompt(provider, session, &text, now)?;
            stored = true;
        }
    }
    if let Some(ask) = ctx.ask.as_deref() {
        let text = redact::clean(ask, MAX_ASK_CHARS);
        if !text.is_empty() {
            store.record_ask(provider, session, &text, now)?;
            stored = true;
        }
    }
    Ok(stored)
}

// ---- dashboard review flag ---------------------------------------------------------------

/// A dashboard session: the derived view plus whether its return card is waiting.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardSession {
    #[serde(flatten)]
    pub view: SessionView,
    pub needs_review: bool,
}

/// True when the session reached an end state, holds any context or items, and the user has
/// not reviewed it since that state was reached.
pub fn needs_review(
    state: SessionState,
    state_timestamp: Option<OffsetDateTime>,
    summary: Option<&ContextSummary>,
) -> bool {
    let ended = matches!(
        state,
        SessionState::Completed
            | SessionState::Failed
            | SessionState::Cancelled
            | SessionState::Ended
    );
    let Some(summary) = summary.filter(|s| s.has_content) else {
        return false;
    };
    ended
        && match summary.reviewed_utc {
            None => true,
            Some(reviewed) => state_timestamp.is_some_and(|t| reviewed < t),
        }
}

pub fn dashboard_session(
    view: SessionView,
    record: &SessionRecord,
    summaries: &HashMap<(String, String), ContextSummary>,
) -> DashboardSession {
    let summary = summaries.get(&(record.provider.clone(), record.provider_session_id.clone()));
    DashboardSession {
        needs_review: needs_review(view.state, record.state_timestamp_utc, summary),
        view,
    }
}

// ---- retention ---------------------------------------------------------------------------

pub fn retention_cutoff(now: OffsetDateTime, days: u32) -> OffsetDateTime {
    now - time::Duration::days(i64::from(days))
}

/// Deletes events older than the stored retention. Returns how many (`None` if the store is
/// unavailable).
pub fn purge_now(core: &Core) -> Option<usize> {
    let guard = core.store.lock().ok()?;
    let store = guard.as_ref()?;
    let prefs = ContextPrefs::from_stored(store.preference(CONTEXT_PREFS_KEY).ok()?.as_deref());
    store
        .purge_events_before(retention_cutoff(
            OffsetDateTime::now_utc(),
            prefs.retention_days,
        ))
        .ok()
}

/// Purges now and then every hour, on a background thread that ends with the process.
pub fn spawn_retention(core: Arc<Core>) {
    let _ = std::thread::Builder::new()
        .name("retention".into())
        .spawn(move || {
            loop {
                let _ = purge_now(&core);
                std::thread::sleep(PURGE_EVERY);
            }
        });
}

/// Start-up: make the consent markers match the stored settings (a lost marker file or a stale
/// one must never contradict the setting).
pub fn reconcile_markers(core: &Core, data_dir: &Path) {
    let prefs = core
        .store
        .lock()
        .ok()
        .and_then(|g| {
            g.as_ref()
                .and_then(|s| s.preference(CONTEXT_PREFS_KEY).ok().flatten())
        })
        .map(|j| ContextPrefs::from_stored(Some(&j)))
        .unwrap_or_default();
    let _ = remove_markers(data_dir, &prefs);
    let _ = create_markers(data_dir, &prefs);
}

// ---- view types --------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyDto {
    provider: String,
    session_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemDto {
    id: i64,
    kind: String,
    category: Option<String>,
    text: String,
    done: bool,
    position: i64,
    created_utc: String,
}

impl From<Item> for ItemDto {
    fn from(i: Item) -> Self {
        Self {
            id: i.id,
            kind: i.kind,
            category: i.category,
            text: i.text,
            done: i.done,
            position: i.position,
            created_utc: i.created_utc,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentDto {
    text: String,
    source: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapturedDto {
    text: String,
    captured_utc: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextDto {
    intent: Option<IntentDto>,
    latest_request: Option<CapturedDto>,
    ask: Option<CapturedDto>,
    notes: String,
    reviewed_utc: Option<String>,
    has_captured: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineDto {
    timestamp_utc: String,
    event_type: String,
    reason: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionDetailDto {
    context: ContextDto,
    items: Vec<ItemDto>,
    links: Vec<KeyDto>,
    timeline: Vec<TimelineDto>,
    retention_days: u32,
}

fn captured_dto(c: Option<&local_store::Captured>) -> Option<CapturedDto> {
    c.map(|c| CapturedDto {
        text: c.text.clone(),
        captured_utc: c.captured_utc.clone(),
    })
}

/// The user's intent if set, otherwise the captured one.
fn context_dto(rec: Option<&ContextRecord>) -> ContextDto {
    let Some(rec) = rec else {
        return ContextDto {
            intent: None,
            latest_request: None,
            ask: None,
            notes: String::new(),
            reviewed_utc: None,
            has_captured: false,
        };
    };
    let intent = match (&rec.user_intent, &rec.captured_intent) {
        (Some(t), _) => Some(IntentDto {
            text: t.clone(),
            source: "user",
        }),
        (None, Some(c)) => Some(IntentDto {
            text: c.text.clone(),
            source: "agent",
        }),
        (None, None) => None,
    };
    ContextDto {
        intent,
        latest_request: None,
        ask: captured_dto(rec.ask.as_ref()),
        notes: rec.notes.clone(),
        reviewed_utc: rec.reviewed_utc.clone(),
        has_captured: rec.has_captured(),
    }
}

fn retention(store: &Store) -> u32 {
    let json = store.preference(CONTEXT_PREFS_KEY).ok().flatten();
    ContextPrefs::from_stored(json.as_deref()).retention_days
}

fn detail(store: &Store, provider: &str, id: &str) -> Result<SessionDetailDto, StoreError> {
    store.session(provider, id)?.ok_or(StoreError::NotFound)?;
    let ctx = store.context(provider, id)?;
    Ok(SessionDetailDto {
        context: context_dto(ctx.as_ref()),
        items: store
            .items(provider, id)?
            .into_iter()
            .map(ItemDto::from)
            .collect(),
        links: store
            .links(provider, id)?
            .into_iter()
            .map(|(provider, session_id)| KeyDto {
                provider,
                session_id,
            })
            .collect(),
        timeline: store
            .timeline(provider, id, TIMELINE_LIMIT)?
            .into_iter()
            .map(|t| TimelineDto {
                timestamp_utc: t.timestamp_utc,
                event_type: t.event_type,
                reason: t.reason,
            })
            .collect(),
        retention_days: retention(store),
    })
}

// ---- export ------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Markdown,
    Json,
}

impl ExportFormat {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "markdown" => Some(Self::Markdown),
            "json" => Some(Self::Json),
            _ => None,
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Markdown => "md",
            Self::Json => "json",
        }
    }
}

/// Everything an export holds. Built under the store lock, rendered and written outside it.
pub struct ExportData {
    pub record: SessionRecord,
    pub context: Option<ContextRecord>,
    pub items: Vec<Item>,
    pub links: Vec<(String, String)>,
    pub timeline: Vec<local_store::TimelineRow>,
    pub generated_utc: OffsetDateTime,
}

fn export_data(store: &Store, provider: &str, id: &str) -> Result<ExportData, StoreError> {
    let record = store.session(provider, id)?.ok_or(StoreError::NotFound)?;
    Ok(ExportData {
        record,
        context: store.context(provider, id)?,
        items: store.items(provider, id)?,
        links: store.links(provider, id)?,
        timeline: store.timeline(provider, id, usize::MAX >> 1)?,
        generated_utc: OffsetDateTime::now_utc(),
    })
}

fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn longest_backtick_run(s: &str) -> usize {
    let (mut best, mut cur) = (0, 0);
    for c in s.chars() {
        cur = if c == '`' { cur + 1 } else { 0 };
        best = best.max(cur);
    }
    best
}

/// Free text as a fenced code block, with a fence longer than any backtick run inside, so the
/// text can't close the block, start HTML or add headings.
fn fenced(s: &str) -> String {
    let fence = "`".repeat(longest_backtick_run(s).max(2) + 1);
    format!("{fence}text\n{}\n{fence}", s.trim_end_matches('\n'))
}

/// Single-line free text as an inline code span (delimiter longer than any backtick run).
fn code_span(s: &str) -> String {
    let text = one_line(s);
    let delim = "`".repeat(longest_backtick_run(&text) + 1);
    format!("{delim} {text} {delim}")
}

/// Markdown export. Captured text is labelled as coming from the agent.
pub fn render_markdown(d: &ExportData) -> String {
    use std::fmt::Write as _;
    let r = &d.record;
    let mut o = String::new();
    let _ = writeln!(o, "# Lullshift session export\n");
    let _ = writeln!(o, "- Agent: {}", code_span(&r.provider));
    let _ = writeln!(o, "- Session: {}", code_span(&r.provider_session_id));
    let _ = writeln!(o, "- Last reported state: {}", r.state.as_str());
    if let Some(t) = r.state_timestamp_utc {
        let _ = writeln!(o, "- State reported at: {}", ts(t));
    }
    let _ = writeln!(o, "- Last seen: {}", ts(r.last_seen_utc));
    let _ = writeln!(o, "- Exported: {}\n", ts(d.generated_utc));

    let ctx = d.context.clone().unwrap_or_default();
    let _ = writeln!(o, "## Intent\n");
    match (&ctx.user_intent, &ctx.captured_intent) {
        (None, None) => {
            let _ = writeln!(o, "_None recorded._\n");
        }
        (user, captured) => {
            if let Some(t) = user {
                let _ = writeln!(o, "Written by you:\n\n{}\n", fenced(t));
            }
            if let Some(c) = captured {
                let _ = writeln!(
                    o,
                    "Captured from the agent (opt-in), {}:\n\n{}\n",
                    c.captured_utc,
                    fenced(&c.text)
                );
            }
        }
    }
    if let Some(c) = &ctx.ask {
        let _ = writeln!(
            o,
            "## What the agent was asking (captured from the agent, opt-in)\n\n{}\n\nCaptured {}.\n",
            fenced(&c.text),
            c.captured_utc
        );
    }
    let _ = writeln!(o, "## Notes (written by you)\n");
    if ctx.notes.is_empty() {
        let _ = writeln!(o, "_None._\n");
    } else {
        let _ = writeln!(o, "{}\n", fenced(&ctx.notes));
    }
    if let Some(t) = &ctx.reviewed_utc {
        let _ = writeln!(o, "Return card reviewed: {t}\n");
    }

    for (kind, title) in [
        ("decision", "Open decisions"),
        ("checklist", "Review checklist"),
        ("next_move", "Next Move"),
    ] {
        let _ = writeln!(o, "## {title}\n");
        let items: Vec<_> = d.items.iter().filter(|i| i.kind == kind).collect();
        if items.is_empty() {
            let _ = writeln!(o, "_None._\n");
            continue;
        }
        for i in items {
            let mark = if i.done { "x" } else { " " };
            let tag = i
                .category
                .as_deref()
                .map(|c| format!("[{c}] "))
                .unwrap_or_default();
            let _ = writeln!(o, "- [{mark}] {tag}{}", code_span(&i.text));
        }
        let _ = writeln!(o);
    }

    let _ = writeln!(o, "## Linked sessions\n");
    if d.links.is_empty() {
        let _ = writeln!(o, "_None._\n");
    } else {
        for (p, s) in &d.links {
            let _ = writeln!(o, "- {} / {}", code_span(p), code_span(s));
        }
        let _ = writeln!(o);
    }

    let _ = writeln!(o, "## Timeline\n");
    if d.timeline.is_empty() {
        let _ = writeln!(
            o,
            "_No events kept (events are deleted after the retention period)._"
        );
    } else {
        for t in &d.timeline {
            match &t.reason {
                Some(r) => {
                    let _ = writeln!(o, "- {} {} ({r})", t.timestamp_utc, t.event_type);
                }
                None => {
                    let _ = writeln!(o, "- {} {}", t.timestamp_utc, t.event_type);
                }
            }
        }
    }
    o
}

/// JSON export. Captured text sits under `capturedFromAgent`, apart from the user's own text.
pub fn render_json(d: &ExportData) -> Result<String, serde_json::Error> {
    use serde_json::json;
    let r = &d.record;
    let ctx = d.context.clone().unwrap_or_default();
    let cap = |c: &Option<local_store::Captured>| {
        c.as_ref()
            .map(|c| json!({ "text": c.text, "capturedUtc": c.captured_utc }))
    };
    let value = json!({
        "format": "lullshift-session-export",
        "version": 1,
        "generatedUtc": ts(d.generated_utc),
        "session": {
            "provider": r.provider,
            "sessionId": r.provider_session_id,
            "lastReportedState": r.state.as_str(),
            "stateReportedUtc": r.state_timestamp_utc.map(ts),
            "lastSeenUtc": ts(r.last_seen_utc),
        },
        "writtenByYou": {
            "intent": ctx.user_intent,
            "notes": ctx.notes,
        },
        "capturedFromAgent": {
            "note": "Captured only because capture was turned on (ADR-013).",
            "intent": cap(&ctx.captured_intent),
            "ask": cap(&ctx.ask),
        },
        "reviewedUtc": ctx.reviewed_utc,
        "items": d.items.iter().map(|i| json!({
            "kind": i.kind,
            "category": i.category,
            "text": i.text,
            "done": i.done,
            "position": i.position,
            "createdUtc": i.created_utc,
        })).collect::<Vec<_>>(),
        "links": d.links.iter().map(|(p, s)| json!({ "provider": p, "sessionId": s })).collect::<Vec<_>>(),
        "timeline": d.timeline.iter().map(|t| json!({
            "timestampUtc": t.timestamp_utc,
            "eventType": t.event_type,
            "reason": t.reason,
        })).collect::<Vec<_>>(),
    });
    serde_json::to_string_pretty(&value)
}

/// A safe default file name: letters, digits, `-` and `_` only.
pub fn export_file_name(provider: &str, session_id: &str, format: ExportFormat) -> String {
    let safe = |s: &str, max: usize| -> String {
        s.chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .take(max)
            .collect()
    };
    format!(
        "lullshift-{}-{}.{}",
        safe(provider, 24),
        safe(session_id, 8),
        format.extension()
    )
}

/// Writes next to the target and renames over it, so a failed write never leaves a half file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let name = path
        .file_name()
        .map_or_else(|| "export".into(), |n| n.to_string_lossy().into_owned());
    let tmp = path.with_file_name(format!(".{name}.lullshift-tmp"));
    let result = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

// ---- commands ----------------------------------------------------------------------------

type CoreState<'a> = tauri::State<'a, Arc<Core>>;

#[tauri::command]
pub async fn session_detail(
    core: CoreState<'_>,
    provider: String,
    session_id: String,
) -> Result<SessionDetailDto, String> {
    let core = Arc::clone(&core);
    blocking(move || with_store(&core, |s| detail(s, &provider, &session_id))).await
}

#[tauri::command]
pub async fn set_session_notes(
    app: AppHandle,
    core: CoreState<'_>,
    provider: String,
    session_id: String,
    notes: String,
) -> Result<(), String> {
    let core = Arc::clone(&core);
    blocking(move || {
        with_store(&core, |s| {
            s.set_notes(&provider, &session_id, &notes, OffsetDateTime::now_utc())
        })
    })
    .await?;
    notify_changed(&app);
    Ok(())
}

#[tauri::command]
pub async fn set_session_intent(
    app: AppHandle,
    core: CoreState<'_>,
    provider: String,
    session_id: String,
    intent: String,
) -> Result<(), String> {
    let core = Arc::clone(&core);
    blocking(move || {
        with_store(&core, |s| {
            s.set_user_intent(&provider, &session_id, &intent, OffsetDateTime::now_utc())
        })
    })
    .await?;
    notify_changed(&app);
    Ok(())
}

#[tauri::command]
pub async fn add_session_item(
    app: AppHandle,
    core: CoreState<'_>,
    provider: String,
    session_id: String,
    kind: String,
    category: Option<String>,
    text: String,
) -> Result<ItemDto, String> {
    let core = Arc::clone(&core);
    let item = blocking(move || {
        with_store(&core, |s| {
            s.add_item(
                &provider,
                &session_id,
                &kind,
                category.as_deref(),
                &text,
                OffsetDateTime::now_utc(),
            )
        })
    })
    .await?;
    notify_changed(&app);
    Ok(item.into())
}

#[tauri::command]
pub async fn update_session_item(
    core: CoreState<'_>,
    id: i64,
    text: Option<String>,
    done: Option<bool>,
) -> Result<(), String> {
    let core = Arc::clone(&core);
    blocking(move || with_store(&core, |s| s.update_item(id, text.as_deref(), done))).await
}

#[tauri::command]
pub async fn delete_session_item(
    app: AppHandle,
    core: CoreState<'_>,
    id: i64,
) -> Result<(), String> {
    let core = Arc::clone(&core);
    blocking(move || with_store(&core, |s| s.delete_item(id))).await?;
    notify_changed(&app);
    Ok(())
}

#[tauri::command]
pub async fn move_session_item(core: CoreState<'_>, id: i64, position: i64) -> Result<(), String> {
    let core = Arc::clone(&core);
    blocking(move || with_store(&core, |s| s.move_item(id, position))).await
}

#[tauri::command]
pub async fn link_sessions(
    core: CoreState<'_>,
    provider: String,
    session_id: String,
    other_provider: String,
    other_session_id: String,
) -> Result<(), String> {
    let core = Arc::clone(&core);
    blocking(move || {
        with_store(&core, |s| {
            s.link_sessions(
                (&provider, &session_id),
                (&other_provider, &other_session_id),
                OffsetDateTime::now_utc(),
            )
        })
    })
    .await
}

#[tauri::command]
pub async fn unlink_sessions(
    core: CoreState<'_>,
    provider: String,
    session_id: String,
    other_provider: String,
    other_session_id: String,
) -> Result<(), String> {
    let core = Arc::clone(&core);
    blocking(move || {
        with_store(&core, |s| {
            s.unlink_sessions(
                (&provider, &session_id),
                (&other_provider, &other_session_id),
            )
        })
    })
    .await
}

#[tauri::command]
pub async fn mark_reviewed(
    app: AppHandle,
    core: CoreState<'_>,
    provider: String,
    session_id: String,
) -> Result<(), String> {
    let core = Arc::clone(&core);
    blocking(move || {
        with_store(&core, |s| {
            s.mark_reviewed(&provider, &session_id, OffsetDateTime::now_utc())
        })
    })
    .await?;
    notify_changed(&app);
    Ok(())
}

#[tauri::command]
pub async fn clear_captured_context(
    app: AppHandle,
    core: CoreState<'_>,
    provider: String,
    session_id: String,
) -> Result<(), String> {
    let core = Arc::clone(&core);
    blocking(move || with_store(&core, |s| s.clear_captured(&provider, &session_id))).await?;
    notify_changed(&app);
    Ok(())
}

#[tauri::command]
pub async fn clear_all_captured_context(app: AppHandle, core: CoreState<'_>) -> Result<(), String> {
    let core = Arc::clone(&core);
    blocking(move || with_store(&core, |s| s.clear_all_captured())).await?;
    notify_changed(&app);
    Ok(())
}

#[tauri::command]
pub async fn export_session(
    app: AppHandle,
    core: CoreState<'_>,
    provider: String,
    session_id: String,
    format: String,
) -> Result<String, String> {
    let format = ExportFormat::parse(&format).ok_or("invalid_input")?;
    let core = Arc::clone(&core);
    blocking(move || {
        let data = with_store(&core, |s| export_data(s, &provider, &session_id))?;
        let text = match format {
            ExportFormat::Markdown => render_markdown(&data),
            ExportFormat::Json => render_json(&data).map_err(|_| "internal")?,
        };
        // The store lock is released: the dialog blocks until the user answers.
        let (filter_name, ext) = match format {
            ExportFormat::Markdown => ("Markdown", "md"),
            ExportFormat::Json => ("JSON", "json"),
        };
        let mut dialog = app
            .dialog()
            .file()
            .set_title("Export session")
            .set_file_name(export_file_name(&provider, &session_id, format))
            .add_filter(filter_name, &[ext]);
        if let Some(window) = app.get_webview_window("main") {
            dialog = dialog.set_parent(&window);
        }
        let Some(chosen) = dialog.blocking_save_file() else {
            return Ok("cancelled".to_owned());
        };
        let path = chosen.into_path().map_err(|_| "internal")?;
        write_atomic(&path, text.as_bytes()).map_err(|_| "internal")?;
        Ok("saved".to_owned())
    })
    .await
}

#[tauri::command]
pub async fn delete_session(
    app: AppHandle,
    core: CoreState<'_>,
    provider: String,
    session_id: String,
) -> Result<(), String> {
    let core = Arc::clone(&core);
    let core2 = Arc::clone(&core);
    blocking(move || with_store(&core, |s| s.delete_session(&provider, &session_id))).await?;
    notify_changed(&app);
    // The mascot may have been awake for the session that is now gone.
    if let Some(face) = crate::current_face(&core2) {
        crate::set_tray_face(&app, &core2, face);
    }
    Ok(())
}

#[tauri::command]
pub async fn context_prefs(core: CoreState<'_>) -> Result<ContextPrefs, String> {
    let core = Arc::clone(&core);
    blocking(move || {
        with_store(&core, |s| {
            Ok(ContextPrefs::from_stored(
                s.preference(CONTEXT_PREFS_KEY)?.as_deref(),
            ))
        })
    })
    .await
}

/// Saves the settings and keeps the consent marker files in step. Turning capture off removes
/// the marker first, so the forwarder stops sending before anything else happens.
#[tauri::command]
pub async fn set_context_prefs(
    app: AppHandle,
    core: CoreState<'_>,
    prefs: ContextPrefs,
) -> Result<(), String> {
    prefs.validate().map_err(str::to_owned)?;
    let data_dir = app.path().app_data_dir().map_err(|_| "internal")?;
    let core = Arc::clone(&core);
    blocking(move || {
        // One change at a time: two overlapping calls must not interleave marker and setting
        // writes. (A poisoned lock only means an earlier call panicked; the guard is still fine.)
        let _one_at_a_time = PREFS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        with_store(&core, |s| {
            Ok(apply_prefs(
                s,
                &data_dir,
                &prefs,
                remove_markers,
                create_markers,
            ))
        })?
        .map_err(str::to_owned)?;
        // A shorter retention applies right away.
        let _ = purge_now(&core);
        Ok(())
    })
    .await
}

/// Serializes `set_context_prefs`.
static PREFS_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

type MarkerFn = fn(&Path, &ContextPrefs) -> std::io::Result<()>;

/// Applies new settings so that the stored setting and the marker files never disagree:
/// markers of adapters turned off go first (the forwarder stops sending at once), then the
/// setting is saved, then markers of adapters turned on are created. If any step fails, the
/// previous setting and its markers are put back (best effort) and the error is returned.
pub fn apply_prefs(
    store: &Store,
    data_dir: &Path,
    prefs: &ContextPrefs,
    remove: MarkerFn,
    create: MarkerFn,
) -> Result<(), &'static str> {
    let previous = ContextPrefs::from_stored(
        store
            .preference(CONTEXT_PREFS_KEY)
            .map_err(|_| "store_unavailable")?
            .as_deref(),
    );
    let save = |p: &ContextPrefs| -> Result<(), &'static str> {
        let json = serde_json::to_string(p).map_err(|_| "internal")?;
        store
            .set_preference(CONTEXT_PREFS_KEY, &json)
            .map_err(|_| "store_unavailable")
    };
    // Back to the previous state: setting first, then markers to match it.
    let restore = || {
        let _ = save(&previous);
        let _ = remove_markers(data_dir, &previous);
        let _ = create_markers(data_dir, &previous);
    };
    if remove(data_dir, prefs).is_err() {
        restore();
        return Err("internal");
    }
    if let Err(code) = save(prefs) {
        restore();
        return Err(code);
    }
    if create(data_dir, prefs).is_err() {
        restore();
        return Err("internal");
    }
    Ok(())
}

#[tauri::command]
pub async fn ui_prefs(core: CoreState<'_>) -> Result<UiPrefs, String> {
    let core = Arc::clone(&core);
    blocking(move || {
        with_store(&core, |s| {
            Ok(UiPrefs::from_stored(s.preference(UI_PREFS_KEY)?.as_deref()))
        })
    })
    .await
}

#[tauri::command]
pub async fn set_ui_prefs(core: CoreState<'_>, prefs: UiPrefs) -> Result<(), String> {
    let core = Arc::clone(&core);
    blocking(move || {
        let json = serde_json::to_string(&prefs).map_err(|_| "internal")?;
        with_store(&core, |s| s.set_preference(UI_PREFS_KEY, &json))
    })
    .await
}

// ---- tests -------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::parse_event;
    use time::macros::datetime;

    const T0: OffsetDateTime = datetime!(2026-11-05 10:00:00 UTC);
    const P: &str = "claude-code";

    fn ev(n: u32, kind: &str, session: &str) -> AgentEvent {
        parse_event(&format!(
            r#"{{"schemaVersion":1,"eventId":"00000000-0000-4000-8000-{n:012}","provider":"claude-code","providerSessionId":"{session}","eventType":"{kind}","timestampUtc":"2026-11-05T10:00:00Z"}}"#
        ))
        .expect("event")
    }

    fn capture_on_prefs() -> ContextPrefs {
        ContextPrefs {
            retention_days: 30,
            capture: BTreeMap::from([(P.to_owned(), true)]),
        }
    }

    fn set_prefs(store: &Store, prefs: &ContextPrefs) {
        store
            .set_preference(
                CONTEXT_PREFS_KEY,
                &serde_json::to_string(prefs).expect("json"),
            )
            .expect("pref");
    }

    fn ctx(intent: Option<&str>, ask: Option<&str>) -> EventContext {
        EventContext {
            intent: intent.map(str::to_owned),
            ask: ask.map(str::to_owned),
        }
    }

    // -- preferences

    #[test]
    fn retention_must_be_one_of_the_offered_choices() {
        for days in RETENTION_CHOICES {
            let p = ContextPrefs {
                retention_days: days,
                capture: BTreeMap::new(),
            };
            assert!(p.validate().is_ok(), "{days}");
        }
        for days in [0, 1, 8, 29, 31, 366, u32::MAX] {
            let p = ContextPrefs {
                retention_days: days,
                capture: BTreeMap::new(),
            };
            assert_eq!(p.validate(), Err("invalid_input"), "{days}");
        }
    }

    #[test]
    fn only_known_adapters_can_have_capture_settings() {
        let p = ContextPrefs {
            retention_days: 30,
            capture: BTreeMap::from([("../escape".to_owned(), true)]),
        };
        assert_eq!(p.validate(), Err("invalid_input"));
        assert!(capture_on_prefs().validate().is_ok());
    }

    #[test]
    fn prefs_default_to_off_and_30_days_and_ignore_bad_rows() {
        let d = ContextPrefs::from_stored(None);
        assert_eq!((d.retention_days, d.capture_on(P)), (30, false));
        for bad in [
            "not json",
            r#"{"retentionDays":5}"#,
            r#"{"retentionDays":30,"x":1}"#,
        ] {
            assert_eq!(
                ContextPrefs::from_stored(Some(bad)),
                ContextPrefs::default()
            );
        }
        let ok = ContextPrefs::from_stored(Some(
            r#"{"retentionDays":90,"capture":{"claude-code":true}}"#,
        ));
        assert_eq!((ok.retention_days, ok.capture_on(P)), (90, true));
        assert!(
            UiPrefs::from_stored(None).detail_panel_pinned,
            "pinned by default"
        );
        assert!(!UiPrefs::from_stored(Some(r#"{"detailPanelPinned":false}"#)).detail_panel_pinned);
    }

    #[test]
    fn markers_follow_the_setting() {
        let dir = tempfile::tempdir().expect("tempdir");
        let on = capture_on_prefs();
        let off = ContextPrefs::default();
        create_markers(dir.path(), &on).expect("create");
        assert!(marker_path(dir.path(), P).is_file());
        remove_markers(dir.path(), &on).expect("on leaves it");
        assert!(marker_path(dir.path(), P).is_file());
        remove_markers(dir.path(), &off).expect("remove");
        assert!(!marker_path(dir.path(), P).exists());
        remove_markers(dir.path(), &off).expect("idempotent");
        create_markers(dir.path(), &off).expect("off creates nothing");
        assert!(!marker_path(dir.path(), P).exists());
        assert_eq!(
            marker_path(dir.path(), P),
            dir.path().join("context-capture").join("claude-code"),
            "the path the forwarder checks"
        );
    }

    #[test]
    fn prefs_roll_back_when_a_marker_cannot_be_written() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Store::open_in_memory().expect("open");
        let boom: MarkerFn = |_, _| Err(std::io::Error::other("disk"));

        // Turning capture on fails while creating the marker: the setting stays off.
        let on = capture_on_prefs();
        assert_eq!(
            apply_prefs(&store, dir.path(), &on, remove_markers, boom),
            Err("internal")
        );
        let stored =
            ContextPrefs::from_stored(store.preference(CONTEXT_PREFS_KEY).expect("p").as_deref());
        assert!(!stored.capture_on(P), "setting rolled back to off");
        assert!(!marker_path(dir.path(), P).exists(), "and no marker");

        // It works when the file system does.
        apply_prefs(&store, dir.path(), &on, remove_markers, create_markers).expect("apply");
        assert!(marker_path(dir.path(), P).is_file());

        // Turning capture off fails while removing the marker: still on, marker still there.
        let off = ContextPrefs::default();
        assert_eq!(
            apply_prefs(&store, dir.path(), &off, boom, create_markers),
            Err("internal")
        );
        let stored =
            ContextPrefs::from_stored(store.preference(CONTEXT_PREFS_KEY).expect("p").as_deref());
        assert!(stored.capture_on(P));
        assert!(marker_path(dir.path(), P).is_file());

        // Failure after the setting was saved (create fails with new retention): rolled back.
        let changed = ContextPrefs {
            retention_days: 7,
            ..on.clone()
        };
        assert_eq!(
            apply_prefs(&store, dir.path(), &changed, remove_markers, boom),
            Err("internal")
        );
        let stored =
            ContextPrefs::from_stored(store.preference(CONTEXT_PREFS_KEY).expect("p").as_deref());
        assert_eq!(stored, on);
        assert!(marker_path(dir.path(), P).is_file());

        // Turning off for real removes the marker and stores off.
        apply_prefs(&store, dir.path(), &off, remove_markers, create_markers).expect("off");
        assert!(!marker_path(dir.path(), P).exists());
        assert!(
            !ContextPrefs::from_stored(store.preference(CONTEXT_PREFS_KEY).expect("p").as_deref())
                .capture_on(P)
        );
    }

    // -- needsReview

    fn summary(has: bool, reviewed: Option<OffsetDateTime>) -> ContextSummary {
        ContextSummary {
            has_content: has,
            reviewed_utc: reviewed,
        }
    }

    #[test]
    fn needs_review_rules() {
        let end = Some(T0);
        let s = |has, rev| summary(has, rev);
        for state in [
            SessionState::Completed,
            SessionState::Failed,
            SessionState::Cancelled,
            SessionState::Ended,
        ] {
            assert!(needs_review(state, end, Some(&s(true, None))), "{state:?}");
        }
        for state in [
            SessionState::Unknown,
            SessionState::Idle,
            SessionState::Working,
            SessionState::NeedsAttention,
        ] {
            assert!(!needs_review(state, end, Some(&s(true, None))), "{state:?}");
        }
        let done = SessionState::Completed;
        assert!(!needs_review(done, end, None), "no context at all");
        assert!(
            !needs_review(done, end, Some(&s(false, None))),
            "empty context"
        );
        assert!(
            !needs_review(
                done,
                end,
                Some(&s(true, Some(T0 + time::Duration::minutes(1))))
            ),
            "reviewed after the state"
        );
        assert!(
            needs_review(
                done,
                end,
                Some(&s(true, Some(T0 - time::Duration::minutes(1))))
            ),
            "state is newer than the review"
        );
        assert!(
            !needs_review(done, None, Some(&s(true, Some(T0)))),
            "reviewed, no state time"
        );
    }

    #[test]
    fn dashboard_flag_comes_from_the_stored_summary() {
        let mut store = Store::open_in_memory().expect("open");
        store.ingest(&ev(1, "completed", "a"), T0).expect("ingest");
        store.ingest(&ev(2, "completed", "b"), T0).expect("ingest");
        store.set_notes(P, "a", "x", T0).expect("notes");
        let summaries = store.context_summaries().expect("summaries");
        let rows: Vec<_> = store
            .sessions()
            .expect("sessions")
            .into_iter()
            .map(|r| {
                let v = agent_core::view(&r, T0, T0, time::Duration::minutes(10));
                (
                    r.provider_session_id.clone(),
                    dashboard_session(v, &r, &summaries).needs_review,
                )
            })
            .collect();
        assert!(rows.contains(&("a".to_owned(), true)));
        assert!(rows.contains(&("b".to_owned(), false)));
        let json = serde_json::to_value(dashboard_session(
            agent_core::view(
                &store.sessions().expect("s")[0],
                T0,
                T0,
                time::Duration::minutes(10),
            ),
            &store.sessions().expect("s")[0],
            &summaries,
        ))
        .expect("json");
        assert!(json.get("needsReview").is_some() && json.get("providerSessionId").is_some());
    }

    // -- capture gates (S04-A04)

    #[test]
    fn capture_off_stores_nothing_even_if_text_arrives() {
        let mut store = Store::open_in_memory().expect("open");
        // Gate two: no stored setting at all.
        let out = ingest_with_context(
            &mut store,
            &ev(1, "working", "s"),
            Some(ctx(Some("SENTINEL_PROMPT"), Some("SENTINEL_ASK"))),
            T0,
        )
        .expect("ingest");
        assert!(
            matches!(out, IngestOutcome::Stored { .. }),
            "the event itself is kept"
        );
        assert!(
            store.context(P, "s").expect("ctx").is_none(),
            "no context row"
        );

        // Explicitly off.
        set_prefs(&store, &ContextPrefs::default());
        ingest_with_context(
            &mut store,
            &ev(2, "working", "s"),
            Some(ctx(Some("SENTINEL_PROMPT"), None)),
            T0,
        )
        .expect("ingest");
        assert!(store.context(P, "s").expect("ctx").is_none());

        // Off for this adapter while another is on.
        let other = ContextPrefs {
            retention_days: 30,
            capture: BTreeMap::from([(P.to_owned(), false)]),
        };
        set_prefs(&store, &other);
        assert!(!capture(&mut store, P, "s", &ctx(Some("SENTINEL"), None), T0).expect("capture"));
    }

    #[test]
    fn capture_on_stores_cleaned_text_with_intent_rules() {
        let mut store = Store::open_in_memory().expect("open");
        set_prefs(&store, &capture_on_prefs());
        ingest_with_context(
            &mut store,
            &ev(1, "working", "s"),
            Some(ctx(Some("first prompt password=hunter2"), None)),
            T0,
        )
        .expect("ingest");
        ingest_with_context(
            &mut store,
            &ev(2, "needs_attention", "s"),
            Some(ctx(Some("second prompt"), Some("Permission to use Bash"))),
            T0 + time::Duration::minutes(1),
        )
        .expect("ingest");
        let c = store.context(P, "s").expect("ctx").expect("row");
        assert_eq!(
            c.captured_intent.clone().expect("intent").text,
            "first prompt password=[hidden]",
            "first prompt stays the intent, secrets hidden again in the app"
        );
        assert!(
            !format!("{c:?}").contains("second prompt"),
            "later prompts are not kept"
        );
        assert_eq!(c.ask.expect("ask").text, "Permission to use Bash");
    }

    #[test]
    fn app_enforces_limits_and_cleaning_on_arriving_text() {
        let mut store = Store::open_in_memory().expect("open");
        set_prefs(&store, &capture_on_prefs());
        let long = "word ".repeat(1000);
        ingest_with_context(
            &mut store,
            &ev(1, "working", "s"),
            Some(ctx(Some(&format!("\u{202e}{long}")), Some(&long))),
            T0,
        )
        .expect("ingest");
        let c = store.context(P, "s").expect("ctx").expect("row");
        let i = c.captured_intent.expect("intent").text;
        assert!(i.chars().count() <= 2000 && !i.contains('\u{202e}'));
        assert!(c.ask.expect("ask").text.chars().count() <= 300);
    }

    #[test]
    fn duplicate_events_do_not_capture_again() {
        let mut store = Store::open_in_memory().expect("open");
        set_prefs(&store, &capture_on_prefs());
        let e = ev(1, "working", "s");
        ingest_with_context(&mut store, &e, Some(ctx(Some("one"), None)), T0).expect("ingest");
        ingest_with_context(&mut store, &e, Some(ctx(Some("two"), None)), T0).expect("ingest");
        let c = store.context(P, "s").expect("ctx").expect("row");
        assert_eq!(c.captured_intent.expect("intent").text, "one");
    }

    #[test]
    fn event_log_never_contains_captured_text_even_with_capture_on() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("l.db");
        let (mut store, _) = Store::open(&path).expect("open");
        set_prefs(&store, &capture_on_prefs());
        for (n, kind) in [(1, "working"), (2, "needs_attention"), (3, "turn_ended")] {
            ingest_with_context(
                &mut store,
                &ev(n, kind, "s"),
                Some(ctx(Some("SENTINEL_PROMPT_TEXT"), Some("SENTINEL_ASK_TEXT"))),
                T0,
            )
            .expect("ingest");
        }
        assert!(
            store
                .context(P, "s")
                .expect("ctx")
                .expect("row")
                .has_captured()
        );
        // The timeline (what History and export read) holds types and reasons only.
        let timeline = format!("{:?}", store.timeline(P, "s", 500).expect("timeline"));
        assert!(!timeline.contains("SENTINEL"));
        // Every byte of every event row, read with a separate connection.
        let conn = rusqlite::Connection::open(&path).expect("raw");
        let mut stmt = conn
            .prepare(
                "SELECT event_json || event_type || provider || provider_session_id FROM events",
            )
            .expect("prepare");
        let rows: Vec<String> = stmt
            .query_map([], |r| r.get(0))
            .expect("query")
            .map(|r| r.expect("row"))
            .collect();
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().all(|r| !r.contains("SENTINEL")));
    }

    // -- retention

    #[test]
    fn retention_cutoff_is_days_back() {
        assert_eq!(retention_cutoff(T0, 30), T0 - time::Duration::days(30));
        assert_eq!(retention_cutoff(T0, 365), T0 - time::Duration::days(365));
    }

    // -- export

    fn sample_data() -> ExportData {
        let mut store = Store::open_in_memory().expect("open");
        set_prefs(&store, &capture_on_prefs());
        ingest_with_context(
            &mut store,
            &ev(1, "needs_attention", "sess-1"),
            Some(ctx(Some("CAPTURED intent"), Some("CAPTURED ask"))),
            T0,
        )
        .expect("ingest");
        ingest_with_context(&mut store, &ev(2, "completed", "sess-2"), None, T0).expect("ingest");
        store
            .set_user_intent(P, "sess-1", "USER intent", T0)
            .expect("intent");
        store
            .set_notes(P, "sess-1", "line one\nline two", T0)
            .expect("notes");
        store
            .add_item(P, "sess-1", "decision", None, "Which db?", T0)
            .expect("item");
        let done = store
            .add_item(P, "sess-1", "checklist", None, "Run tests\nall of them", T0)
            .expect("item");
        store.update_item(done.id, None, Some(true)).expect("done");
        store
            .add_item(
                P,
                "sess-1",
                "next_move",
                Some("test"),
                "Try empty input",
                T0,
            )
            .expect("item");
        store
            .link_sessions((P, "sess-1"), (P, "sess-2"), T0)
            .expect("link");
        export_data(&store, P, "sess-1").expect("data")
    }

    #[test]
    fn markdown_export_has_all_parts_and_labels_captured_text() {
        let md = render_markdown(&sample_data());
        for want in [
            "# Lullshift session export",
            "Written by you:\n\n```text\nUSER intent\n```",
            "Captured from the agent (opt-in)",
            "```text\nCAPTURED intent\n```",
            "(captured from the agent, opt-in)",
            "```text\nCAPTURED ask\n```",
            "```text\nline one\nline two\n```",
            "- [ ] ` Which db? `",
            "- [x] ` Run tests all of them `",
            "- [ ] [test] ` Try empty input `",
            "- ` claude-code ` / ` sess-2 `",
            "needs_attention",
            "## Timeline",
        ] {
            assert!(md.contains(want), "missing {want:?} in:\n{md}");
        }
    }

    #[test]
    fn markdown_export_cannot_be_injected_through_free_text() {
        let mut store = Store::open_in_memory().expect("open");
        set_prefs(&store, &capture_on_prefs());
        let evil = "<img src=x onerror=alert(1)>\n# Heading\n```\nclosed early?\n````\n[x](javascript:alert(1))";
        ingest_with_context(
            &mut store,
            &ev(1, "needs_attention", "s"),
            Some(ctx(Some(evil), Some("<b>ask</b> ``` # not a heading"))),
            T0,
        )
        .expect("ingest");
        store.set_user_intent(P, "s", evil, T0).expect("intent");
        store.set_notes(P, "s", evil, T0).expect("notes");
        store
            .add_item(P, "s", "decision", None, "<img src=y> `` # h ``", T0)
            .expect("item");
        let md = render_markdown(&export_data(&store, P, "s").expect("data"));
        // Walk the document: inside a fence, only a line of exactly the fence closes it; outside
        // a fence, no raw HTML, link or heading from the free text appears.
        let mut open: Option<usize> = None;
        for line in md.lines() {
            let ticks = line.chars().take_while(|c| *c == '`').count();
            match open {
                Some(n) if ticks == n && line.len() == n => open = None,
                Some(n) => assert!(
                    !(ticks >= n && line.len() == ticks),
                    "a line inside a fence closes it: {line}"
                ),
                None if ticks >= 3 && line[ticks..].chars().all(|c| c.is_ascii_alphabetic()) => {
                    open = Some(ticks);
                }
                None if line.starts_with("- [") => {} // list items are inline code spans (checked below)
                None => {
                    assert!(!line.contains("<img") && !line.contains("<b>"), "{line}");
                    assert!(!line.contains("](javascript:"), "{line}");
                    assert!(!line.starts_with("# Heading"), "{line}");
                    assert!(!line.starts_with("# not a"), "{line}");
                }
            }
        }
        assert!(open.is_none(), "all fences are closed:\n{md}");
        // Text holding a four-backtick run gets a fence of at least five.
        assert!(md.contains("`````text"), "{md}");
        // Item text is an inline code span, not live markdown.
        assert!(md.contains("- [ ] ``` <img src=y> `` # h `` ```"), "{md}");
    }

    #[test]
    fn json_export_separates_user_and_captured_text() {
        let json: serde_json::Value =
            serde_json::from_str(&render_json(&sample_data()).expect("json")).expect("parse");
        assert_eq!(json["format"], "lullshift-session-export");
        assert_eq!(json["writtenByYou"]["intent"], "USER intent");
        assert_eq!(json["writtenByYou"]["notes"], "line one\nline two");
        assert_eq!(
            json["capturedFromAgent"]["intent"]["text"],
            "CAPTURED intent"
        );
        assert_eq!(json["capturedFromAgent"]["ask"]["text"], "CAPTURED ask");
        assert!(json["capturedFromAgent"].get("latestRequest").is_none());
        assert!(json["capturedFromAgent"]["ask"]["capturedUtc"].is_string());
        assert_eq!(json["items"].as_array().expect("items").len(), 3);
        assert_eq!(json["links"][0]["sessionId"], "sess-2");
        assert_eq!(json["timeline"][0]["eventType"], "needs_attention");
    }

    #[test]
    fn export_without_context_says_so() {
        let mut store = Store::open_in_memory().expect("open");
        store.ingest(&ev(1, "working", "s"), T0).expect("ingest");
        let data = export_data(&store, P, "s").expect("data");
        let md = render_markdown(&data);
        assert!(md.contains("_None recorded._") && !md.contains("captured from the agent"));
        let json: serde_json::Value =
            serde_json::from_str(&render_json(&data).expect("json")).expect("parse");
        assert!(json["capturedFromAgent"]["intent"].is_null());
        assert!(matches!(
            export_data(&store, P, "missing"),
            Err(StoreError::NotFound)
        ));
    }

    #[test]
    fn export_file_names_are_safe_and_atomic_write_replaces() {
        assert_eq!(
            export_file_name("claude-code", "../../etc/passwd", ExportFormat::Markdown),
            "lullshift-claude-code-etcpassw.md"
        );
        assert_eq!(
            export_file_name("p", "abc", ExportFormat::Json),
            "lullshift-p-abc.json"
        );
        assert_eq!(
            ExportFormat::parse("markdown"),
            Some(ExportFormat::Markdown)
        );
        assert_eq!(ExportFormat::parse("pdf"), None);

        let dir = tempfile::tempdir().expect("tempdir");
        let target = dir.path().join("out.md");
        std::fs::write(&target, "old").expect("write");
        write_atomic(&target, b"new").expect("atomic");
        assert_eq!(std::fs::read_to_string(&target).expect("read"), "new");
        assert_eq!(
            std::fs::read_dir(dir.path()).expect("dir").count(),
            1,
            "no temp left"
        );
        // A failed write leaves the target and no temp file behind.
        let bad = dir.path().join("no-such-dir").join("x.md");
        assert!(write_atomic(&bad, b"x").is_err());
    }

    #[test]
    fn detail_reports_user_intent_first_then_captured() {
        let mut store = Store::open_in_memory().expect("open");
        set_prefs(&store, &capture_on_prefs());
        ingest_with_context(
            &mut store,
            &ev(1, "working", "s"),
            Some(ctx(Some("captured"), None)),
            T0,
        )
        .expect("ingest");
        let d = detail(&store, P, "s").expect("detail");
        let json = serde_json::to_value(&d).expect("json");
        assert_eq!(json["context"]["intent"]["source"], "agent");
        assert_eq!(json["context"]["hasCaptured"], true);
        assert_eq!(json["retentionDays"], 30);
        store.set_user_intent(P, "s", "mine", T0).expect("intent");
        let json = serde_json::to_value(detail(&store, P, "s").expect("detail")).expect("json");
        assert_eq!(json["context"]["intent"]["source"], "user");
        assert_eq!(json["context"]["intent"]["text"], "mine");
        assert!(matches!(
            detail(&store, P, "nope"),
            Err(StoreError::NotFound)
        ));
        // No row yet: empty but well-formed.
        store
            .ingest(&ev(5, "working", "fresh"), T0)
            .expect("ingest");
        let json = serde_json::to_value(detail(&store, P, "fresh").expect("detail")).expect("json");
        assert_eq!(json["context"]["notes"], "");
        assert!(json["context"]["intent"].is_null() && json["items"] == serde_json::json!([]));
    }

    #[test]
    fn store_errors_map_to_the_short_codes() {
        assert_eq!(code(&StoreError::NotFound), "not_found");
        assert_eq!(code(&StoreError::InvalidInput("x")), "invalid_input");
        assert_eq!(code(&StoreError::TooMany { limit: 1 }), "too_many");
        assert_eq!(code(&StoreError::Corrupt("x")), "store_unavailable");
    }
}
