//! Lullshift local store (ADR-007): one SQLite database in WAL mode holding the agent event log
//! and per-session records reduced by `agent_core::reducer` (ADR-008).
//!
//! Migrations run on every open, so an updated app upgrades its database on first launch.
//! Before upgrading an existing database a copy is written next to it (`VACUUM INTO`). Each
//! migration runs in its own transaction: a failure rolls that step back and leaves the database
//! at the last good version. A database newer than this build supports is refused, never touched.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

mod companion;
mod context;
mod marketplaces;
mod waits;
pub use waits::{
    MIN_STRETCHES, WaitEstimate, WaitEvent, WorkStretch, estimates, fold_stretches,
    stretches_from_events,
};

pub use marketplaces::{
    ActivityKind, AddressChange, BUILTIN_MARKETPLACE_ID, CodeConsent, InstalledActivity,
    MAX_ACTIVITY_ID_CHARS, MAX_MARKETPLACE_ID_CHARS, MAX_PATH_CHARS, MAX_URL_CHARS,
    MarketplaceRecord, NewInstalledActivity, SavedState,
};

pub use companion::{
    CLEARED_RESULT_TEXT, CompanionMessage, CompanionModel, CompanionProfile, CompanionThread,
    CompanionToolCall, CompanionToolNotice, CompanionUsage, DEFAULT_THREAD_TITLE,
    LAST_MODEL_REFUSED, MAX_ATTACHED_CHARS, MAX_MESSAGE_CHARS, MAX_MODEL_LABEL_CHARS,
    MAX_MODELS_PER_PROFILE, MAX_PROFILES, MAX_THREAD_TITLE_CHARS, MAX_THREADS, NewToolCall,
    NewToolNotice, NewUserMessage, month_of, title_from_message,
};

pub use context::{
    Captured, ContextRecord, ContextSummary, Item, MAX_ASK_CHARS, MAX_INTENT_CHARS, MAX_ITEM_CHARS,
    MAX_ITEMS_PER_SESSION, MAX_NOTES_CHARS, MAX_TITLE_CHARS, SessionLabels, TimelineRow,
};

use agent_core::{AgentEvent, EventType, SessionRecord, SessionState, apply};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

/// One schema step. `version` is the version the database has after the step.
#[derive(Debug, Clone, Copy)]
pub struct Migration {
    pub version: i64,
    pub sql: &'static str,
}

/// All migrations, in order. Append only; never edit a released step.
pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        sql: include_str!("../migrations/0001_init.sql"),
    },
    Migration {
        version: 2,
        sql: include_str!("../migrations/0002_adapter_installations.sql"),
    },
    Migration {
        version: 3,
        sql: include_str!("../migrations/0003_preferences_and_activities.sql"),
    },
    Migration {
        version: 4,
        sql: include_str!("../migrations/0004_session_context.sql"),
    },
    Migration {
        version: 5,
        sql: include_str!("../migrations/0005_activity_play_and_session_tracking.sql"),
    },
    Migration {
        version: 6,
        sql: include_str!("../migrations/0006_marketplaces.sql"),
    },
    Migration {
        version: 7,
        sql: include_str!("../migrations/0007_saved_state_owner.sql"),
    },
    Migration {
        version: 8,
        sql: include_str!("../migrations/0008_companion.sql"),
    },
    Migration {
        version: 9,
        sql: include_str!("../migrations/0009_companion_message_host.sql"),
    },
    Migration {
        version: 10,
        sql: include_str!("../migrations/0010_companion_tool_calls.sql"),
    },
    Migration {
        version: 11,
        sql: include_str!("../migrations/0011_marketplace_private_host.sql"),
    },
    Migration {
        version: 12,
        sql: include_str!("../migrations/0012_install_source_note.sql"),
    },
    Migration {
        version: 13,
        sql: include_str!("../migrations/0013_companion_tool_notices.sql"),
    },
    Migration {
        version: 14,
        sql: include_str!("../migrations/0014_code_consent.sql"),
    },
    Migration {
        version: 15,
        sql: include_str!("../migrations/0015_code_consent_address.sql"),
    },
    Migration {
        version: 16,
        sql: include_str!("../migrations/0016_imported_sessions.sql"),
    },
    Migration {
        version: 17,
        sql: include_str!("../migrations/0017_companion_models.sql"),
    },
    Migration {
        version: 18,
        sql: include_str!("../migrations/0018_process_check.sql"),
    },
    Migration {
        version: 19,
        sql: include_str!("../migrations/0019_companion_provider_reason.sql"),
    },
];

pub fn latest_version() -> i64 {
    MIGRATIONS.last().map_or(0, |m| m.version)
}

