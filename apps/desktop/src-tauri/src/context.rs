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
use agent_adapters::{codex, cursor, opencode};
use agent_core::{AgentEvent, SessionRecord, SessionState, SessionView};
use agent_ipc::{EventContext, MAX_ASK_CHARS, MAX_INTENT_CHARS, MAX_TITLE_CHARS};
use local_store::{
    ContextRecord, ContextSummary, IngestOutcome, Item, SessionLabels, Store, StoreError,
};
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
pub const CAPTURE_ADAPTERS: [&str; 4] = [
    ADAPTER_ID,
    codex::ADAPTER_ID,
    cursor::ADAPTER_ID,
    opencode::ADAPTER_ID,
];
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

/// How wide the main content may grow. Full width is the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContentWidth {
    #[default]
    Full,
    Comfortable,
}

/// The app theme (ADR-026 section 5). System follows the operating system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

/// What a left click on the tray icon opens (ADR-026 section 6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrayClick {
    /// The small tray window.
    #[default]
    Quick,
    /// The main window, as before.
    Main,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UiPrefs {
    pub detail_panel_pinned: bool,
    /// Missing in preferences saved before it existed; those read as full width.
    #[serde(default)]
    pub content_width: ContentWidth,
    /// The first-launch welcome was shown and closed (or skipped). Missing in older preferences,
    /// which read as not yet shown.
    #[serde(default)]
    pub onboarding_done: bool,
    /// The Companion section of Settings is expanded (decision 36). Missing in older preferences,
    /// which read as collapsed.
    #[serde(default)]
    pub companion_settings_open: bool,
    /// Width in pixels of the docked session panel; the app keeps it within the window. Missing
    /// (or null) in older preferences, which read as the default width.
    #[serde(default)]
    pub dock_width: Option<u32>,
    /// Missing in older preferences, which read as System.
    #[serde(default)]
    pub theme: Theme,
    /// Missing in older preferences, which read as the quick window.
    #[serde(default)]
    pub tray_click: TrayClick,
}

impl Default for UiPrefs {
    fn default() -> Self {
        Self {
            detail_panel_pinned: true,
            content_width: ContentWidth::Full,
            onboarding_done: false,
            companion_settings_open: false,
            dock_width: None,
            theme: Theme::System,
            tray_click: TrayClick::Quick,
        }
    }
}

/// The docked panel is kept within these widths (pixels) when read and when saved.
pub const DOCK_WIDTH_RANGE: std::ops::RangeInclusive<u32> = 200..=4096;

fn clamp_dock_width(px: u32) -> u32 {
    px.clamp(*DOCK_WIDTH_RANGE.start(), *DOCK_WIDTH_RANGE.end())
}

impl UiPrefs {
    /// Reads each field on its own: a missing or damaged field, or a value this build doesn't
    /// know, takes its default and never resets the others (so a downgrade or a newer build's
    /// value cannot bring the welcome back). Unknown keys are ignored. Writes from the UI stay
    /// strict.
    pub fn from_stored(json: Option<&str>) -> Self {
        let Some(serde_json::Value::Object(map)) =
            json.and_then(|j| serde_json::from_str::<serde_json::Value>(j).ok())
        else {
            return Self::default();
        };
        fn field<T: serde::de::DeserializeOwned>(
            map: &serde_json::Map<String, serde_json::Value>,
            key: &str,
        ) -> Option<T> {
            map.get(key)
                .and_then(|v| serde_json::from_value(v.clone()).ok())
        }
        let d = Self::default();
        Self {
            detail_panel_pinned: field(&map, "detailPanelPinned").unwrap_or(d.detail_panel_pinned),
            content_width: field(&map, "contentWidth").unwrap_or(d.content_width),
            onboarding_done: field(&map, "onboardingDone").unwrap_or(d.onboarding_done),
            companion_settings_open: field(&map, "companionSettingsOpen")
                .unwrap_or(d.companion_settings_open),
            // Fractional, negative or non-numeric widths read as the default width.
            dock_width: field::<u32>(&map, "dockWidth").map(clamp_dock_width),
            theme: field(&map, "theme").unwrap_or(d.theme),
            tray_click: field(&map, "trayClick").unwrap_or(d.tray_click),
        }
    }

