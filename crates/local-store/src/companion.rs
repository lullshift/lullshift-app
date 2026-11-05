//! Companion storage (Sprint 09, ADR-020): provider profiles, threads, messages and token usage.
//!
//! This module stores NO key material. Keys live in the OS keychain only. A profile row holds a
//! label, provider kind, base URL, model name and an optional monthly token budget.

use rusqlite::{OptionalExtension, Row, params};
use time::OffsetDateTime;

use crate::{Result, Store, StoreError, fmt_ts};

pub const MAX_MESSAGE_CHARS: usize = 20_000;
pub const MAX_ATTACHED_CHARS: usize = 60_000;
pub const MAX_THREAD_TITLE_CHARS: usize = 120;
pub const MAX_PROFILES: usize = 20;
pub const MAX_THREADS: usize = 500;
pub const DEFAULT_THREAD_TITLE: &str = "New conversation";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompanionProfile {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub base_url: String,
    pub model: String,
    pub monthly_token_budget: Option<i64>,
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompanionThread {
    pub id: i64,
    pub title: String,
    pub profile_id: Option<String>,
    pub model: Option<String>,
    pub created_utc: String,
    pub updated_utc: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompanionMessage {
    pub id: i64,
    pub thread_id: i64,
    pub role: String,
    pub text: String,
    pub attached_context: Option<String>,
    pub scope_kind: String,
    pub status: String,
    pub error_kind: Option<String>,
    pub error_message: Option<String>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub profile_id: Option<String>,
    pub model: Option<String>,
    /// The host this message went to (user) or came from (reply).
    pub host: Option<String>,
    pub created_utc: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompanionUsage {
    pub profile_id: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
}

/// A user message about to be sent.
#[derive(Debug, Clone)]
pub struct NewUserMessage<'a> {
    pub thread_id: i64,
    pub text: &'a str,
    pub attached_context: Option<&'a str>,
    pub scope_kind: &'a str,
    pub profile_id: &'a str,
    pub model: &'a str,
    /// `host` or `host:port` of the profile at send time; history only goes to the same host.
    pub host: &'a str,
}

/// `YYYY-MM` in UTC.
pub fn month_of(t: OffsetDateTime) -> String {
    let t = t.to_offset(time::UtcOffset::UTC);
    format!("{:04}-{:02}", t.year(), u8::from(t.month()))
}

const MSG_COLS: &str = "id, thread_id, role, text, attached_context, scope_kind, status, error_kind, \
     error_message, input_tokens, output_tokens, profile_id, model, host, created_utc";

fn msg_row(r: &Row<'_>) -> rusqlite::Result<CompanionMessage> {
    Ok(CompanionMessage {
        id: r.get(0)?,
        thread_id: r.get(1)?,
        role: r.get(2)?,
        text: r.get(3)?,
        attached_context: r.get(4)?,
        scope_kind: r.get(5)?,
        status: r.get(6)?,
        error_kind: r.get(7)?,
        error_message: r.get(8)?,
        input_tokens: r.get(9)?,
        output_tokens: r.get(10)?,
        profile_id: r.get(11)?,
        model: r.get(12)?,
        host: r.get(13)?,
        created_utc: r.get(14)?,
    })
}

const PROFILE_COLS: &str = "id, kind, label, base_url, model, monthly_token_budget, is_default";

fn profile_row(r: &Row<'_>) -> rusqlite::Result<CompanionProfile> {
    Ok(CompanionProfile {
        id: r.get(0)?,
        kind: r.get(1)?,
        label: r.get(2)?,
        base_url: r.get(3)?,
        model: r.get(4)?,
        monthly_token_budget: r.get(5)?,
        is_default: r.get::<_, i64>(6)? != 0,
    })
}

fn thread_row(r: &Row<'_>) -> rusqlite::Result<CompanionThread> {
    Ok(CompanionThread {
        id: r.get(0)?,
        title: r.get(1)?,
        profile_id: r.get(2)?,
        model: r.get(3)?,
        created_utc: r.get(4)?,
        updated_utc: r.get(5)?,
    })
}

fn clean_title(t: &str) -> Result<String> {
    let t: String = t.chars().filter(|c| !c.is_control()).collect();
    let t = t.trim().to_owned();
    if t.is_empty() || t.chars().count() > MAX_THREAD_TITLE_CHARS {
        return Err(StoreError::InvalidInput("title length"));
    }
    Ok(t)
}

/// A CHECK or FOREIGN KEY violation is a caller mistake, not a database fault.
fn check_invalid(e: rusqlite::Error) -> StoreError {
    match &e {
        rusqlite::Error::SqliteFailure(f, _)
            if f.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            StoreError::InvalidInput("a value was refused by the database")
        }
        _ => StoreError::Sqlite(e),
    }
}

fn record_usage_on(
    conn: &rusqlite::Connection,
    profile_id: &str,
    now: OffsetDateTime,
    input_tokens: i64,
    output_tokens: i64,
) -> Result<()> {
    conn.execute(
        "INSERT INTO companion_usage (profile_id, month, input_tokens, output_tokens, recorded_utc)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            profile_id,
            month_of(now),
            input_tokens.max(0),
            output_tokens.max(0),
            fmt_ts(now)?
        ],
    )
    .map_err(check_invalid)?;
    Ok(())
}