#[derive(Debug)]
pub enum StoreError {
    /// The database was written by a newer Lullshift. Left untouched.
    TooNew {
        found: i64,
        supported: i64,
    },
    /// Migration to `version` failed and was rolled back.
    Migration {
        version: i64,
        source: rusqlite::Error,
    },
    /// Stored data does not match the expected shape (e.g. edited by hand).
    Corrupt(&'static str),
    /// An activity write would go past `limit` bytes. Nothing was written.
    QuotaExceeded {
        limit: usize,
    },
    /// An activity already holds the most keys it may. Nothing was written.
    TooManyKeys {
        limit: usize,
    },
    /// The session, item or link asked for does not exist.
    NotFound,
    /// Text too long, empty, or a kind/category/position the store does not accept.
    InvalidInput(&'static str),
    /// A session already holds the maximum number of items.
    TooMany {
        limit: usize,
    },
    /// The built-in marketplace cannot be removed.
    BuiltinProtected,
    /// The id is already in use.
    AlreadyExists,
    Sqlite(rusqlite::Error),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooNew { found, supported } => write!(
                f,
                "database schema version {found} is newer than supported version {supported}"
            ),
            Self::Migration { version, source } => {
                write!(
                    f,
                    "migration to version {version} failed and was rolled back: {source}"
                )
            }
            Self::Corrupt(what) => write!(f, "stored data is invalid: {what}"),
            Self::QuotaExceeded { limit } => {
                write!(f, "activity storage limit of {limit} bytes reached")
            }
            Self::TooManyKeys { limit } => {
                write!(f, "activity storage holds the most keys it may ({limit})")
            }
            Self::NotFound => f.write_str("not found"),
            Self::InvalidInput(what) => write!(f, "invalid input: {what}"),
            Self::TooMany { limit } => write!(f, "limit of {limit} items reached"),
            Self::BuiltinProtected => f.write_str("the built-in marketplace cannot be removed"),
            Self::AlreadyExists => f.write_str("already exists"),
            Self::Sqlite(e) => write!(f, "database error: {e}"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Sqlite(e)
    }
}

pub type Result<T> = std::result::Result<T, StoreError>;

/// Bytes every stored activity key is charged on top of its key and value, for the row's own
/// cost in the database (ADR-012).
pub const ACTIVITY_ROW_OVERHEAD: usize = 64;
/// Most keys one activity may hold.
pub const ACTIVITY_MAX_KEYS: usize = 1024;

/// Result of ingesting one event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IngestOutcome {
    /// `eventId` already stored; nothing changed.
    Duplicate,
    /// Event stored; `record` is the session after applying it.
    Stored {
        state_changed: bool,
        record: SessionRecord,
    },
}

/// Outcome of opening a database, for the UI and logs (no user data).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenReport {
    pub from_version: i64,
    pub to_version: i64,
    pub backup: Option<PathBuf>,
}

pub struct Store {
    conn: Connection,
}

/// The agent's process behind a session's newest hook event (decision 35, ADR-025). The values
/// come from outside and are range-checked before they get here; `start` is an opaque key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessKey {
    pub pid: u32,
    pub start: u64,
    /// The PID namespace the process was seen in (Linux), if known.
    pub ns: Option<u64>,
}

impl ProcessKey {
    /// A process with no namespace information.
    pub const fn new(pid: u32, start: u64) -> Self {
        Self {
            pid,
            start,
            ns: None,
        }
    }
}

/// A live-looking session with a recorded process, for the process check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessCandidate {
    pub provider: String,
    pub provider_session_id: String,
    pub process: ProcessKey,
    pub state: SessionState,
    /// When the session last received an event (local clock).
    pub last_received_utc: OffsetDateTime,
}

/// A log newer than Lullshift's own last word by less than this is not "newer": the agent's log
/// and its hook describe the same moment a few seconds apart (decision 35, ADR-025).
pub const LOG_HINT_MARGIN: time::Duration = time::Duration::seconds(10);

/// Most sessions one process check looks at.
pub const MAX_PROCESS_CANDIDATES: usize = 1000;

impl Store {
    /// Opens (creating if needed) the database at `path` and migrates it to the latest version.
    pub fn open(path: &Path) -> Result<(Self, OpenReport)> {
        let conn = Connection::open(path)?;
        configure(&conn, true)?;
        let backup = |from: i64| backup_path(path, from);
        let report = migrate(&conn, MIGRATIONS, Some(&backup))?;
        Ok((Self { conn }, report))
    }

