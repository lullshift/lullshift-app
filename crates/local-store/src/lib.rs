//! Lullshift local store (ADR-007): one SQLite database in WAL mode holding the agent event log
//! and per-session records reduced by `agent_core::reducer` (ADR-008).
//!
//! Migrations run on every open, so an updated app upgrades its database on first launch.
//! Before upgrading an existing database a copy is written next to it (`VACUUM INTO`). Each
//! migration runs in its own transaction: a failure rolls that step back and leaves the database
//! at the last good version. A database newer than this build supports is refused, never touched.

use std::fmt;
use std::path::{Path, PathBuf};

use agent_core::{AgentEvent, SessionRecord, SessionState, apply};
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

    pub fn schema_version(&self) -> Result<i64> {
        read_version(&self.conn)
    }

    /// Stores `event` (deduplicated by `eventId`) and updates its session in one transaction.
    pub fn ingest(
        &mut self,
        event: &AgentEvent,
        received_utc: OffsetDateTime,
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
        let current = load_session(&tx, &event.provider, &event.provider_session_id)?;
        let before = current.as_ref().map(|r| r.state);
        let record = apply(current, event, received_utc);
        save_session(&tx, &record)?;
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

fn save_session(tx: &Transaction<'_>, r: &SessionRecord) -> Result<()> {
    tx.execute(
        "INSERT INTO sessions (provider, provider_session_id, state, state_timestamp_utc, last_seen_utc, last_received_utc)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT (provider, provider_session_id) DO UPDATE SET
           state = excluded.state,
           state_timestamp_utc = excluded.state_timestamp_utc,
           last_seen_utc = excluded.last_seen_utc,
           last_received_utc = excluded.last_received_utc",
        params![
            r.provider,
            r.provider_session_id,
            r.state.as_str(),
            r.state_timestamp_utc.map(fmt_ts).transpose()?,
            fmt_ts(r.last_seen_utc)?,
            fmt_ts(r.last_received_utc)?,
        ],
    )?;
    Ok(())
}