impl Store {
    // ---- profiles ----------------------------------------------------------------------

    pub fn companion_profiles(&self) -> Result<Vec<CompanionProfile>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {PROFILE_COLS} FROM companion_profiles ORDER BY is_default DESC, created_utc, id"
        ))?;
        let rows = stmt.query_map([], profile_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn companion_profile(&self, id: &str) -> Result<Option<CompanionProfile>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {PROFILE_COLS} FROM companion_profiles WHERE id = ?1"),
                [id],
                profile_row,
            )
            .optional()?)
    }

    /// Inserts or updates a profile. The first profile becomes the default; asking for the
    /// default clears it on the others in the same transaction. The caller validates the URL.
    pub fn companion_save_profile(
        &mut self,
        p: &CompanionProfile,
        now: OffsetDateTime,
    ) -> Result<CompanionProfile> {
        let tx = self.conn.transaction()?;
        let existing: i64 =
            tx.query_row("SELECT count(*) FROM companion_profiles", [], |r| r.get(0))?;
        let known: bool = tx.query_row(
            "SELECT EXISTS (SELECT 1 FROM companion_profiles WHERE id = ?1)",
            [&p.id],
            |r| r.get(0),
        )?;
        if !known && existing as usize >= MAX_PROFILES {
            return Err(StoreError::TooMany {
                limit: MAX_PROFILES,
            });
        }
        let make_default = p.is_default || existing == 0;
        if make_default {
            tx.execute(
                "UPDATE companion_profiles SET is_default = 0 WHERE id <> ?1",
                [&p.id],
            )?;
        }
        let ts = fmt_ts(now)?;
        tx.execute(
            "INSERT INTO companion_profiles
               (id, kind, label, base_url, model, monthly_token_budget, is_default, created_utc, updated_utc)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
             ON CONFLICT (id) DO UPDATE SET kind = excluded.kind, label = excluded.label,
               base_url = excluded.base_url, model = excluded.model,
               monthly_token_budget = excluded.monthly_token_budget,
               is_default = CASE WHEN ?7 = 1 THEN 1 ELSE companion_profiles.is_default END,
               updated_utc = excluded.updated_utc",
            params![
                p.id,
                p.kind,
                p.label,
                p.base_url,
                p.model,
                p.monthly_token_budget,
                i64::from(make_default),
                ts
            ],
        )
        .map_err(check_invalid)?;
        let out = tx.query_row(
            &format!("SELECT {PROFILE_COLS} FROM companion_profiles WHERE id = ?1"),
            [&p.id],
            profile_row,
        )?;
        tx.commit()?;
        Ok(out)
    }

    /// Deletes a profile and its usage. Threads and messages keep their text; their profile
    /// link becomes empty. If it was the default, the oldest remaining profile becomes it.
    pub fn companion_delete_profile(&mut self, id: &str) -> Result<()> {
        let tx = self.conn.transaction()?;
        let n = tx.execute("DELETE FROM companion_profiles WHERE id = ?1", [id])?;
        if n == 0 {
            return Err(StoreError::NotFound);
        }
        tx.execute(
            "UPDATE companion_profiles SET is_default = 1
             WHERE id = (SELECT id FROM companion_profiles ORDER BY created_utc, id LIMIT 1)
               AND NOT EXISTS (SELECT 1 FROM companion_profiles WHERE is_default = 1)",
            [],
        )?;
        tx.commit()?;
        Ok(())
    }

    // ---- threads -----------------------------------------------------------------------

    pub fn companion_threads(&self) -> Result<Vec<CompanionThread>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, profile_id, model, created_utc, updated_utc FROM companion_threads
             ORDER BY updated_utc DESC, id DESC",
        )?;
        let rows = stmt.query_map([], thread_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn companion_thread(&self, id: i64) -> Result<Option<CompanionThread>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, title, profile_id, model, created_utc, updated_utc
                 FROM companion_threads WHERE id = ?1",
                [id],
                thread_row,
            )
            .optional()?)
    }

    pub fn companion_new_thread(
        &mut self,
        title: Option<&str>,
        now: OffsetDateTime,
    ) -> Result<CompanionThread> {
        let title = match title {
            Some(t) => clean_title(t)?,
            None => DEFAULT_THREAD_TITLE.to_owned(),
        };
        let n: i64 = self
            .conn
            .query_row("SELECT count(*) FROM companion_threads", [], |r| r.get(0))?;
        if n as usize >= MAX_THREADS {
            return Err(StoreError::TooMany { limit: MAX_THREADS });
        }
        let ts = fmt_ts(now)?;
        self.conn.execute(
            "INSERT INTO companion_threads (title, created_utc, updated_utc) VALUES (?1, ?2, ?2)",
            params![title, ts],
        )?;
        let id = self.conn.last_insert_rowid();
        self.companion_thread(id)?.ok_or(StoreError::NotFound)
    }

    pub fn companion_rename_thread(&self, id: i64, title: &str) -> Result<()> {
        let title = clean_title(title)?;
        let n = self.conn.execute(
            "UPDATE companion_threads SET title = ?2 WHERE id = ?1",
            params![id, title],
        )?;
        if n == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Deletes a thread and its messages (cascade), then checkpoints the WAL so the text does
    /// not linger (`secure_delete` is on).
    pub fn companion_delete_thread(&self, id: i64) -> Result<()> {
        let n = self
            .conn
            .execute("DELETE FROM companion_threads WHERE id = ?1", [id])?;
        if n == 0 {
            return Err(StoreError::NotFound);
        }
        let _ = self
            .conn
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()));
        Ok(())
    }

    // ---- messages ----------------------------------------------------------------------

    pub fn companion_messages(&self, thread_id: i64) -> Result<Vec<CompanionMessage>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {MSG_COLS} FROM companion_messages WHERE thread_id = ?1 ORDER BY id"
        ))?;
        let rows = stmt.query_map([thread_id], msg_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn companion_message(&self, id: i64) -> Result<Option<CompanionMessage>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {MSG_COLS} FROM companion_messages WHERE id = ?1"),
                [id],
                msg_row,
            )
            .optional()?)
    }

    /// Stores a user message as `pending` and links the thread to the profile and model used.
    pub fn companion_add_user_message(
        &mut self,
        m: &NewUserMessage<'_>,
        now: OffsetDateTime,
    ) -> Result<i64> {
        if m.text.trim().is_empty() || m.text.chars().count() > MAX_MESSAGE_CHARS {
            return Err(StoreError::InvalidInput("message length"));
        }
        if m.attached_context
            .is_some_and(|c| c.chars().count() > MAX_ATTACHED_CHARS)
        {
            return Err(StoreError::InvalidInput("attached context length"));
        }
        let ts = fmt_ts(now)?;
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO companion_messages
               (thread_id, role, text, attached_context, scope_kind, status, profile_id, model, host, created_utc)
             VALUES (?1, 'user', ?2, ?3, ?4, 'pending', ?5, ?6, ?7, ?8)",
            params![
                m.thread_id,
                m.text,
                m.attached_context,
                m.scope_kind,
                m.profile_id,
                m.model,
                m.host,
                ts
            ],
        )
        .map_err(check_invalid)?;
        let id = tx.last_insert_rowid();
        tx.execute(
            "UPDATE companion_threads SET profile_id = ?2, model = ?3, updated_utc = ?4 WHERE id = ?1",
            params![m.thread_id, m.profile_id, m.model, ts],
        )?;
        tx.commit()?;
        Ok(id)
    }

    /// Completes a send in one transaction: the user message becomes `sent`, the assistant
    /// message is stored, and the usage is recorded for the UTC month of `now`.
    pub fn companion_complete_send(
        &mut self,
        user_message: i64,
        reply: &str,
        input_tokens: i64,
        output_tokens: i64,
        now: OffsetDateTime,
    ) -> Result<CompanionMessage> {
        let ts = fmt_ts(now)?;
        let tx = self.conn.transaction()?;
        let (thread, profile, model, host): (i64, Option<String>, Option<String>, Option<String>) =
            tx.query_row(
                "SELECT thread_id, profile_id, model, host FROM companion_messages
                 WHERE id = ?1 AND role = 'user'",
                [user_message],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?
            .ok_or(StoreError::NotFound)?;
        tx.execute(
            "UPDATE companion_messages SET status = 'sent', error_kind = NULL, error_message = NULL,
               input_tokens = ?2, output_tokens = ?3 WHERE id = ?1",
            params![user_message, input_tokens, output_tokens],
        )?;
        tx.execute(
            "INSERT INTO companion_messages
               (thread_id, role, text, scope_kind, status, output_tokens, profile_id, model, host, created_utc)
             VALUES (?1, 'assistant', ?2, 'none', 'sent', ?3, ?4, ?5, ?6, ?7)",
            params![thread, reply, output_tokens, profile, model, host, ts],
        )
        .map_err(check_invalid)?;
        let id = tx.last_insert_rowid();
        if let Some(p) = &profile {
            record_usage_on(&tx, p, now, input_tokens, output_tokens)?;
        }
        tx.execute(
            "UPDATE companion_threads SET updated_utc = ?2 WHERE id = ?1",
            params![thread, ts],
        )?;
        let out = tx.query_row(
            &format!("SELECT {MSG_COLS} FROM companion_messages WHERE id = ?1"),
            [id],
            msg_row,
        )?;
        tx.commit()?;
        Ok(out)
    }

    /// Marks a user message `failed` with a fixed kind and friendly message.
    pub fn companion_fail_message(&self, id: i64, kind: &str, message: &str) -> Result<()> {
        let n = self.conn.execute(
            "UPDATE companion_messages SET status = 'failed', error_kind = ?2, error_message = ?3
             WHERE id = ?1 AND role = 'user'",
            params![id, kind, message],
        )?;
        if n == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Puts a failed user message back to `pending` for a retry.
    pub fn companion_mark_pending(&self, id: i64) -> Result<()> {
        let n = self.conn.execute(
            "UPDATE companion_messages SET status = 'pending', error_kind = NULL, error_message = NULL
             WHERE id = ?1 AND role = 'user' AND status = 'failed'",
            [id],
        )?;
        if n == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Messages left `pending` by a crash or quit become `failed` (kind `cancelled`), so they
    /// can be retried. Returns how many were changed.
    pub fn companion_fail_stale_pending(&self) -> Result<usize> {
        Ok(self.conn.execute(
            "UPDATE companion_messages SET status = 'failed', error_kind = 'cancelled',
               error_message = 'This message was interrupted before it finished. Try again.'
             WHERE status = 'pending'",
            [],
        )?)
    }

    // ---- usage -------------------------------------------------------------------------

    /// Records tokens for a call that is not part of a message (the connection test).
    pub fn companion_record_usage(
        &self,
        profile_id: &str,
        now: OffsetDateTime,
        input_tokens: i64,
        output_tokens: i64,
    ) -> Result<()> {
        record_usage_on(&self.conn, profile_id, now, input_tokens, output_tokens)
    }

    /// Per-profile totals for `month` (`YYYY-MM`); profiles with no usage show zero.
    pub fn companion_usage(&self, month: &str) -> Result<Vec<CompanionUsage>> {
        let mut stmt = self.conn.prepare(
            "SELECT p.id, COALESCE(SUM(u.input_tokens), 0), COALESCE(SUM(u.output_tokens), 0)
             FROM companion_profiles p
             LEFT JOIN companion_usage u ON u.profile_id = p.id AND u.month = ?1
             GROUP BY p.id ORDER BY p.created_utc, p.id",
        )?;
        let rows = stmt.query_map([month], |r| {
            Ok(CompanionUsage {
                profile_id: r.get(0)?,
                input_tokens: r.get(1)?,
                output_tokens: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}