    /// The same preferences with the panel width kept in range.
    pub fn clamped(mut self) -> Self {
        self.dock_width = self.dock_width.map(clamp_dock_width);
        self
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
#[cfg(test)]
pub fn ingest_with_context(
    store: &mut Store,
    event: &AgentEvent,
    context: Option<EventContext>,
    received: OffsetDateTime,
) -> Result<IngestOutcome, StoreError> {
    ingest_with_process(store, event, context, None, received)
}

/// [`ingest_with_context`] that also records the agent's process for the process check
/// (decision 35, ADR-025).
pub fn ingest_with_process(
    store: &mut Store,
    event: &AgentEvent,
    context: Option<EventContext>,
    process: Option<local_store::ProcessKey>,
    received: OffsetDateTime,
) -> Result<IngestOutcome, StoreError> {
    let outcome = store.ingest_with_process(event, received, process)?;
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
    if let Some(title) = ctx.title.as_deref()
        && let Some(text) = clean_title(title)
    {
        store.record_agent_title(provider, session, &text, now)?;
        stored = true;
    }
    Ok(stored)
}

/// A session title as stored: cleaned like captured text, collapsed to one line and cut to
/// [`MAX_TITLE_CHARS`] characters (after redaction, so a cut can't expose half a secret).
/// `None` if nothing is left.
pub fn clean_title(raw: &str) -> Option<String> {
    let cleaned = redact::clean(raw, MAX_INTENT_CHARS);
    let line = one_line(&cleaned);
    let cut: String = line.chars().take(MAX_TITLE_CHARS).collect();
    let cut = cut.trim_end().to_owned();
    (!cut.is_empty()).then_some(cut)
}

/// A name typed by the user: plain text, trimmed, at most [`MAX_TITLE_CHARS`] characters, no
/// control, bidi or invisible characters. Empty is allowed (it clears the name).
pub fn validate_user_title(raw: &str) -> Result<String, &'static str> {
    let t = raw.trim();
    if t.chars().count() > MAX_TITLE_CHARS
        || t.chars().any(char::is_control)
        || redact::strip_unsafe(t) != t
    {
        return Err("invalid_input");
    }
    Ok(t.to_owned())
}

// ---- dashboard review flag ---------------------------------------------------------------

/// A dashboard session: the derived view plus whether its return card is waiting.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardSession {
    #[serde(flatten)]
    pub view: SessionView,
    pub needs_review: bool,
    /// A name the user gave the session.
    pub user_title: Option<String>,
    /// The agent's own title (captured with consent).
    pub agent_title: Option<String>,
    /// First line of the captured intent, for the title fallback.
    pub intent_line: Option<String>,
    /// Set when the session's tool was disconnected and the session sent nothing since: it is
    /// no longer tracked. `view` keeps the last reported state (S05-T10).
    #[serde(with = "time::serde::rfc3339::option")]
    pub disconnected_utc: Option<OffsetDateTime>,
    /// Set when the user imported the session from the agent's own log and no hook event has
    /// arrived since (ADR-024): `view.last_known_state` is a guess, not an observation.
    #[serde(with = "time::serde::rfc3339::option")]
    pub imported_utc: Option<OffsetDateTime>,
    /// Set (to `process_gone`) when Lullshift, not the agent, ended the session because the
    /// agent's process is no longer running (ADR-025). `view.state` is then `ended`.
    pub ended_reason: Option<String>,
}

/// Everything the dashboard joins onto the session list. A failed read of any part only hides
/// that part; the list still shows.
#[derive(Default)]
pub struct DashboardLookups {
    pub summaries: HashMap<(String, String), ContextSummary>,
    pub labels: HashMap<(String, String), SessionLabels>,
    pub disconnected: HashMap<(String, String), OffsetDateTime>,
    pub imported: HashMap<(String, String), OffsetDateTime>,
    pub ended_reasons: HashMap<(String, String), String>,
}