    /// In-memory database for tests and previews.
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        configure(&conn, false)?;
        migrate(&conn, MIGRATIONS, None)?;
        Ok(Self { conn })
    }

    /// Whether freed pages are overwritten (`PRAGMA secure_delete`).
    pub fn secure_delete_enabled(&self) -> Result<bool> {
        let v: i64 = self
            .conn
            .query_row("PRAGMA secure_delete", [], |r| r.get(0))?;
        Ok(v != 0)
    }

    pub fn schema_version(&self) -> Result<i64> {
        read_version(&self.conn)
    }

    /// Stores `event` (deduplicated by `eventId`) and updates its session in one transaction.
    pub fn ingest(
        &mut self,
        event: &AgentEvent,
        received_utc: OffsetDateTime,
    ) -> Result<IngestOutcome> {
        self.ingest_with_process(event, received_utc, None)
    }

    /// [`Store::ingest`] that also records the agent's process for the process check. An event
    /// without a process clears the recorded one: a stale PID is never acted on.
    ///
    /// A session that Lullshift ended because its process was gone (`ended_reason`) is reopened
    /// by any real event that changes its state; a heartbeat, which reports no state but shows
    /// the agent is alive, reopens it to `unknown` ("connected, state not reported").
    pub fn ingest_with_process(
        &mut self,
        event: &AgentEvent,
        received_utc: OffsetDateTime,
        process: Option<ProcessKey>,
    ) -> Result<IngestOutcome> {
        let tx = self.conn.transaction()?;
        let event_json = serde_json::to_string(event)
            .map_err(|_| StoreError::Corrupt("event not serializable"))?;
        let inserted = tx.execute(
            "INSERT INTO events (event_id, provider, provider_session_id, event_type, timestamp_utc, received_utc, event_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) ON CONFLICT (event_id) DO NOTHING",
            params![
                event.event_id.to_string(),
                event.provider,
                event.provider_session_id,
                event.event_type.as_str(),
                fmt_ts(event.timestamp_utc)?,
                fmt_ts(received_utc)?,
                event_json,
            ],
        )?;
        if inserted == 0 {
            return Ok(IngestOutcome::Duplicate);
        }
        let mut current = load_session(&tx, &event.provider, &event.provider_session_id)?;
        let marks = session_marks(&tx, &event.provider, &event.provider_session_id)?;
        let loaded = current.clone();
        if current.is_some() && marks.hinted {
            // A guess the opt-in catch-up filled in over a tracked session (ADR-025): the
            // record from before it is what the event is applied to, under the normal ordering
            // rules. An older event cannot overwrite newer reported state, and the event only
            // counts as a change (and so notifies) if it differs from what was reported.
            // A prior that can't be read (tampered columns) is treated like an import: the
            // event replaces the guess, instead of every event for the session failing.
            current = load_prior(&tx, &event.provider, &event.provider_session_id)
                .ok()
                .flatten();
        } else if current.is_some() && marks.imported {
            // A session imported from the agent's own log holds only a guess (ADR-024). The
            // first real event replaces it outright, counts as a change (so it can notify like
            // any first event), and `save_session` clears the imported mark in this same
            // transaction.
            current = None;
        }
        let before = current.as_ref().map(|r| r.state);
        let process_gone = marks.process_gone;
        if !marks.imported
            && process_gone
            && event.event_type == EventType::Heartbeat
            && let Some(rec) = current.as_mut()
        {
            // Alive after all, but nothing says in what state.
            rec.state = SessionState::Unknown;
            rec.state_timestamp_utc = None;
        }
        let record = apply(current, event, received_utc);
        // The "process gone" reason stays only while the stored state is exactly the one the
        // process check set (a stale event that changed nothing).
        let keep_reason = process_gone
            && loaded.as_ref().is_some_and(|l| {
                l.state == record.state && l.state_timestamp_utc == record.state_timestamp_utc
            });
        save_session(&tx, &record, process, keep_reason)?;
        tx.commit()?;
        Ok(IngestOutcome::Stored {
            state_changed: before != Some(record.state),
            record,
        })
    }

    pub fn session(
        &self,
        provider: &str,
        provider_session_id: &str,
    ) -> Result<Option<SessionRecord>> {
        load_session(&self.conn, provider, provider_session_id)
    }

    /// All sessions, most recently received first.
    pub fn sessions(&self) -> Result<Vec<SessionRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT provider, provider_session_id, state, state_timestamp_utc, last_seen_utc, last_received_utc
             FROM sessions",
        )?;
        let rows = stmt.query_map([], raw_row)?;
        let mut out = rows
            .map(|r| r.map_err(StoreError::from).and_then(decode_row))
            .collect::<Result<Vec<_>>>()?;
        // Sorted here, not in SQL: RFC 3339 text with varying fraction digits doesn't sort lexically.
        out.sort_by(|a, b| {
            b.last_received_utc
                .cmp(&a.last_received_utc)
                .then_with(|| a.provider.cmp(&b.provider))
                .then_with(|| a.provider_session_id.cmp(&b.provider_session_id))
        });
        Ok(out)
    }

    /// Ends tracking of `provider`'s sessions that are not in a terminal state (S05-T10): their
    /// `disconnected_utc` is set to `now`, in one transaction. Returns how many sessions were
    /// newly marked; already-disconnected and finished sessions are untouched. Their stored
    /// state is kept, so History still shows what was last reported. A later event from a
    /// session clears the mark (see [`Store::ingest`]).
    pub fn disconnect_provider(&mut self, provider: &str, now: OffsetDateTime) -> Result<usize> {
        let tx = self.conn.transaction()?;
        let n = tx.execute(
            "UPDATE sessions SET disconnected_utc = ?2
             WHERE provider = ?1 AND disconnected_utc IS NULL
               AND state NOT IN ('completed', 'failed', 'cancelled', 'ended')",
            params![provider, fmt_ts(now)?],
        )?;
        tx.commit()?;
        Ok(n)
    }

    /// Sessions whose tool was disconnected and that sent nothing since: key to the time.
    pub fn disconnected_sessions(&self) -> Result<HashMap<(String, String), OffsetDateTime>> {
        let mut stmt = self.conn.prepare(
            "SELECT provider, provider_session_id, disconnected_utc FROM sessions
             WHERE disconnected_utc IS NOT NULL",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        let mut out = HashMap::new();
        for row in rows {
            let (p, s, t) = row?;
            out.insert((p, s), parse_ts(&t)?);
        }
        Ok(out)
    }

    /// Why Lullshift (not the provider) ended each such session: key to the reason, today only
    /// [`agent_core::reducer::ENDED_PROCESS_GONE`].
    pub fn ended_reasons(&self) -> Result<HashMap<(String, String), String>> {
        let mut stmt = self.conn.prepare(
            "SELECT provider, provider_session_id, ended_reason FROM sessions
             WHERE ended_reason IS NOT NULL",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        let mut out = HashMap::new();
        for row in rows {
            let (p, s, reason) = row?;
            out.insert((p, s), reason);
        }
        Ok(out)
    }

    /// Sessions whose state was filled in from the agent's log by the opt-in catch-up and that
    /// have sent nothing since: key to when. A subset of [`Store::imported_sessions`].
    pub fn log_hinted_sessions(&self) -> Result<HashMap<(String, String), OffsetDateTime>> {
        let mut stmt = self.conn.prepare(
            "SELECT provider, provider_session_id, log_hint_utc FROM sessions
             WHERE log_hint_utc IS NOT NULL",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        let mut out = HashMap::new();
        for row in rows {
            let (p, s, t) = row?;
            out.insert((p, s), parse_ts(&t)?);
        }
        Ok(out)
    }

    /// The process recorded for a session, if any.
    pub fn session_process(
        &self,
        provider: &str,
        provider_session_id: &str,
    ) -> Result<Option<ProcessKey>> {
        let row: Option<(Option<i64>, Option<i64>, Option<i64>)> = self
            .conn
            .query_row(
                "SELECT process_pid, process_start, process_ns FROM sessions
                 WHERE provider = ?1 AND provider_session_id = ?2",
                params![provider, provider_session_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        Ok(row.and_then(|(pid, start, ns)| decode_process(pid?, start?, ns)))
    }

    /// Sessions that are not finished, are still tracked, and have a recorded process: the ones
    /// the process check looks at. Newest first, at most [`MAX_PROCESS_CANDIDATES`].
    pub fn process_candidates(&self) -> Result<Vec<ProcessCandidate>> {
        let mut stmt = self.conn.prepare(
            "SELECT provider, provider_session_id, process_pid, process_start, process_ns, state,
                    last_received_utc
             FROM sessions
             WHERE process_pid IS NOT NULL AND process_start IS NOT NULL
               AND disconnected_utc IS NULL
               AND state NOT IN ('completed', 'failed', 'cancelled', 'ended')
             ORDER BY last_received_utc DESC LIMIT ?1",
        )?;
        #[allow(clippy::type_complexity)]
        let rows = stmt.query_map(params![MAX_PROCESS_CANDIDATES as i64], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, Option<i64>>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (provider, provider_session_id, pid, start, ns, state, received) = row?;
            // A tampered row (out of range, unknown state or time) is skipped, never acted on.
            let (Some(process), Some(state), Ok(last_received_utc)) = (
                decode_process(pid, start, ns),
                SessionState::from_name(&state),
                parse_ts(&received),
            ) else {
                continue;
            };
            out.push(ProcessCandidate {
                provider,
                provider_session_id,
                process,
                state,
                last_received_utc,
            });
        }
        Ok(out)
    }

    /// Ends `provider_session_id` because its process is gone (decision 35, ADR-025), but only
    /// if, at this moment, it still holds exactly `process`, is still tracked and unfinished,
    /// is not waiting for the user (a session that needs you is never ended this way), and has
    /// received nothing since `checked_utc` (when the process was looked at). Otherwise a newer
    /// event got there first and nothing changes. Returns whether the session was ended. Writes
    /// no event; the state change is [`agent_core::reducer::process_gone`].
    pub fn end_session_process_gone(
        &mut self,
        provider: &str,
        provider_session_id: &str,
        process: ProcessKey,
        checked_utc: OffsetDateTime,
        now_utc: OffsetDateTime,
    ) -> Result<bool> {
        let tx = self.conn.transaction()?;
        let Some(rec) = load_session(&tx, provider, provider_session_id)? else {
            return Ok(false);
        };
        let row: (Option<i64>, Option<i64>, Option<i64>, bool) = tx.query_row(
            "SELECT process_pid, process_start, process_ns, disconnected_utc IS NOT NULL
             FROM sessions WHERE provider = ?1 AND provider_session_id = ?2",
            params![provider, provider_session_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;
        let same = row
            .0
            .zip(row.1)
            .and_then(|(p, s)| decode_process(p, s, row.2))
            == Some(process);
        if !same
            || row.3
            || rec.state == SessionState::NeedsAttention
            || rec.last_received_utc > checked_utc
        {
            return Ok(false);
        }
        let Some(ended) = agent_core::reducer::process_gone(rec, now_utc) else {
            return Ok(false);
        };
        tx.execute(
            "UPDATE sessions SET state = ?3, state_timestamp_utc = ?4, ended_reason = ?5,
               process_pid = NULL, process_start = NULL, process_ns = NULL, imported_utc = NULL,
               log_hint_utc = NULL, hint_prior_state = NULL, hint_prior_state_ts = NULL,
               hint_prior_seen = NULL, hint_prior_received = NULL
             WHERE provider = ?1 AND provider_session_id = ?2",
            params![
                provider,
                provider_session_id,
                ended.state.as_str(),
                ended.state_timestamp_utc.map(fmt_ts).transpose()?,
                agent_core::reducer::ENDED_PROCESS_GONE,
            ],
        )?;
        tx.commit()?;
        Ok(true)
    }

    /// Fills a gap from the agent's own log (decision 35, ADR-025): the session Lullshift
    /// already tracks takes `state`, a guess, only if the log is newer than Lullshift's own last
    /// word by more than [`LOG_HINT_MARGIN`]. The session is marked like an imported one (a
    /// guess, not reported), and the record as it was is kept beside it, so the next real
    /// event is applied to that record under the normal ordering rules and then clears the
    /// mark. The recorded process, if any, is kept: the log says nothing about it, and the
    /// process check still covers the session.
    ///
    /// Never creates a session. Leaves alone: unknown sessions, finished sessions (a reported
    /// end is a fact, and logs are written after a session ends), sessions Lullshift ended
    /// because the process is gone, sessions whose tool was disconnected, sessions the user
    /// imported (that is Import's job), and guesses that are `unknown` or terminal. Returns
    /// whether anything changed. Writes no event.
    pub fn apply_log_hint(
        &mut self,
        provider: &str,
        provider_session_id: &str,
        state: SessionState,
        log_utc: OffsetDateTime,
        now_utc: OffsetDateTime,
    ) -> Result<bool> {
        if state == SessionState::Unknown || is_terminal_state(state) {
            return Ok(false);
        }
        let tx = self.conn.transaction()?;
        let Some(rec) = load_session(&tx, provider, provider_session_id)? else {
            return Ok(false);
        };
        let (disconnected, ended, imported, hinted): (bool, bool, bool, bool) = tx.query_row(
            "SELECT disconnected_utc IS NOT NULL, ended_reason IS NOT NULL,
                    imported_utc IS NOT NULL, log_hint_utc IS NOT NULL
             FROM sessions WHERE provider = ?1 AND provider_session_id = ?2",
            params![provider, provider_session_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;
        let at = log_utc.min(now_utc);
        if disconnected
            || ended
            || (imported && !hinted)
            || is_terminal_state(rec.state)
            || at <= rec.last_seen_utc + LOG_HINT_MARGIN
        {
            return Ok(false);
        }
        // The first hint keeps the record as it was; later ones keep that first record.
        tx.execute(
            "UPDATE sessions SET
               hint_prior_state = CASE WHEN log_hint_utc IS NULL THEN state ELSE hint_prior_state END,
               hint_prior_state_ts = CASE WHEN log_hint_utc IS NULL THEN state_timestamp_utc ELSE hint_prior_state_ts END,
               hint_prior_seen = CASE WHEN log_hint_utc IS NULL THEN last_seen_utc ELSE hint_prior_seen END,
               hint_prior_received = CASE WHEN log_hint_utc IS NULL THEN last_received_utc ELSE hint_prior_received END,
               state = ?3, state_timestamp_utc = ?4, last_seen_utc = ?4,
               last_received_utc = ?5, imported_utc = ?5, log_hint_utc = ?5
             WHERE provider = ?1 AND provider_session_id = ?2",
            params![
                provider,
                provider_session_id,
                state.as_str(),
                fmt_ts(at)?,
                fmt_ts(now_utc)?,
            ],
        )?;
        tx.commit()?;
        Ok(true)
    }

    /// Adds a session found in the agent's own log (owner decision 34, ADR-024), marked as
    /// imported. `state` is the guess from the log and `last_activity` the log's last write
    /// (capped at `now`). Nothing goes into the event log. Returns `false`, changing nothing, if
    /// Lullshift already has the session. Never notifies: there is no ingest outcome.
    pub fn import_session(
        &mut self,
        provider: &str,
        provider_session_id: &str,
        state: SessionState,
        last_activity: OffsetDateTime,
        now: OffsetDateTime,
    ) -> Result<bool> {
        if provider.is_empty() || provider_session_id.is_empty() || is_terminal_state(state) {
            return Err(StoreError::InvalidInput("imported session"));
        }
        let at = last_activity.min(now);
        let state_ts = (state != SessionState::Unknown).then_some(at);
        let n = self.conn.execute(
            "INSERT INTO sessions (provider, provider_session_id, state, state_timestamp_utc,
               last_seen_utc, last_received_utc, imported_utc)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6)
             ON CONFLICT (provider, provider_session_id) DO NOTHING",
            params![
                provider,
                provider_session_id,
                state.as_str(),
                state_ts.map(fmt_ts).transpose()?,
                fmt_ts(at)?,
                fmt_ts(now)?,
            ],
        )?;
        Ok(n == 1)
    }

    /// Sessions imported from a log that have not sent a hook event since: key to import time.
    pub fn imported_sessions(&self) -> Result<HashMap<(String, String), OffsetDateTime>> {
        let mut stmt = self.conn.prepare(
            "SELECT provider, provider_session_id, imported_utc FROM sessions
             WHERE imported_utc IS NOT NULL",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        let mut out = HashMap::new();
        for row in rows {
            let (p, s, t) = row?;
            out.insert((p, s), parse_ts(&t)?);
        }
        Ok(out)
    }

    /// When the app last received an event from `provider` (the newest by receipt order), or
    /// `None` if the log holds none. Used to tell "installed" from "seen working" for an
    /// experimental adapter (ADR-022).
    pub fn last_received_from(&self, provider: &str) -> Result<Option<OffsetDateTime>> {
        let ts: Option<String> = self
            .conn
            .query_row(
                "SELECT received_utc FROM events WHERE provider = ?1
                 ORDER BY receipt_seq DESC LIMIT 1",
                params![provider],
                |r| r.get(0),
            )
            .optional()?;
        ts.map(|t| parse_ts(&t)).transpose()
    }

    pub fn event_count(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT count(*) FROM events", [], |r| r.get(0))?)
    }

    /// Saves (or replaces) an adapter's install record as `installed`.
    pub fn save_adapter_install(
        &self,
        adapter_id: &str,
        record_json: &str,
        consent_utc: OffsetDateTime,
    ) -> Result<()> {
        let now = fmt_ts(OffsetDateTime::now_utc())?;
        self.conn.execute(
            "INSERT INTO adapter_installations (adapter_id, status, record_json, consent_utc, updated_utc)
             VALUES (?1, 'installed', ?2, ?3, ?4)
             ON CONFLICT (adapter_id) DO UPDATE SET status = 'installed', record_json = excluded.record_json,
               consent_utc = excluded.consent_utc, updated_utc = excluded.updated_utc",
            params![adapter_id, record_json, fmt_ts(consent_utc)?, now],
        )?;
        Ok(())
    }

    /// The record of an `installed` adapter, if any.
    pub fn adapter_install(&self, adapter_id: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT record_json FROM adapter_installations WHERE adapter_id = ?1 AND status = 'installed'",
                [adapter_id],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// A stored preference (JSON text), if set.
    pub fn preference(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT value_json FROM preferences WHERE key = ?1",
                [key],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// Saves (or replaces) a preference. The caller validates `value_json` first.
    pub fn set_preference(&self, key: &str, value_json: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO preferences (key, value_json, updated_utc) VALUES (?1, ?2, ?3)
             ON CONFLICT (key) DO UPDATE SET value_json = excluded.value_json, updated_utc = excluded.updated_utc",
            params![key, value_json, fmt_ts(OffsetDateTime::now_utc())?],
        )?;
        Ok(())
    }

    /// One value from an activity's namespace.
    pub fn activity_value(&self, activity_id: &str, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT value FROM activity_storage WHERE activity_id = ?1 AND key = ?2",
                [activity_id, key],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// What an activity's namespace costs against its quota: for every row the key and value in
    /// UTF-8 bytes plus [`ACTIVITY_ROW_OVERHEAD`], so many tiny rows are not free.
    pub fn activity_usage(&self, activity_id: &str) -> Result<usize> {
        let used: i64 = self.conn.query_row(
            "SELECT coalesce(sum(length(CAST(key AS BLOB)) + length(CAST(value AS BLOB)) + ?2), 0)
             FROM activity_storage WHERE activity_id = ?1",
            params![activity_id, ACTIVITY_ROW_OVERHEAD as i64],
            |r| r.get(0),
        )?;
        usize::try_from(used).map_err(|_| StoreError::Corrupt("negative storage size"))
    }

    /// Writes one value, refusing the write (and changing nothing) if the namespace would then
    /// cost more than `limit` bytes (see [`Store::activity_usage`]) or, for a new key, would hold
    /// more than [`ACTIVITY_MAX_KEYS`] keys. The check and the write share one transaction.
    ///
    /// The check sums the namespace's other rows. That scan stays on the `(activity_id, key)`
    /// primary key and covers at most [`ACTIVITY_MAX_KEYS`] rows, so it costs microseconds while
    /// the store lock is held and needs no running counter (and no schema change).
    pub fn set_activity_value(
        &mut self,
        activity_id: &str,
        key: &str,
        value: &str,
        limit: usize,
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        let (others, rows): (i64, i64) = tx.query_row(
            "SELECT coalesce(sum(length(CAST(key AS BLOB)) + length(CAST(value AS BLOB)) + ?3), 0),
                    count(*)
             FROM activity_storage WHERE activity_id = ?1 AND key <> ?2",
            params![activity_id, key, ACTIVITY_ROW_OVERHEAD as i64],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let others =
            usize::try_from(others).map_err(|_| StoreError::Corrupt("negative storage size"))?;
        let rows = usize::try_from(rows).map_err(|_| StoreError::Corrupt("negative row count"))?;
        if rows >= ACTIVITY_MAX_KEYS {
            return Err(StoreError::TooManyKeys {
                limit: ACTIVITY_MAX_KEYS,
            });
        }
        let cost = key
            .len()
            .saturating_add(value.len())
            .saturating_add(ACTIVITY_ROW_OVERHEAD);
        if others.saturating_add(cost) > limit {
            return Err(StoreError::QuotaExceeded { limit });
        }
        tx.execute(
            "INSERT INTO activity_storage (activity_id, key, value, updated_utc) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (activity_id, key) DO UPDATE SET value = excluded.value, updated_utc = excluded.updated_utc",
            params![activity_id, key, value, fmt_ts(OffsetDateTime::now_utc())?],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn remove_activity_value(&self, activity_id: &str, key: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM activity_storage WHERE activity_id = ?1 AND key = ?2",
            [activity_id, key],
        )?;
        Ok(())
    }

    /// Saves an activity's latest state, replacing the previous one. More than `limit` bytes is
    /// refused and the previous snapshot is kept.
    pub fn save_snapshot(
        &self,
        activity_id: &str,
        snapshot: &str,
        saved: OffsetDateTime,
        limit: usize,
    ) -> Result<()> {
        if snapshot.len() > limit {
            return Err(StoreError::QuotaExceeded { limit });
        }
        self.conn.execute(
            "INSERT INTO activity_snapshots (activity_id, snapshot, saved_utc) VALUES (?1, ?2, ?3)
             ON CONFLICT (activity_id) DO UPDATE SET snapshot = excluded.snapshot, saved_utc = excluded.saved_utc",
            params![activity_id, snapshot, fmt_ts(saved)?],
        )?;
        Ok(())
    }

    /// The last saved state of an activity and when it was saved (RFC 3339).
    pub fn snapshot(&self, activity_id: &str) -> Result<Option<(String, String)>> {
        Ok(self
            .conn
            .query_row(
                "SELECT snapshot, saved_utc FROM activity_snapshots WHERE activity_id = ?1",
                [activity_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
    }

    /// Activities with a saved state, most recent first, as `(activity_id, saved_utc)`.
    pub fn snapshots(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT activity_id, saved_utc FROM activity_snapshots ORDER BY saved_utc DESC",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn clear_snapshot(&self, activity_id: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM activity_snapshots WHERE activity_id = ?1",
            [activity_id],
        )?;
        Ok(())
    }

    /// Adds time an activity spent running in view. `seconds` is clamped to 0..=`max_add`, so a
    /// bad clock or a caller bug can't add hours at once. `launch` counts a new visit.
    pub fn add_play_time(
        &self,
        activity_id: &str,
        seconds: i64,
        launch: bool,
        max_add: i64,
        at: OffsetDateTime,
    ) -> Result<()> {
        let add = seconds.clamp(0, max_add.max(0));
        self.conn.execute(
            "INSERT INTO activity_play (activity_id, played_seconds, launches, last_played_utc)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (activity_id) DO UPDATE SET
               played_seconds = min(played_seconds + excluded.played_seconds, 4000000000),
               launches = launches + excluded.launches,
               last_played_utc = excluded.last_played_utc",
            params![activity_id, add, i64::from(launch), fmt_ts(at)?],
        )?;
        Ok(())
    }

    /// Total seconds played per activity, as `(activity_id, seconds)`.
    pub fn play_times(&self) -> Result<Vec<(String, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT activity_id, played_seconds FROM activity_play ORDER BY activity_id",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Total seconds played for one activity; 0 if never played.
    pub fn play_time(&self, activity_id: &str) -> Result<i64> {
        Ok(self
            .conn
            .query_row(
                "SELECT played_seconds FROM activity_play WHERE activity_id = ?1",
                [activity_id],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0))
    }

    /// Marks an adapter uninstalled. The row (and its backup paths) is kept for reference.
    pub fn mark_adapter_uninstalled(&self, adapter_id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE adapter_installations SET status = 'uninstalled', updated_utc = ?2 WHERE adapter_id = ?1",
            params![adapter_id, fmt_ts(OffsetDateTime::now_utc())?],
        )?;
        Ok(())
    }
}

fn configure(conn: &Connection, on_disk: bool) -> Result<()> {
    if on_disk {
        let mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
        if !mode.eq_ignore_ascii_case("wal") {
            return Err(StoreError::Corrupt("WAL journal mode unavailable"));
        }
        conn.pragma_update(None, "synchronous", "NORMAL")?;
    }
    // Deleted rows (captured text, deleted sessions) are overwritten, not left in free pages.
    conn.pragma_update(None, "secure_delete", "ON")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(())
}

fn read_version(conn: &Connection) -> Result<i64> {
    let has_table: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = 'schema_version')",
        [],
        |r| r.get(0),
    )?;
    if !has_table {
        return Ok(0);
    }
    Ok(conn
        .query_row("SELECT version FROM schema_version", [], |r| r.get(0))
        .optional()?
        .unwrap_or(0))
}

fn backup_path(db: &Path, from_version: i64) -> PathBuf {
    let stamp = OffsetDateTime::now_utc().unix_timestamp();
    let name = db.file_name().map_or_else(
        || "lullshift.db".into(),
        |n| n.to_string_lossy().into_owned(),
    );
    let mut candidate = db.with_file_name(format!("{name}.v{from_version}-{stamp}.bak"));
    let mut n = 1;
    while candidate.exists() {
        candidate = db.with_file_name(format!("{name}.v{from_version}-{stamp}-{n}.bak"));
        n += 1;
    }
    candidate
}

/// Brings `conn` up to the last of `migrations`. Exposed for migration and rollback tests.
pub fn migrate(
    conn: &Connection,
    migrations: &[Migration],
    backup_to: Option<&dyn Fn(i64) -> PathBuf>,
) -> Result<OpenReport> {
    let from = read_version(conn)?;
    let supported = migrations.last().map_or(0, |m| m.version);
    if from > supported {
        return Err(StoreError::TooNew {
            found: from,
            supported,
        });
    }
    let pending: Vec<_> = migrations.iter().filter(|m| m.version > from).collect();
    let mut backup = None;
    if from > 0
        && !pending.is_empty()
        && let Some(make_path) = backup_to
    {
        let path = make_path(from);
        conn.execute("VACUUM INTO ?1", [path.to_string_lossy()])?;
        backup = Some(path);
    }
    for m in pending {
        run_step(conn, m).map_err(|source| StoreError::Migration {
            version: m.version,
            source,
        })?;
    }
    Ok(OpenReport {
        from_version: from,
        to_version: read_version(conn)?,
        backup,
    })
}

fn run_step(conn: &Connection, m: &Migration) -> rusqlite::Result<()> {
    // unchecked_transaction: `migrate` only borrows the connection; the transaction still rolls
    // back on drop if any statement fails.
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(m.sql)?;
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL) STRICT",
    )?;
    tx.execute("DELETE FROM schema_version", [])?;
    tx.execute(
        "INSERT INTO schema_version (version) VALUES (?1)",
        [m.version],
    )?;
    tx.commit()
}

fn fmt_ts(ts: OffsetDateTime) -> Result<String> {
    ts.format(&Rfc3339)
        .map_err(|_| StoreError::Corrupt("timestamp not representable"))
}

fn parse_ts(s: &str) -> Result<OffsetDateTime> {
    OffsetDateTime::parse(s, &Rfc3339).map_err(|_| StoreError::Corrupt("stored timestamp"))
}

type RawRow = (String, String, String, Option<String>, String, String);

fn raw_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<RawRow> {
    Ok((
        r.get(0)?,
        r.get(1)?,
        r.get(2)?,
        r.get(3)?,
        r.get(4)?,
        r.get(5)?,
    ))
}

fn decode_row(
    (provider, provider_session_id, state, state_ts, seen, received): RawRow,
) -> Result<SessionRecord> {
    Ok(SessionRecord {
        provider,
        provider_session_id,
        state: SessionState::from_name(&state)
            .ok_or(StoreError::Corrupt("stored session state"))?,
        state_timestamp_utc: state_ts.as_deref().map(parse_ts).transpose()?,
        last_seen_utc: parse_ts(&seen)?,
        last_received_utc: parse_ts(&received)?,
    })
}

fn load_session(conn: &Connection, provider: &str, id: &str) -> Result<Option<SessionRecord>> {
    conn.query_row(
        "SELECT provider, provider_session_id, state, state_timestamp_utc, last_seen_utc, last_received_utc
         FROM sessions WHERE provider = ?1 AND provider_session_id = ?2",
        params![provider, id],
        raw_row,
    )
    .optional()?
    .map(decode_row)
    .transpose()
}

fn is_terminal_state(state: SessionState) -> bool {
    agent_core::reducer::is_terminal(state)
}

/// What the session row says about how its state came about. A missing row has none.
#[derive(Default)]
struct Marks {
    /// An unconfirmed import (ADR-024).
    imported: bool,
    /// A guess filled in over a tracked session by the opt-in catch-up (ADR-025).
    hinted: bool,
    /// Ended by Lullshift because the process was gone (ADR-025).
    process_gone: bool,
}

fn session_marks(conn: &Connection, provider: &str, id: &str) -> Result<Marks> {
    Ok(conn
        .query_row(
            "SELECT imported_utc IS NOT NULL, log_hint_utc IS NOT NULL, ended_reason = ?3
             FROM sessions WHERE provider = ?1 AND provider_session_id = ?2",
            params![provider, id, agent_core::reducer::ENDED_PROCESS_GONE],
            |r| {
                Ok(Marks {
                    imported: r.get::<_, bool>(0)?,
                    hinted: r.get::<_, bool>(1)?,
                    process_gone: r.get::<_, Option<bool>>(2)?.unwrap_or(false),
                })
            },
        )
        .optional()?
        .unwrap_or_default())
}

/// The record a session had before the catch-up filled in a guess, if one is kept.
fn load_prior(conn: &Connection, provider: &str, id: &str) -> Result<Option<SessionRecord>> {
    type Prior = (
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    );
    let row: Option<Prior> = conn
        .query_row(
            "SELECT hint_prior_state, hint_prior_state_ts, hint_prior_seen, hint_prior_received
             FROM sessions WHERE provider = ?1 AND provider_session_id = ?2",
            params![provider, id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    let Some((Some(state), state_ts, Some(seen), Some(received))) = row else {
        return Ok(None);
    };
    decode_row((
        provider.to_owned(),
        id.to_owned(),
        state,
        state_ts,
        seen,
        received,
    ))
    .map(Some)
}

/// A stored PID, start key and namespace, or `None` if the PID or start key is out of range (a
/// tampered row). An out-of-range namespace is dropped to "unknown namespace".
fn decode_process(pid: i64, start: i64, ns: Option<i64>) -> Option<ProcessKey> {
    let pid = u32::try_from(pid)
        .ok()
        .filter(|p| process_probe::valid_pid(*p))?;
    let start = u64::try_from(start)
        .ok()
        .filter(|s| process_probe::valid_start(*s))?;
    let ns = ns
        .and_then(|n| u64::try_from(n).ok())
        .filter(|n| process_probe::valid_start(*n));
    Some(ProcessKey { pid, start, ns })
}

fn save_session(
    tx: &Transaction<'_>,
    r: &SessionRecord,
    process: Option<ProcessKey>,
    keep_ended_reason: bool,
) -> Result<()> {
    // An out-of-range process is stored as none: the process check never acts on it.
    let process =
        process.filter(|p| process_probe::valid_pid(p.pid) && process_probe::valid_start(p.start));
    tx.execute(
        "INSERT INTO sessions (provider, provider_session_id, state, state_timestamp_utc, last_seen_utc,
           last_received_utc, process_pid, process_start, process_ns)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT (provider, provider_session_id) DO UPDATE SET
           state = excluded.state,
           state_timestamp_utc = excluded.state_timestamp_utc,
           last_seen_utc = excluded.last_seen_utc,
           last_received_utc = excluded.last_received_utc,
           disconnected_utc = NULL,
           imported_utc = NULL,
           log_hint_utc = NULL,
           hint_prior_state = NULL,
           hint_prior_state_ts = NULL,
           hint_prior_seen = NULL,
           hint_prior_received = NULL,
           process_pid = excluded.process_pid,
           process_start = excluded.process_start,
           process_ns = excluded.process_ns,
           ended_reason = CASE WHEN ?10 THEN ended_reason ELSE NULL END",
        params![
            r.provider,
            r.provider_session_id,
            r.state.as_str(),
            r.state_timestamp_utc.map(fmt_ts).transpose()?,
            fmt_ts(r.last_seen_utc)?,
            fmt_ts(r.last_received_utc)?,
            process.map(|p| i64::from(p.pid)),
            process.map(|p| i64::try_from(p.start).unwrap_or(0)),
            process
                .and_then(|p| p.ns)
                .filter(|n| process_probe::valid_start(*n))
                .map(|n| i64::try_from(n).unwrap_or(0)),
            keep_ended_reason,
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod migration_list_tests {
    use super::{MIGRATIONS, latest_version};

    /// Migrations built in parallel are merged by hand; a gap, duplicate or wrong order here
    /// would skip or repeat a step on upgrade.
    #[test]
    fn versions_are_contiguous_and_ascending_from_one() {
        for (i, m) in MIGRATIONS.iter().enumerate() {
            let want = i64::try_from(i).expect("small") + 1;
            assert_eq!(m.version, want, "migration at index {i}");
            assert!(!m.sql.trim().is_empty(), "migration {} has SQL", m.version);
        }
        assert_eq!(
            latest_version(),
            i64::try_from(MIGRATIONS.len()).expect("small")
        );
    }
}
