//! Context Keeper and Next Move storage (ADR-013): per-session context, items and links, the
//! event timeline, retention purge and session deletion.
//!
//! Captured (agent) text and user-written text are kept apart. Nothing here ever writes to the
//! `events` table, so the event log stays free of task text.

use std::collections::HashMap;

use rusqlite::{Connection, OptionalExtension, params};
use time::OffsetDateTime;

use crate::{Result, Store, StoreError, fmt_ts, parse_ts};

/// Intent (user-written or captured) and latest request.
pub const MAX_INTENT_CHARS: usize = 2000;
/// What the agent is asking (captured).
pub const MAX_ASK_CHARS: usize = 300;
/// One decision, checklist or Next Move item.
pub const MAX_ITEM_CHARS: usize = 2000;
pub const MAX_NOTES_CHARS: usize = 20_000;
/// A session title, the user's or the agent's (S05-T11).
pub const MAX_TITLE_CHARS: usize = 120;
/// Longest first line of a captured intent that [`SessionLabels`] carries (the UI cuts it
/// shorter for display).
const INTENT_LINE_CHARS: usize = 200;
pub const MAX_ITEMS_PER_SESSION: usize = 200;
/// Sessions the Notes overview returns at most (newest first).
pub const NOTES_OVERVIEW_MAX_SESSIONS: usize = 200;
/// Total characters of user text the Notes overview returns at most. Text is never cut (an
/// edit of a cut note would lose the rest), so a session that would pass this is left out and
/// the overview says it is truncated.
pub const NOTES_OVERVIEW_MAX_CHARS: usize = 1_000_000;

const KINDS: [&str; 3] = ["decision", "checklist", "next_move"];
const CATEGORIES: [&str; 4] = ["question", "acceptance", "test", "decision"];

/// A captured text and when it was captured (RFC 3339).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Captured {
    pub text: String,
    pub captured_utc: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContextRecord {
    /// Written by the user.
    pub user_intent: Option<String>,
    pub notes: String,
    pub captured_intent: Option<Captured>,
    pub ask: Option<Captured>,
    pub reviewed_utc: Option<String>,
    /// A name the user gave the session.
    pub user_title: Option<String>,
    /// The agent's own title, captured only with consent.
    pub agent_title: Option<Captured>,
}

impl ContextRecord {
    pub fn has_captured(&self) -> bool {
        self.captured_intent.is_some() || self.ask.is_some() || self.agent_title.is_some()
    }
}

/// What the session lists need to name a session (S05-T11). The display order (user title,
/// agent title, intent line, tool name and short id) is decided by the UI.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SessionLabels {
    pub user_title: Option<String>,
    pub agent_title: Option<String>,
    /// First line of the captured intent, at most 200 characters.
    pub intent_line: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub id: i64,
    pub kind: String,
    pub category: Option<String>,
    pub text: String,
    pub done: bool,
    pub position: i64,
    pub created_utc: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineRow {
    pub timestamp_utc: String,
    pub event_type: String,
    /// Enum-like hint only (`permission`, `idle_prompt`); never free text.
    pub reason: Option<String>,
}

/// One session's own notes, for the Notes overview: the user's intent, notes and checklist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotesEntry {
    pub provider: String,
    pub session_id: String,
    pub last_received_utc: OffsetDateTime,
    pub user_title: Option<String>,
    /// The agent's title, captured with consent.
    pub agent_title: Option<String>,
    /// The user's own intent (captured intent is not part of the overview).
    pub intent: Option<String>,
    pub notes: String,
    /// Review checklist items in their saved order.
    pub checklist: Vec<Item>,
}

/// The Notes overview: entries newest first, and whether some were left out by the caps.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NotesOverview {
    pub entries: Vec<NotesEntry>,
    pub truncated: bool,
}

/// Whether a session holds any context or items, and when it was last reviewed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextSummary {
    pub has_content: bool,
    pub reviewed_utc: Option<OffsetDateTime>,
}