impl DashboardLookups {
    pub fn load(store: &Store) -> Self {
        Self {
            summaries: store.context_summaries().unwrap_or_default(),
            labels: store.session_labels().unwrap_or_default(),
            disconnected: store.disconnected_sessions().unwrap_or_default(),
            imported: store.imported_sessions().unwrap_or_default(),
            ended_reasons: store.ended_reasons().unwrap_or_default(),
        }
    }
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
    lookups: &DashboardLookups,
) -> DashboardSession {
    let key = (record.provider.clone(), record.provider_session_id.clone());
    let labels = lookups.labels.get(&key).cloned().unwrap_or_default();
    let disconnected_utc = lookups.disconnected.get(&key).copied();
    let imported_utc = lookups.imported.get(&key).copied();
    DashboardSession {
        // A session that is no longer (or not yet) tracked has no return card waiting either.
        needs_review: disconnected_utc.is_none()
            && imported_utc.is_none()
            && needs_review(
                view.state,
                record.state_timestamp_utc,
                lookups.summaries.get(&key),
            ),
        view,
        user_title: labels.user_title,
        agent_title: labels.agent_title,
        intent_line: labels.intent_line,
        disconnected_utc,
        imported_utc,
        // Only an `ended` session carries it: a stale reason beside any other state is ignored.
        ended_reason: lookups
            .ended_reasons
            .get(&key)
            .filter(|_| record.state == SessionState::Ended)
            .cloned(),
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
    let cutoff = retention_cutoff(OffsetDateTime::now_utc(), prefs.retention_days);
    // Companion lookup results that outlived the retention period go with the events.
    let _ = store.companion_clear_overview_results(Some(cutoff));
    store.purge_events_before(cutoff).ok()
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
    /// A name the user gave the session.
    user_title: Option<String>,
    /// The agent's own title, captured with consent.
    agent_title: Option<CapturedDto>,
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
            user_title: None,
            agent_title: None,
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
        user_title: rec.user_title.clone(),
        agent_title: captured_dto(rec.agent_title.as_ref()),
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

/// One session's own text for the Notes view (ADR-026 section 7).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotesSessionDto {
    provider: String,
    session_id: String,
    last_received_utc: String,
    user_title: Option<String>,
    agent_title: Option<String>,
    intent: Option<String>,
    notes: String,
    checklist: Vec<ItemDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotesOverviewDto {
    sessions: Vec<NotesSessionDto>,
    /// Older sessions were left out by the size caps.
    truncated: bool,
}

/// Everything the user wrote across sessions, bounded by the store's caps. Reads only.
fn notes_overview_dto(store: &Store) -> Result<NotesOverviewDto, StoreError> {
    let o = store.notes_overview()?;
    Ok(NotesOverviewDto {
        truncated: o.truncated,
        sessions: o
            .entries
            .into_iter()
            .map(|e| NotesSessionDto {
                provider: e.provider,
                session_id: e.session_id,
                last_received_utc: ts(e.last_received_utc),
                user_title: e.user_title,
                agent_title: e.agent_title,
                intent: e.intent,
                notes: e.notes,
                checklist: e.checklist.into_iter().map(ItemDto::from).collect(),
            })
            .collect(),
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
    /// Set while the session is still only imported from the agent's log (decision 34): its
    /// state is a guess, not a report.
    pub imported_utc: Option<OffsetDateTime>,
    /// `process_gone` while Lullshift itself ended the session because the agent's process is
    /// gone (ADR-025): the `ended` state was inferred, not reported.
    pub ended_reason: Option<String>,
    /// Set while the state was filled in from the agent's log at start-up by the opt-in catch-up
    /// (ADR-025) and the agent has not reported since. Also counted in `imported_utc`.
    pub log_hint_utc: Option<OffsetDateTime>,
}

impl ExportData {
    /// Lullshift itself ended the session because the agent's program is gone: both the state
    /// and the reason must say so.
    fn ended_by_process_gone(&self) -> bool {
        self.record.state == SessionState::Ended
            && self.ended_reason.as_deref() == Some(agent_core::reducer::ENDED_PROCESS_GONE)
    }
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
        imported_utc: store
            .imported_sessions()?
            .get(&(provider.to_owned(), id.to_owned()))
            .copied(),
        ended_reason: store
            .ended_reasons()?
            .get(&(provider.to_owned(), id.to_owned()))
            .cloned(),
        log_hint_utc: store
            .log_hinted_sessions()?
            .get(&(provider.to_owned(), id.to_owned()))
            .copied(),
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
    let ctx = d.context.clone().unwrap_or_default();
    if let Some(t) = &ctx.user_title {
        let _ = writeln!(o, "- Title (written by you): {}", code_span(t));
    }
    if let Some(t) = &ctx.agent_title {
        let _ = writeln!(
            o,
            "- Title (captured from the agent, opt-in): {}",
            code_span(&t.text)
        );
    }
    if let Some(at) = d.log_hint_utc {
        let _ = writeln!(
            o,
            "- State guessed from the agent's log (filled in at start-up {}; the agent has not reported since): {}",
            ts(at),
            r.state.as_str()
        );
    } else if let Some(at) = d.imported_utc {
        let _ = writeln!(
            o,
            "- State guessed from the agent's log (imported {}; the agent has not reported since): {}",
            ts(at),
            r.state.as_str()
        );
    } else if d.ended_by_process_gone() {
        let _ = writeln!(
            o,
            "- State: ended (inferred by Lullshift because the agent's process is gone; the agent did not report it)"
        );
    } else {
        let _ = writeln!(o, "- Last reported state: {}", r.state.as_str());
    }
    if let Some(t) = r.state_timestamp_utc {
        let _ = writeln!(o, "- State reported at: {}", ts(t));
    }
    let _ = writeln!(o, "- Last seen: {}", ts(r.last_seen_utc));
    let _ = writeln!(o, "- Exported: {}\n", ts(d.generated_utc));

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
            // "reported" by the agent, or "guessed_from_log" for a session only imported from
            // the agent's log (decision 34).
            // "process_gone" when Lullshift ended it because the agent's process is gone
            // (ADR-025): inferred, not reported.
            "stateSource": if d.imported_utc.is_some() {
                "guessed_from_log"
            } else if d.ended_by_process_gone() {
                "inferred_process_gone"
            } else {
                "reported"
            },
            "importedFromLogUtc": d.imported_utc.filter(|_| d.log_hint_utc.is_none()).map(ts),
            "filledInFromLogUtc": d.log_hint_utc.map(ts),
            "stateReportedUtc": r.state_timestamp_utc.map(ts),
            "lastSeenUtc": ts(r.last_seen_utc),
        },
        "writtenByYou": {
            "title": ctx.user_title,
            "intent": ctx.user_intent,
            "notes": ctx.notes,
        },
        "capturedFromAgent": {
            "note": "Captured only because capture was turned on (ADR-013).",
            "title": cap(&ctx.agent_title),
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

/// The Notes view: every session's intent, checklist and notes in one read (main window only).
#[tauri::command]
pub async fn notes_overview(core: CoreState<'_>) -> Result<NotesOverviewDto, String> {
    let core = Arc::clone(&core);
    blocking(move || with_store(&core, |s| notes_overview_dto(s))).await
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

/// Names a session. Plain text, trimmed, at most 120 characters; empty removes the name.
#[tauri::command]
pub async fn set_session_title(
    app: AppHandle,
    core: CoreState<'_>,
    provider: String,
    session_id: String,
    title: String,
) -> Result<(), String> {
    let title = validate_user_title(&title).map_err(str::to_owned)?;
    let core = Arc::clone(&core);
    blocking(move || {
        with_store(&core, |s| {
            s.set_user_title(&provider, &session_id, &title, OffsetDateTime::now_utc())
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
    blocking(move || {
        with_store(&core, |s| {
            s.clear_captured(&provider, &session_id)?;
            // Which Companion lookups mentioned this session is not recorded, so all stored
            // overview text and attached session summaries are cleared.
            s.companion_clear_overview_results(None)?;
            s.companion_clear_attached_session_context()?;
            Ok(())
        })
    })
    .await?;
    notify_changed(&app);
    Ok(())
}

#[tauri::command]
pub async fn clear_all_captured_context(app: AppHandle, core: CoreState<'_>) -> Result<(), String> {
    let core = Arc::clone(&core);
    blocking(move || {
        with_store(&core, |s| {
            s.clear_all_captured()?;
            s.companion_clear_overview_results(None)?;
            s.companion_clear_attached_session_context()?;
            Ok(())
        })
    })
    .await?;
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
    // Capture was turned off for some tool: Companion text derived from captured task text
    // (approved session overview results, session summaries attached to messages) goes too.
    // Best effort: the setting itself has already been saved and the markers are in step.
    if previous
        .capture
        .iter()
        .any(|(adapter, on)| *on && !prefs.capture_on(adapter))
    {
        let _ = store.companion_clear_overview_results(None);
        let _ = store.companion_clear_attached_session_context();
    }
    Ok(())
}

/// Whether the store knows this session (the tray's "Go to session" is checked with it).
pub fn session_exists(core: &Core, provider: &str, session_id: &str) -> Result<bool, String> {
    with_store(core, |s| s.session(provider, session_id)).map(|r| r.is_some())
}

/// The saved interface preferences; the defaults if they can't be read (the tray click handler
/// must always do something sensible).
pub fn read_ui_prefs(core: &Core) -> UiPrefs {
    with_store(core, |s| s.preference(UI_PREFS_KEY))
        .map(|json| UiPrefs::from_stored(json.as_deref()))
        .unwrap_or_default()
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
        let json = serde_json::to_string(&prefs.clamped()).map_err(|_| "internal")?;
        with_store(&core, |s| s.set_preference(UI_PREFS_KEY, &json))?;
        // Only once saved: the tray icon's click handler reads this, not the database.
        crate::tray_window::set_quick(prefs.tray_click);
        Ok(())
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
            title: None,
        }
    }

    fn title_ctx(title: &str) -> EventContext {
        EventContext {
            title: Some(title.to_owned()),
            ..EventContext::default()
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
        assert_eq!(
            UiPrefs::from_stored(Some(r#"{"detailPanelPinned":false}"#)).content_width,
            ContentWidth::Full,
            "older saved prefs read as full width"
        );
        assert_eq!(
            UiPrefs::from_stored(Some(
                r#"{"detailPanelPinned":true,"contentWidth":"comfortable"}"#
            ))
            .content_width,
            ContentWidth::Comfortable
        );
        assert_eq!(
            UiPrefs::from_stored(Some(r#"{"detailPanelPinned":true,"contentWidth":"huge"}"#)),
            UiPrefs::default(),
            "unknown widths fall back to the defaults"
        );
        // Migration: preferences saved before the welcome existed have no flag and read as not
        // yet shown; the flag round-trips; a damaged value falls back to the defaults.
        assert!(
            !UiPrefs::from_stored(Some(r#"{"detailPanelPinned":false,"contentWidth":"full"}"#))
                .onboarding_done
        );
        let done = UiPrefs {
            onboarding_done: true,
            ..UiPrefs::default()
        };
        let json = serde_json::to_string(&done).expect("serialize");
        assert_eq!(UiPrefs::from_stored(Some(&json)), done);
        assert_eq!(
            UiPrefs::from_stored(Some(r#"{"detailPanelPinned":true,"onboardingDone":"yes"}"#)),
            UiPrefs::default(),
            "a damaged flag falls back to the defaults"
        );
        // The docked panel width: older preferences have none; a saved one round-trips; a
        // negative value falls back to the defaults.
        assert_eq!(
            UiPrefs::from_stored(Some(r#"{"detailPanelPinned":true}"#)).dock_width,
            None
        );
        let wide = UiPrefs {
            dock_width: Some(512),
            ..UiPrefs::default()
        };
        let json = serde_json::to_string(&wide).expect("serialize");
        assert_eq!(UiPrefs::from_stored(Some(&json)), wide);
        assert_eq!(
            UiPrefs::from_stored(Some(r#"{"detailPanelPinned":true,"dockWidth":-5}"#)),
            UiPrefs::default()
        );
        // The theme: older preferences read as System; light and dark round-trip; an unknown
        // value falls back to the defaults.
        assert_eq!(
            UiPrefs::from_stored(Some(r#"{"detailPanelPinned":true}"#)).theme,
            Theme::System
        );
        for theme in [Theme::System, Theme::Light, Theme::Dark] {
            let with = UiPrefs {
                theme,
                ..UiPrefs::default()
            };
            let json = serde_json::to_string(&with).expect("serialize");
            assert_eq!(UiPrefs::from_stored(Some(&json)), with);
        }
        assert!(
            serde_json::to_string(&UiPrefs {
                theme: Theme::Dark,
                ..UiPrefs::default()
            })
            .expect("serialize")
            .contains(r#""theme":"dark""#)
        );
        assert_eq!(
            UiPrefs::from_stored(Some(r#"{"detailPanelPinned":true,"theme":"sepia"}"#)),
            UiPrefs::default()
        );
    }

    // Rollback and recovery (CLAUDE.md rule 8): one value this build can't read must never reset
    // the others, in particular not the welcome flag.
    #[test]
    fn one_unreadable_preference_keeps_the_others() {
        let base = r#""detailPanelPinned":false,"contentWidth":"comfortable","onboardingDone":true,"companionSettingsOpen":true"#;
        let keeps = |p: UiPrefs| {
            assert!(!p.detail_panel_pinned);
            assert_eq!(p.content_width, ContentWidth::Comfortable);
            assert!(p.onboarding_done, "the welcome must not come back");
            assert!(p.companion_settings_open);
            p
        };
        // A key from a newer build.
        let p = keeps(UiPrefs::from_stored(Some(&format!(
            r#"{{{base},"dockWidth":500,"theme":"dark","someFutureKey":{{"a":1}}}}"#
        ))));
        assert_eq!((p.dock_width, p.theme), (Some(500), Theme::Dark));
        // A theme this build doesn't know.
        let p = keeps(UiPrefs::from_stored(Some(&format!(
            r#"{{{base},"dockWidth":500,"theme":"sepia"}}"#
        ))));
        assert_eq!((p.dock_width, p.theme), (Some(500), Theme::System));
        // Bad panel widths: fractional, negative, text.
        for bad in ["420.5", "-5", r#""wide""#, "99999999999"] {
            let p = keeps(UiPrefs::from_stored(Some(&format!(
                r#"{{{base},"theme":"light","dockWidth":{bad}}}"#
            ))));
            assert_eq!((p.dock_width, p.theme), (None, Theme::Light), "{bad}");
        }
        // Not an object at all: the defaults.
        assert_eq!(UiPrefs::from_stored(Some("[1]")), UiPrefs::default());
        assert_eq!(UiPrefs::from_stored(Some("not json")), UiPrefs::default());
    }

    #[test]
    fn the_tray_click_setting_reads_per_field_and_unknown_values_fall_back() {
        let click = |json: &str| UiPrefs::from_stored(Some(json)).tray_click;
        assert_eq!(click(r#"{"detailPanelPinned":true}"#), TrayClick::Quick);
        assert_eq!(click(r#"{"trayClick":"main"}"#), TrayClick::Main);
        assert_eq!(click(r#"{"trayClick":"quick"}"#), TrayClick::Quick);
        // A value this build doesn't know, or the wrong type: the default, and the rest is kept.
        for bad in [r#""menu""#, "3", "null", r#"{"a":1}"#] {
            let p = UiPrefs::from_stored(Some(&format!(
                r#"{{"detailPanelPinned":false,"onboardingDone":true,"theme":"dark","trayClick":{bad}}}"#
            )));
            assert_eq!(p.tray_click, TrayClick::Quick, "{bad}");
            assert!(!p.detail_panel_pinned && p.onboarding_done, "{bad}");
            assert_eq!(p.theme, Theme::Dark, "{bad}");
        }
        let main = UiPrefs {
            tray_click: TrayClick::Main,
            ..UiPrefs::default()
        };
        let json = serde_json::to_string(&main).expect("json");
        assert!(json.contains(r#""trayClick":"main""#));
        assert_eq!(UiPrefs::from_stored(Some(&json)), main);
    }

    #[test]
    fn the_panel_width_is_kept_in_range() {
        let wide = |px| {
            UiPrefs::from_stored(Some(&format!(
                r#"{{"detailPanelPinned":true,"dockWidth":{px}}}"#
            )))
            .dock_width
        };
        assert_eq!(wide(10), Some(200));
        assert_eq!(wide(5000), Some(4096));
        assert_eq!(wide(640), Some(640));
        let saved = UiPrefs {
            dock_width: Some(1),
            ..UiPrefs::default()
        };
        assert_eq!(saved.clamped().dock_width, Some(200));
        assert_eq!(UiPrefs::default().clamped().dock_width, None);
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

    /// L2: stored Companion lookup text that came from captured task text is cleared when the
    /// capture that produced it is turned off, and not when something else changes.
    #[test]
    fn turning_capture_off_clears_companion_overview_text() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = Store::open_in_memory().expect("open");
        store
            .companion_save_profile(
                &local_store::CompanionProfile {
                    id: "p".into(),
                    kind: "openaiCompatible".into(),
                    label: "P".into(),
                    base_url: "http://localhost:11434/v1".into(),
                    model: "m".into(),
                    monthly_token_budget: None,
                    is_default: true,
                },
                T0,
            )
            .expect("profile");
        let thread = store.companion_new_thread(None, T0).expect("thread");
        let mut ids = Vec::new();
        for (scope, ctx) in [("sessionSummary", Some("SECRET-INTENT")), ("none", None)] {
            ids.push(
                store
                    .companion_add_user_message(
                        &local_store::NewUserMessage {
                            thread_id: thread.id,
                            text: "hi",
                            attached_context: ctx,
                            scope_kind: scope,
                            profile_id: "p",
                            model: "m",
                            host: "localhost:11434",
                        },
                        T0,
                    )
                    .expect("message"),
            );
        }
        store
            .companion_add_tool_call(
                &local_store::NewToolCall {
                    message_id: ids[1],
                    round: 1,
                    tool: "session_overview",
                    arguments: "{}",
                    result_text: "claude-code, working, label SECRET-PROMPT-LINE",
                    status: "approved",
                    card_activity_id: None,
                    card_name: None,
                    card_minutes: None,
                    host: "localhost:11434",
                },
                T0,
            )
            .expect("tool call");
        let texts = |store: &Store| -> (String, Option<String>) {
            (
                store.companion_tool_calls(thread.id).expect("calls")[0]
                    .result_text
                    .clone(),
                store.companion_messages(thread.id).expect("msgs")[0]
                    .attached_context
                    .clone(),
            )
        };
        let on = capture_on_prefs();
        apply_prefs(&store, dir.path(), &on, remove_markers, create_markers).expect("on");
        // Turning capture on, or changing retention while it stays on, clears nothing.
        let longer = ContextPrefs {
            retention_days: 90,
            ..capture_on_prefs()
        };
        apply_prefs(&store, dir.path(), &longer, remove_markers, create_markers).expect("same");
        let (result, attached) = texts(&store);
        assert!(result.contains("SECRET-PROMPT-LINE"));
        assert_eq!(attached.as_deref(), Some("SECRET-INTENT"));
        // Turning it off clears both.
        apply_prefs(
            &store,
            dir.path(),
            &ContextPrefs::default(),
            remove_markers,
            create_markers,
        )
        .expect("off");
        let (result, attached) = texts(&store);
        assert_eq!(result, local_store::CLEARED_RESULT_TEXT);
        assert_eq!(attached, None);
        assert_eq!(
            store.companion_messages(thread.id).expect("msgs")[0].text,
            "hi",
            "the user's own message stays"
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
        let summaries = DashboardLookups::load(&store);
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

    // -- Codex and Cursor prompts (ADR-013, ADR-021)

    fn ev_for(provider: &str, n: u32, kind: &str, session: &str) -> AgentEvent {
        parse_event(&format!(
            r#"{{"schemaVersion":1,"eventId":"00000000-0000-4000-8000-{n:012}","provider":"{provider}","providerSessionId":"{session}","eventType":"{kind}","timestampUtc":"2026-11-05T10:00:00Z"}}"#
        ))
        .expect("event")
    }

    fn only(adapter: &str) -> ContextPrefs {
        ContextPrefs {
            retention_days: 30,
            capture: BTreeMap::from([(adapter.to_owned(), true)]),
        }
    }

    #[test]
    fn codex_cursor_and_opencode_have_their_own_capture_setting_and_marker() {
        for adapter in ["codex", "cursor", "opencode"] {
            let on = only(adapter);
            assert!(on.validate().is_ok(), "{adapter}");
            assert!(on.capture_on(adapter) && !on.capture_on(P));
            let dir = tempfile::tempdir().expect("tempdir");
            create_markers(dir.path(), &on).expect("create");
            assert!(marker_path(dir.path(), adapter).is_file());
            assert!(!marker_path(dir.path(), P).exists(), "only its own marker");
            assert_eq!(
                marker_path(dir.path(), adapter),
                dir.path().join("context-capture").join(adapter),
                "the path the forwarder checks"
            );
            remove_markers(dir.path(), &ContextPrefs::default()).expect("remove");
            assert!(!marker_path(dir.path(), adapter).exists());
        }
    }

    #[test]
    fn codex_cursor_and_opencode_prompts_are_stored_only_when_their_capture_is_on() {
        for adapter in ["codex", "cursor", "opencode"] {
            let mut store = Store::open_in_memory().expect("open");
            // Off (no setting): the arriving text is discarded, the event is kept.
            let out = ingest_with_context(
                &mut store,
                &ev_for(adapter, 1, "working", "s"),
                Some(ctx(Some("SENTINEL_PROMPT"), None)),
                T0,
            )
            .expect("ingest");
            assert!(matches!(out, IngestOutcome::Stored { .. }));
            assert!(
                store.context(adapter, "s").expect("ctx").is_none(),
                "{adapter}: opt-in off, nothing stored"
            );
            // Claude Code's setting does not switch on another agent.
            set_prefs(&store, &capture_on_prefs());
            ingest_with_context(
                &mut store,
                &ev_for(adapter, 2, "working", "s"),
                Some(ctx(Some("SENTINEL_PROMPT"), None)),
                T0,
            )
            .expect("ingest");
            assert!(store.context(adapter, "s").expect("ctx").is_none());
            // On for this agent: first prompt kept, cleaned; later prompts are not.
            set_prefs(&store, &only(adapter));
            ingest_with_context(
                &mut store,
                &ev_for(adapter, 3, "working", "s"),
                Some(ctx(Some("fix it password=hunter2"), None)),
                T0,
            )
            .expect("ingest");
            ingest_with_context(
                &mut store,
                &ev_for(adapter, 4, "working", "s"),
                Some(ctx(Some("second prompt"), None)),
                T0 + time::Duration::minutes(1),
            )
            .expect("ingest");
            let c = store.context(adapter, "s").expect("ctx").expect("row");
            assert_eq!(
                c.captured_intent.clone().expect("intent").text,
                "fix it password=[hidden]"
            );
            assert!(!format!("{c:?}").contains("second prompt"));
            // The existing clear control removes it.
            store.clear_all_captured().expect("clear");
            let c = store.context(adapter, "s").expect("ctx");
            assert!(c.is_none_or(|c| c.captured_intent.is_none()));
        }
    }

    #[test]
    fn event_log_never_holds_codex_cursor_or_opencode_prompts() {
        for adapter in ["codex", "cursor", "opencode"] {
            let mut store = Store::open_in_memory().expect("open");
            set_prefs(&store, &only(adapter));
            ingest_with_context(
                &mut store,
                &ev_for(adapter, 1, "working", "s"),
                Some(ctx(Some("SENTINEL_PROMPT"), None)),
                T0,
            )
            .expect("ingest");
            let timeline = format!("{:?}", store.timeline(adapter, "s", 500).expect("timeline"));
            assert!(!timeline.contains("SENTINEL_PROMPT"), "{adapter}");
        }
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
    fn an_imported_session_exports_its_state_as_a_guess_from_the_log() {
        let mut store = Store::open_in_memory().expect("open");
        store
            .import_session(P, "sess-i", SessionState::NeedsAttention, T0, T0)
            .expect("import");
        let d = export_data(&store, P, "sess-i").expect("data");
        let md = render_markdown(&d);
        assert!(md.contains("State guessed from the agent's log"), "{md}");
        assert!(!md.contains("Last reported state"));
        let v: serde_json::Value =
            serde_json::from_str(&render_json(&d).expect("json")).expect("parse");
        assert_eq!(v["session"]["stateSource"], "guessed_from_log");
        let reported: serde_json::Value =
            serde_json::from_str(&render_json(&sample_data()).expect("json")).expect("parse");
        assert_eq!(reported["session"]["stateSource"], "reported");
    }

    #[test]
    fn a_process_gone_session_exports_its_end_as_inferred() {
        let mut store = Store::open_in_memory().expect("open");
        let process = local_store::ProcessKey::new(4242, 99);
        store
            .ingest_with_process(&ev(1, "working", "sess-g"), T0, Some(process))
            .expect("ingest");
        store
            .end_session_process_gone(P, "sess-g", process, T0, T0)
            .expect("end");
        let d = export_data(&store, P, "sess-g").expect("data");
        let md = render_markdown(&d);
        assert!(md.contains("inferred by Lullshift"), "{md}");
        assert!(!md.contains("Last reported state"));
        let v: serde_json::Value =
            serde_json::from_str(&render_json(&d).expect("json")).expect("parse");
        assert_eq!(v["session"]["stateSource"], "inferred_process_gone");
    }

    #[test]
    fn the_inferred_end_needs_both_the_ended_state_and_the_reason() {
        let mut d = sample_data();
        d.ended_reason = Some("process_gone".to_owned());
        // A reason beside a state that is not `ended` is ignored in both formats.
        assert_ne!(d.record.state, SessionState::Ended);
        let md = render_markdown(&d);
        assert!(md.contains("Last reported state"), "{md}");
        assert!(!md.contains("inferred by Lullshift"));
        let v: serde_json::Value =
            serde_json::from_str(&render_json(&d).expect("json")).expect("parse");
        assert_eq!(v["session"]["stateSource"], "reported");
        // Ended with no reason is a reported end.
        d.record.state = SessionState::Ended;
        d.ended_reason = None;
        let v: serde_json::Value =
            serde_json::from_str(&render_json(&d).expect("json")).expect("parse");
        assert_eq!(v["session"]["stateSource"], "reported");
        assert!(render_markdown(&d).contains("Last reported state"));
        // Both together: inferred.
        d.ended_reason = Some("process_gone".to_owned());
        let v: serde_json::Value =
            serde_json::from_str(&render_json(&d).expect("json")).expect("parse");
        assert_eq!(v["session"]["stateSource"], "inferred_process_gone");
        assert!(render_markdown(&d).contains("inferred by Lullshift"));
    }

    #[test]
    fn a_catch_up_guess_exports_as_filled_in_from_the_log_not_as_an_import() {
        let mut store = Store::open_in_memory().expect("open");
        store
            .ingest(&ev(1, "working", "sess-h"), T0)
            .expect("ingest");
        assert!(
            store
                .apply_log_hint(
                    P,
                    "sess-h",
                    SessionState::Idle,
                    T0 + time::Duration::minutes(30),
                    T0 + time::Duration::minutes(31),
                )
                .expect("hint")
        );
        let d = export_data(&store, P, "sess-h").expect("data");
        let md = render_markdown(&d);
        assert!(md.contains("filled in at start-up"), "{md}");
        assert!(!md.contains("(imported "));
        let v: serde_json::Value =
            serde_json::from_str(&render_json(&d).expect("json")).expect("parse");
        assert_eq!(v["session"]["stateSource"], "guessed_from_log");
        assert!(v["session"]["filledInFromLogUtc"].is_string());
        assert!(v["session"]["importedFromLogUtc"].is_null());
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
    fn notes_overview_dto_is_camel_case_bounded_and_empty_when_nothing_written() {
        let mut store = Store::open_in_memory().expect("open");
        store.ingest(&ev(1, "working", "s"), T0).expect("ingest");
        let json = serde_json::to_value(notes_overview_dto(&store).expect("dto")).expect("json");
        assert_eq!(json["sessions"], serde_json::json!([]));
        assert_eq!(json["truncated"], false);
        store.set_notes(P, "s", "my note", T0).expect("notes");
        store
            .add_item(P, "s", "checklist", None, "check", T0)
            .expect("item");
        let json = serde_json::to_value(notes_overview_dto(&store).expect("dto")).expect("json");
        assert_eq!(json["sessions"][0]["sessionId"], "s");
        assert_eq!(json["sessions"][0]["notes"], "my note");
        assert_eq!(json["sessions"][0]["checklist"][0]["text"], "check");
        assert!(json["sessions"][0]["lastReceivedUtc"].is_string());
    }

    #[test]
    fn store_errors_map_to_the_short_codes() {
        assert_eq!(code(&StoreError::NotFound), "not_found");
        assert_eq!(code(&StoreError::InvalidInput("x")), "invalid_input");
        assert_eq!(code(&StoreError::TooMany { limit: 1 }), "too_many");
        assert_eq!(code(&StoreError::Corrupt("x")), "store_unavailable");
    }

    // -- session titles and disconnects (Sprint 05)

    #[test]
    fn agent_title_is_stored_only_with_capture_on_and_kept_when_missing_later() {
        let mut store = Store::open_in_memory().expect("open");
        // Second gate: capture off discards the title.
        ingest_with_context(
            &mut store,
            &ev(1, "working", "s"),
            Some(title_ctx("SENTINEL_TITLE")),
            T0,
        )
        .expect("ingest");
        assert!(store.context(P, "s").expect("ctx").is_none());
        assert!(store.session_labels().expect("labels").is_empty());

        set_prefs(&store, &capture_on_prefs());
        ingest_with_context(
            &mut store,
            &ev(2, "working", "s"),
            Some(title_ctx("  Fix the\nlogin   bug ")),
            T0,
        )
        .expect("ingest");
        let rec = store.context(P, "s").expect("ctx").expect("row");
        assert_eq!(
            rec.agent_title.as_ref().map(|c| c.text.as_str()),
            Some("Fix the login bug")
        );
        // A later event without a title keeps the stored one.
        ingest_with_context(&mut store, &ev(3, "turn_ended", "s"), None, T0).expect("ingest");
        ingest_with_context(
            &mut store,
            &ev(4, "working", "s"),
            Some(ctx(Some("an intent"), None)),
            T0,
        )
        .expect("ingest");
        let rec = store.context(P, "s").expect("ctx").expect("row");
        assert_eq!(
            rec.agent_title.as_ref().map(|c| c.text.as_str()),
            Some("Fix the login bug")
        );
        // A newer title replaces it.
        ingest_with_context(
            &mut store,
            &ev(5, "working", "s"),
            Some(title_ctx("Renamed")),
            T0,
        )
        .expect("ingest");
        assert_eq!(
            store
                .context(P, "s")
                .expect("ctx")
                .expect("row")
                .agent_title
                .map(|c| c.text),
            Some("Renamed".to_owned())
        );
        // Clearing captured text clears it; the user's own name stays.
        store.set_user_title(P, "s", "Mine", T0).expect("title");
        store.clear_captured(P, "s").expect("clear");
        let rec = store.context(P, "s").expect("ctx").expect("row");
        assert!(rec.agent_title.is_none() && rec.user_title.as_deref() == Some("Mine"));
    }

    #[test]
    fn titles_are_cleaned_and_cut_again_in_the_app() {
        let key = concat!("sk-", "abcd1234efgh5678");
        let t = clean_title(&format!("\u{202e}deploy {key}\r\n now")).expect("title");
        assert!(
            !t.contains("sk-") && !t.contains('\u{202e}') && !t.contains('\n'),
            "{t}"
        );
        let long = clean_title(&"é ".repeat(300)).expect("title");
        assert!((119..=120).contains(&long.chars().count()));
        assert_eq!(clean_title(" \n\u{200b} "), None);
    }

    #[test]
    fn user_titles_are_plain_trimmed_and_bounded() {
        assert_eq!(
            validate_user_title("  Fix login  "),
            Ok("Fix login".to_owned())
        );
        assert_eq!(validate_user_title("   "), Ok(String::new()));
        assert_eq!(
            validate_user_title(&"x".repeat(120)).map(|t| t.len()),
            Ok(120)
        );
        for bad in [
            "x".repeat(121),
            "a\nb".to_owned(),
            "a\tb".to_owned(),
            "a\u{202e}b".to_owned(),
            "a\u{200b}b".to_owned(),
            "a\u{0000}b".to_owned(),
        ] {
            assert_eq!(validate_user_title(&bad), Err("invalid_input"), "{bad:?}");
        }
    }

    #[test]
    fn dashboard_carries_titles_and_the_disconnected_mark() {
        let mut store = Store::open_in_memory().expect("open");
        set_prefs(&store, &capture_on_prefs());
        store.ingest(&ev(1, "working", "a"), T0).expect("ingest");
        store.ingest(&ev(2, "completed", "b"), T0).expect("ingest");
        store.set_notes(P, "b", "x", T0).expect("notes");
        ingest_with_context(
            &mut store,
            &ev(3, "working", "a"),
            Some(EventContext {
                intent: Some("first line\nmore".into()),
                ask: None,
                title: Some("Agent title".into()),
            }),
            T0,
        )
        .expect("ingest");
        store.set_user_title(P, "a", "My title", T0).expect("title");
        assert_eq!(store.disconnect_provider(P, T0).expect("disconnect"), 1);

        let lookups = DashboardLookups::load(&store);
        let rows: Vec<_> = store
            .sessions()
            .expect("sessions")
            .into_iter()
            .map(|r| {
                let v = agent_core::view(&r, T0, T0, time::Duration::minutes(10));
                serde_json::to_value(dashboard_session(v, &r, &lookups)).expect("json")
            })
            .collect();
        let a = rows
            .iter()
            .find(|r| r["providerSessionId"] == "a")
            .expect("a");
        let b = rows
            .iter()
            .find(|r| r["providerSessionId"] == "b")
            .expect("b");
        assert_eq!(a["userTitle"], "My title");
        assert_eq!(a["agentTitle"], "Agent title");
        assert_eq!(a["intentLine"], "first line");
        assert_eq!(a["lastKnownState"], "working", "the raw last state is kept");
        assert!(a["disconnectedUtc"].is_string());
        assert_eq!(a["needsReview"], false);
        assert!(
            b["disconnectedUtc"].is_null(),
            "finished sessions are not marked"
        );
        assert_eq!(b["needsReview"], true);
        assert!(b["userTitle"].is_null() && b["agentTitle"].is_null());

        // A new event brings the session back.
        store.ingest(&ev(9, "working", "a"), T0).expect("ingest");
        let lookups = DashboardLookups::load(&store);
        assert!(lookups.disconnected.is_empty());
    }

    #[test]
    fn exports_include_labelled_titles() {
        let mut store = Store::open_in_memory().expect("open");
        set_prefs(&store, &capture_on_prefs());
        ingest_with_context(
            &mut store,
            &ev(1, "working", "s"),
            Some(title_ctx("Agent `title`")),
            T0,
        )
        .expect("ingest");
        store.set_user_title(P, "s", "My title", T0).expect("title");
        let data = export_data(&store, P, "s").expect("data");
        let md = render_markdown(&data);
        assert!(md.contains("Title (written by you): ` My title `"), "{md}");
        assert!(
            md.contains("Title (captured from the agent, opt-in): `` Agent `title` ``"),
            "{md}"
        );
        let json: serde_json::Value =
            serde_json::from_str(&render_json(&data).expect("json")).expect("parse");
        assert_eq!(json["writtenByYou"]["title"], "My title");
        assert_eq!(json["capturedFromAgent"]["title"]["text"], "Agent `title`");
        // No titles: the lines are absent, not empty.
        store.ingest(&ev(2, "working", "t"), T0).expect("ingest");
        let md = render_markdown(&export_data(&store, P, "t").expect("data"));
        assert!(!md.contains("Title ("), "{md}");
        let detail_json =
            serde_json::to_value(detail(&store, P, "s").expect("detail")).expect("json");
        assert_eq!(detail_json["context"]["userTitle"], "My title");
        assert_eq!(
            detail_json["context"]["agentTitle"]["text"],
            "Agent `title`"
        );
    }
}