fn chars(s: &str) -> usize {
    s.chars().count()
}

fn require_session(conn: &Connection, provider: &str, id: &str) -> Result<()> {
    let found: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sessions WHERE provider = ?1 AND provider_session_id = ?2)",
        params![provider, id],
        |r| r.get(0),
    )?;
    if found {
        Ok(())
    } else {
        Err(StoreError::NotFound)
    }
}

/// Makes sure the session exists and has a context row; returns the timestamp text used.
fn ensure_row(conn: &Connection, provider: &str, id: &str, now: OffsetDateTime) -> Result<String> {
    require_session(conn, provider, id)?;
    let ts = fmt_ts(now)?;
    conn.execute(
        "INSERT INTO session_context (provider, provider_session_id, updated_utc) VALUES (?1, ?2, ?3)
         ON CONFLICT (provider, provider_session_id) DO NOTHING",
        params![provider, id, ts],
    )?;
    Ok(ts)
}

fn captured(text: Option<String>, utc: Option<String>) -> Result<Option<Captured>> {
    match (text, utc) {
        (None, _) => Ok(None),
        (Some(text), Some(captured_utc)) => Ok(Some(Captured { text, captured_utc })),
        (Some(_), None) => Err(StoreError::Corrupt("captured text without time")),
    }
}

fn item_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Item> {
    Ok(Item {
        id: r.get(0)?,
        kind: r.get(1)?,
        category: r.get(2)?,
        text: r.get(3)?,
        done: r.get::<_, i64>(4)? != 0,
        position: r.get(5)?,
        created_utc: r.get(6)?,
    })
}

const ITEM_COLUMNS: &str = "id, kind, category, text, done, position, created_utc";

/// Rewrites positions 0..n for one session and kind, in `ids` order.
fn write_positions(conn: &Connection, ids: &[i64]) -> Result<()> {
    let mut stmt = conn.prepare("UPDATE session_items SET position = ?2 WHERE id = ?1")?;
    for (pos, id) in ids.iter().enumerate() {
        stmt.execute(params![id, i64::try_from(pos).unwrap_or(i64::MAX)])?;
    }
    Ok(())
}

fn ordered_ids(conn: &Connection, provider: &str, id: &str, kind: &str) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare(
        "SELECT id FROM session_items WHERE provider = ?1 AND provider_session_id = ?2 AND kind = ?3
         ORDER BY position, id",
    )?;
    let rows = stmt.query_map(params![provider, id, kind], |r| r.get(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

type Key<'a> = (&'a str, &'a str);

fn link_order<'a>(a: Key<'a>, b: Key<'a>) -> (Key<'a>, Key<'a>) {
    if a <= b { (a, b) } else { (b, a) }
}

/// Owner of an item: `(provider, session id, kind)`.
fn item_owner(conn: &Connection, item: i64) -> Result<(String, String, String)> {
    conn.query_row(
        "SELECT provider, provider_session_id, kind FROM session_items WHERE id = ?1",
        [item],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )
    .optional()?
    .ok_or(StoreError::NotFound)
}

impl Store {
    /// The session's context row, if it has one.
    pub fn context(&self, provider: &str, id: &str) -> Result<Option<ContextRecord>> {
        type Row = (
            Option<String>,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        );
        let row: Option<Row> = self
            .conn
            .query_row(
                "SELECT user_intent, notes, captured_intent, captured_intent_utc, NULL,
                        NULL, ask, ask_utc, reviewed_utc, user_title, agent_title, agent_title_utc
                 FROM session_context WHERE provider = ?1 AND provider_session_id = ?2",
                params![provider, id],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                        r.get(7)?,
                        r.get(8)?,
                        r.get(9)?,
                        r.get(10)?,
                        r.get(11)?,
                    ))
                },
            )
            .optional()?;
        row.map(
            |(ui, notes, ci, ciu, _lr, _lru, ask, asku, rev, ut, at, atu)| {
                Ok(ContextRecord {
                    user_intent: ui,
                    notes,
                    captured_intent: captured(ci, ciu)?,
                    ask: captured(ask, asku)?,
                    reviewed_utc: rev,
                    user_title: ut,
                    agent_title: captured(at, atu)?,
                })
            },
        )
        .transpose()
    }

    /// Sets the user's notes (empty clears). At most [`MAX_NOTES_CHARS`] characters.
    pub fn set_notes(
        &mut self,
        provider: &str,
        id: &str,
        notes: &str,
        now: OffsetDateTime,
    ) -> Result<()> {
        if chars(notes) > MAX_NOTES_CHARS {
            return Err(StoreError::InvalidInput("notes too long"));
        }
        let tx = self.conn.transaction()?;
        let ts = ensure_row(&tx, provider, id, now)?;
        tx.execute(
            "UPDATE session_context SET notes = ?3, updated_utc = ?4 WHERE provider = ?1 AND provider_session_id = ?2",
            params![provider, id, notes, ts],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Sets the user's own intent; an empty (or blank) text removes it.
    pub fn set_user_intent(
        &mut self,
        provider: &str,
        id: &str,
        intent: &str,
        now: OffsetDateTime,
    ) -> Result<()> {
        let intent = intent.trim();
        if chars(intent) > MAX_INTENT_CHARS {
            return Err(StoreError::InvalidInput("intent too long"));
        }
        let value = (!intent.is_empty()).then_some(intent);
        let tx = self.conn.transaction()?;
        let ts = ensure_row(&tx, provider, id, now)?;
        tx.execute(
            "UPDATE session_context SET user_intent = ?3, updated_utc = ?4 WHERE provider = ?1 AND provider_session_id = ?2",
            params![provider, id, value, ts],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Stores a captured prompt (ADR-013): it becomes the captured intent only if there is none
    /// yet; later prompts are not kept. A new prompt means the previous question was answered,
    /// so the captured ask is cleared. The caller has cleaned and limited the text.
    pub fn record_prompt(
        &mut self,
        provider: &str,
        id: &str,
        text: &str,
        now: OffsetDateTime,
    ) -> Result<()> {
        if text.is_empty() || chars(text) > MAX_INTENT_CHARS {
            return Err(StoreError::InvalidInput("captured prompt"));
        }
        let tx = self.conn.transaction()?;
        let ts = ensure_row(&tx, provider, id, now)?;
        tx.execute(
            "UPDATE session_context SET
               captured_intent_utc = CASE WHEN captured_intent IS NULL THEN ?4 ELSE captured_intent_utc END,
               captured_intent = coalesce(captured_intent, ?3),
               ask = NULL, ask_utc = NULL, updated_utc = ?4
             WHERE provider = ?1 AND provider_session_id = ?2",
            params![provider, id, text, ts],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Stores what the agent is asking (captured), replacing the previous one.
    pub fn record_ask(
        &mut self,
        provider: &str,
        id: &str,
        text: &str,
        now: OffsetDateTime,
    ) -> Result<()> {
        if text.is_empty() || chars(text) > MAX_ASK_CHARS {
            return Err(StoreError::InvalidInput("captured ask"));
        }
        let tx = self.conn.transaction()?;
        let ts = ensure_row(&tx, provider, id, now)?;
        tx.execute(
            "UPDATE session_context SET ask = ?3, ask_utc = ?4, updated_utc = ?4
             WHERE provider = ?1 AND provider_session_id = ?2",
            params![provider, id, text, ts],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Sets the name the user gave the session; empty (or blank) removes it. Plain text on one
    /// line: at most [`MAX_TITLE_CHARS`] characters and no control characters.
    pub fn set_user_title(
        &mut self,
        provider: &str,
        id: &str,
        title: &str,
        now: OffsetDateTime,
    ) -> Result<()> {
        let title = title.trim();
        if chars(title) > MAX_TITLE_CHARS || agent_core::text::has_unsafe_chars(title, false) {
            return Err(StoreError::InvalidInput("title"));
        }
        let value = (!title.is_empty()).then_some(title);
        let tx = self.conn.transaction()?;
        let ts = ensure_row(&tx, provider, id, now)?;
        tx.execute(
            "UPDATE session_context SET user_title = ?3, updated_utc = ?4 WHERE provider = ?1 AND provider_session_id = ?2",
            params![provider, id, value, ts],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Stores the agent's own title (captured), replacing the previous one. The caller has
    /// cleaned and limited the text. A later event without a title leaves it alone.
    pub fn record_agent_title(
        &mut self,
        provider: &str,
        id: &str,
        title: &str,
        now: OffsetDateTime,
    ) -> Result<()> {
        if title.is_empty() || chars(title) > MAX_TITLE_CHARS || title.chars().any(char::is_control)
        {
            return Err(StoreError::InvalidInput("captured title"));
        }
        let tx = self.conn.transaction()?;
        let ts = ensure_row(&tx, provider, id, now)?;
        tx.execute(
            "UPDATE session_context SET agent_title = ?3, agent_title_utc = ?4, updated_utc = ?4
             WHERE provider = ?1 AND provider_session_id = ?2",
            params![provider, id, title, ts],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Names for the session lists: user title, agent title and first intent line, per session.
    pub fn session_labels(&self) -> Result<HashMap<(String, String), SessionLabels>> {
        let mut stmt = self.conn.prepare(
            "SELECT provider, provider_session_id, user_title, agent_title,
                    substr(captured_intent, 1, ?1)
             FROM session_context
             WHERE user_title IS NOT NULL OR agent_title IS NOT NULL OR captured_intent IS NOT NULL",
        )?;
        let rows = stmt.query_map([i64::try_from(INTENT_LINE_CHARS * 4).unwrap_or(800)], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
            ))
        })?;
        let mut out = HashMap::new();
        for row in rows {
            let (p, s, user_title, agent_title, intent) = row?;
            let intent_line = intent
                .as_deref()
                .and_then(|t| t.lines().map(str::trim).find(|l| !l.is_empty()))
                .map(|l| l.chars().take(INTENT_LINE_CHARS).collect::<String>());
            out.insert(
                (p, s),
                SessionLabels {
                    user_title,
                    agent_title,
                    intent_line,
                },
            );
        }
        Ok(out)
    }

    pub fn mark_reviewed(&mut self, provider: &str, id: &str, now: OffsetDateTime) -> Result<()> {
        let tx = self.conn.transaction()?;
        let ts = ensure_row(&tx, provider, id, now)?;
        tx.execute(
            "UPDATE session_context SET reviewed_utc = ?3 WHERE provider = ?1 AND provider_session_id = ?2",
            params![provider, id, ts],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Removes only captured text for one session; the user's own text stays.
    pub fn clear_captured(&self, provider: &str, id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE session_context SET captured_intent = NULL, captured_intent_utc = NULL,
               latest_request = NULL, latest_request_utc = NULL, ask = NULL, ask_utc = NULL,
               agent_title = NULL, agent_title_utc = NULL
             WHERE provider = ?1 AND provider_session_id = ?2",
            params![provider, id],
        )?;
        self.checkpoint();
        Ok(())
    }

    /// Removes captured text from every session.
    pub fn clear_all_captured(&self) -> Result<()> {
        self.conn.execute_batch(
            "UPDATE session_context SET captured_intent = NULL, captured_intent_utc = NULL,
               latest_request = NULL, latest_request_utc = NULL, ask = NULL, ask_utc = NULL,
               agent_title = NULL, agent_title_utc = NULL",
        )?;
        self.checkpoint();
        Ok(())
    }

    /// Folds the WAL back into the database and truncates it, so deleted text does not linger
    /// in the log file. Best effort: failure (or an in-memory database) is ignored.
    fn checkpoint(&self) {
        let _ = self
            .conn
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()));
    }

    /// Items of a session: decisions, checklist, then Next Move, each in position order.
    pub fn items(&self, provider: &str, id: &str) -> Result<Vec<Item>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {ITEM_COLUMNS} FROM session_items WHERE provider = ?1 AND provider_session_id = ?2
             ORDER BY CASE kind WHEN 'decision' THEN 0 WHEN 'checklist' THEN 1 ELSE 2 END, position, id"
        ))?;
        let rows = stmt.query_map(params![provider, id], item_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Adds an item at the end of its kind. `category` is only for `next_move` items.
    pub fn add_item(
        &mut self,
        provider: &str,
        id: &str,
        kind: &str,
        category: Option<&str>,
        text: &str,
        now: OffsetDateTime,
    ) -> Result<Item> {
        if !KINDS.contains(&kind) {
            return Err(StoreError::InvalidInput("kind"));
        }
        if let Some(c) = category
            && (kind != "next_move" || !CATEGORIES.contains(&c))
        {
            return Err(StoreError::InvalidInput("category"));
        }
        let text = text.trim();
        if text.is_empty() || chars(text) > MAX_ITEM_CHARS {
            return Err(StoreError::InvalidInput("item text"));
        }
        let tx = self.conn.transaction()?;
        require_session(&tx, provider, id)?;
        let total: i64 = tx.query_row(
            "SELECT count(*) FROM session_items WHERE provider = ?1 AND provider_session_id = ?2",
            params![provider, id],
            |r| r.get(0),
        )?;
        if usize::try_from(total).unwrap_or(usize::MAX) >= MAX_ITEMS_PER_SESSION {
            return Err(StoreError::TooMany {
                limit: MAX_ITEMS_PER_SESSION,
            });
        }
        let position: i64 = tx.query_row(
            "SELECT count(*) FROM session_items WHERE provider = ?1 AND provider_session_id = ?2 AND kind = ?3",
            params![provider, id, kind],
            |r| r.get(0),
        )?;
        let created = fmt_ts(now)?;
        tx.execute(
            "INSERT INTO session_items (provider, provider_session_id, kind, category, text, done, position, created_utc)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?7)",
            params![provider, id, kind, category, text, position, created],
        )?;
        let item_id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(Item {
            id: item_id,
            kind: kind.to_owned(),
            category: category.map(str::to_owned),
            text: text.to_owned(),
            done: false,
            position,
            created_utc: created,
        })
    }

    /// Changes an item's text and/or done flag.
    pub fn update_item(&mut self, item: i64, text: Option<&str>, done: Option<bool>) -> Result<()> {
        let text = match text {
            Some(t) => {
                let t = t.trim();
                if t.is_empty() || chars(t) > MAX_ITEM_CHARS {
                    return Err(StoreError::InvalidInput("item text"));
                }
                Some(t)
            }
            None => None,
        };
        let changed = self.conn.execute(
            "UPDATE session_items SET text = coalesce(?2, text), done = coalesce(?3, done) WHERE id = ?1",
            params![item, text, done.map(i64::from)],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Deletes an item and closes the gap, so positions stay 0..n.
    pub fn delete_item(&mut self, item: i64) -> Result<()> {
        let tx = self.conn.transaction()?;
        let (provider, id, kind) = item_owner(&tx, item)?;
        tx.execute("DELETE FROM session_items WHERE id = ?1", [item])?;
        let ids = ordered_ids(&tx, &provider, &id, &kind)?;
        write_positions(&tx, &ids)?;
        tx.commit()?;
        Ok(())
    }

    /// Moves an item to `position` among the items of its session and kind (clamped to the
    /// ends); the others shift.
    pub fn move_item(&mut self, item: i64, position: i64) -> Result<()> {
        let tx = self.conn.transaction()?;
        let (provider, id, kind) = item_owner(&tx, item)?;
        let mut ids = ordered_ids(&tx, &provider, &id, &kind)?;
        ids.retain(|i| *i != item);
        let at = usize::try_from(position.max(0))
            .unwrap_or(usize::MAX)
            .min(ids.len());
        ids.insert(at, item);
        write_positions(&tx, &ids)?;
        tx.commit()?;
        Ok(())
    }

    /// Links two different existing sessions ("continues"); shown from both sides. Linking
    /// twice changes nothing.
    pub fn link_sessions(&mut self, a: Key<'_>, b: Key<'_>, now: OffsetDateTime) -> Result<()> {
        if a == b {
            return Err(StoreError::InvalidInput("link to itself"));
        }
        let tx = self.conn.transaction()?;
        require_session(&tx, a.0, a.1)?;
        require_session(&tx, b.0, b.1)?;
        let (x, y) = link_order(a, b);
        tx.execute(
            "INSERT INTO session_links (provider_a, session_a, provider_b, session_b, created_utc)
             VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT DO NOTHING",
            params![x.0, x.1, y.0, y.1, fmt_ts(now)?],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn unlink_sessions(&self, a: Key<'_>, b: Key<'_>) -> Result<()> {
        let (x, y) = link_order(a, b);
        self.conn.execute(
            "DELETE FROM session_links WHERE provider_a = ?1 AND session_a = ?2 AND provider_b = ?3 AND session_b = ?4",
            params![x.0, x.1, y.0, y.1],
        )?;
        Ok(())
    }

    /// Sessions linked to this one, as `(provider, session id)`, from either side.
    pub fn links(&self, provider: &str, id: &str) -> Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT provider_b, session_b FROM session_links WHERE provider_a = ?1 AND session_a = ?2
             UNION
             SELECT provider_a, session_a FROM session_links WHERE provider_b = ?1 AND session_b = ?2
             ORDER BY 1, 2",
        )?;
        let rows = stmt.query_map(params![provider, id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The newest `limit` events of a session, oldest first. Types and enum-like reasons only.
    pub fn timeline(&self, provider: &str, id: &str, limit: usize) -> Result<Vec<TimelineRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT timestamp_utc, event_type, event_json FROM events
             WHERE provider = ?1 AND provider_session_id = ?2 ORDER BY receipt_seq DESC LIMIT ?3",
        )?;
        let rows = stmt.query_map(
            params![provider, id, i64::try_from(limit).unwrap_or(i64::MAX)],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            },
        )?;
        let mut out = rows
            .map(|r| {
                let (timestamp_utc, event_type, json) = r?;
                let reason = serde_json::from_str::<serde_json::Value>(&json)
                    .ok()
                    .and_then(|v| v["metadata"]["reason"].as_str().map(str::to_owned))
                    .filter(|s| {
                        (1..=64).contains(&s.len())
                            && s.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
                    });
                Ok(TimelineRow {
                    timestamp_utc,
                    event_type,
                    reason,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        out.reverse();
        Ok(out)
    }

    /// Everything the user wrote across sessions, in one read: sessions (newest activity first)
    /// that have an intent of the user's own, notes or checklist items. At most
    /// [`NOTES_OVERVIEW_MAX_SESSIONS`] sessions and [`NOTES_OVERVIEW_MAX_CHARS`] characters; when
    /// a cap is hit, `truncated` is set and later (older) sessions are left out whole.
    pub fn notes_overview(&self) -> Result<NotesOverview> {
        type Ctx = (Option<String>, Option<String>, String, Option<String>);
        let mut contexts: HashMap<(String, String), Ctx> = HashMap::new();
        let mut stmt = self.conn.prepare(
            "SELECT provider, provider_session_id, user_intent, user_title, notes, agent_title
             FROM session_context",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                (r.get::<_, String>(0)?, r.get::<_, String>(1)?),
                (
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, Option<String>>(5)?,
                ),
            ))
        })?;
        for row in rows {
            let (key, ctx) = row?;
            contexts.insert(key, ctx);
        }
        let mut lists: HashMap<(String, String), Vec<Item>> = HashMap::new();
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {ITEM_COLUMNS}, provider, provider_session_id FROM session_items
             WHERE kind = 'checklist' ORDER BY position, id"
        ))?;
        let rows = stmt.query_map([], |r| {
            Ok((
                item_row(r)?,
                (r.get::<_, String>(7)?, r.get::<_, String>(8)?),
            ))
        })?;
        for row in rows {
            let (item, key) = row?;
            lists.entry(key).or_default().push(item);
        }

        let mut out = NotesOverview::default();
        let mut total = 0usize;
        // `sessions()` is already newest first.
        for rec in self.sessions()? {
            let key = (rec.provider.clone(), rec.provider_session_id.clone());
            let ctx = contexts.remove(&key);
            let checklist = lists.remove(&key).unwrap_or_default();
            let (intent, user_title, notes, agent_title) = ctx.unwrap_or_default();
            let intent = intent.filter(|t| !t.trim().is_empty());
            if intent.is_none() && notes.is_empty() && checklist.is_empty() {
                continue;
            }
            let size = intent.as_deref().map_or(0, chars)
                + chars(&notes)
                + checklist.iter().map(|i| chars(&i.text)).sum::<usize>();
            if out.entries.len() >= NOTES_OVERVIEW_MAX_SESSIONS
                || total.saturating_add(size) > NOTES_OVERVIEW_MAX_CHARS
            {
                out.truncated = true;
                break;
            }
            total += size;
            out.entries.push(NotesEntry {
                provider: rec.provider,
                session_id: rec.provider_session_id,
                last_received_utc: rec.last_received_utc,
                user_title,
                agent_title,
                intent,
                notes,
                checklist,
            });
        }
        Ok(out)
    }

    /// Per session: whether it holds any context or items, and when it was last reviewed.
    pub fn context_summaries(&self) -> Result<HashMap<(String, String), ContextSummary>> {
        let mut out: HashMap<(String, String), ContextSummary> = HashMap::new();
        let mut stmt = self.conn.prepare(
            "SELECT provider, provider_session_id, reviewed_utc,
                    (user_intent IS NOT NULL OR notes <> '' OR captured_intent IS NOT NULL
                     OR latest_request IS NOT NULL OR ask IS NOT NULL)
             FROM session_context",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, i64>(3)? != 0,
            ))
        })?;
        for row in rows {
            let (p, s, reviewed, has) = row?;
            let reviewed_utc = reviewed.as_deref().map(parse_ts).transpose()?;
            out.insert(
                (p, s),
                ContextSummary {
                    has_content: has,
                    reviewed_utc,
                },
            );
        }
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT provider, provider_session_id FROM session_items")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        for row in rows {
            out.entry(row?)
                .and_modify(|c| c.has_content = true)
                .or_insert(ContextSummary {
                    has_content: true,
                    reviewed_utc: None,
                });
        }
        Ok(out)
    }

    /// Deletes events received before `cutoff` (retention). Returns how many. Sessions, context
    /// and items are untouched.
    pub fn purge_events_before(&self, cutoff: OffsetDateTime) -> Result<usize> {
        // unixepoch() understands the RFC 3339 text however many fraction digits it has.
        Ok(self.conn.execute(
            "DELETE FROM events WHERE unixepoch(received_utc) < ?1",
            [cutoff.unix_timestamp()],
        )?)
    }

    /// Deletes a session with its events, context, items and links, in one transaction.
    pub fn delete_session(&mut self, provider: &str, id: &str) -> Result<()> {
        let tx = self.conn.transaction()?;
        require_session(&tx, provider, id)?;
        for sql in [
            "DELETE FROM events WHERE provider = ?1 AND provider_session_id = ?2",
            "DELETE FROM session_context WHERE provider = ?1 AND provider_session_id = ?2",
            "DELETE FROM session_items WHERE provider = ?1 AND provider_session_id = ?2",
            "DELETE FROM session_links WHERE (provider_a = ?1 AND session_a = ?2) OR (provider_b = ?1 AND session_b = ?2)",
            "DELETE FROM sessions WHERE provider = ?1 AND provider_session_id = ?2",
        ] {
            tx.execute(sql, params![provider, id])?;
        }
        tx.commit()?;
        self.checkpoint();
        Ok(())
    }
}
