//! Companion storage (Sprint 09, ADR-020): provider profiles, threads, messages and token usage.
//!
//! This module stores NO key material. Keys live in the OS keychain only. A profile (provider) row
//! holds a label, provider kind, base URL and an optional monthly token budget; its models live in
//! `companion_models` (decision 36), one of them the provider's default. The overall default model
//! is the default model of the default provider.

use rusqlite::{OptionalExtension, Row, params};
use time::OffsetDateTime;

use crate::{Result, Store, StoreError, fmt_ts};

/// What a cleared lookup result is replaced with.
pub const CLEARED_RESULT_TEXT: &str =
    "This lookup's text was removed when its task text was turned off or deleted.";

pub const MAX_MESSAGE_CHARS: usize = 20_000;
pub const MAX_ATTACHED_CHARS: usize = 60_000;
pub const MAX_THREAD_TITLE_CHARS: usize = 120;
pub const MAX_PROFILES: usize = 20;
/// Most models one provider may list.
pub const MAX_MODELS_PER_PROFILE: usize = 50;
pub const MAX_MODEL_LABEL_CHARS: usize = 80;
/// The `InvalidInput` reason when removing a provider's last model; callers match on it.
pub const LAST_MODEL_REFUSED: &str = "a provider keeps at least one model";
pub const MAX_THREADS: usize = 500;
pub const DEFAULT_THREAD_TITLE: &str = "New conversation";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompanionProfile {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub base_url: String,
    /// The provider's default model. Saving a profile adds this model to its list if it is new
    /// and makes it the default; reading returns the current default.
    pub model: String,
    pub monthly_token_budget: Option<i64>,
    pub is_default: bool,
}

/// One model a provider offers (decision 36).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompanionModel {
    pub id: i64,
    pub profile_id: String,
    pub model: String,
    /// Optional display name; the model id is shown when absent.
    pub label: Option<String>,
    pub is_default: bool,
    pub created_utc: String,
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
    /// Who said `error_reason` ("OpenAI", "Anthropic", "The provider"), and what they said: the
    /// provider's own sanitised words, apart from the fixed `error_message` (migration 19).
    pub error_reason_by: Option<String>,
    pub error_reason: Option<String>,
    /// A reply that arrived all at once because the request was not streamed (migration 19).
    pub no_stream: bool,
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

/// A lookup the model asked for during a reply, with what was sent back (S12-T16).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompanionToolCall {
    pub id: i64,
    pub message_id: i64,
    pub round: i64,
    pub tool: String,
    pub arguments: String,
    /// Exactly what was sent to the provider.
    pub result_text: String,
    /// `approved` or `declined`.
    pub status: String,
    pub card_activity_id: Option<String>,
    pub card_name: Option<String>,
    pub card_minutes: Option<i64>,
    pub host: Option<String>,
    pub created_utc: String,
}

/// A call the model made that Lullshift did not run (owner decision 27.6). No arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompanionToolNotice {
    pub id: i64,
    pub message_id: i64,
    pub round: i64,
    /// `unknown`, `malformed` or `limit`.
    pub reason: String,
    /// The requested tool name, only when it was a short safe identifier.
    pub tool: Option<String>,
    pub created_utc: String,
}

#[derive(Debug, Clone)]
pub struct NewToolNotice<'a> {
    pub message_id: i64,
    pub round: i64,
    pub reason: &'a str,
    pub tool: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct NewToolCall<'a> {
    pub message_id: i64,
    pub round: i64,
    pub tool: &'a str,
    pub arguments: &'a str,
    pub result_text: &'a str,
    pub status: &'a str,
    pub card_activity_id: Option<&'a str>,
    pub card_name: Option<&'a str>,
    pub card_minutes: Option<i64>,
    pub host: &'a str,
}

/// `YYYY-MM` in UTC.
pub fn month_of(t: OffsetDateTime) -> String {
    let t = t.to_offset(time::UtcOffset::UTC);
    format!("{:04}-{:02}", t.year(), u8::from(t.month()))
}

const MSG_COLS: &str = "id, thread_id, role, text, attached_context, scope_kind, status, error_kind, \
     error_message, input_tokens, output_tokens, profile_id, model, host, created_utc, \n     error_reason_by, error_reason, no_stream";

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
        error_reason_by: r.get(15)?,
        error_reason: r.get(16)?,
        no_stream: r.get::<_, i64>(17)? != 0,
    })
}

const PROFILE_COLS: &str = "p.id, p.kind, p.label, p.base_url, \
     COALESCE((SELECT m.model FROM companion_models m WHERE m.profile_id = p.id AND m.is_default = 1), ''), \
     p.monthly_token_budget, p.is_default";

const MODEL_COLS: &str = "id, profile_id, model, label, is_default, created_utc";

fn model_row(r: &Row<'_>) -> rusqlite::Result<CompanionModel> {
    Ok(CompanionModel {
        id: r.get(0)?,
        profile_id: r.get(1)?,
        model: r.get(2)?,
        label: r.get(3)?,
        is_default: r.get::<_, i64>(4)? != 0,
        created_utc: r.get(5)?,
    })
}

fn clean_model_label(label: Option<&str>) -> Result<Option<String>> {
    let Some(l) = label else { return Ok(None) };
    let l: String = l.chars().filter(|c| !c.is_control()).collect();
    let l = l.trim();
    if l.is_empty() {
        return Ok(None);
    }
    if l.chars().count() > MAX_MODEL_LABEL_CHARS {
        return Err(StoreError::InvalidInput("model label length"));
    }
    Ok(Some(l.to_owned()))
}

/// Adds `model` to a provider (keeping an existing row) and, if asked, makes it the default.
/// `label`: `None` leaves an existing label alone; `Some(x)` sets it (empty clears it).
fn put_model_on(
    tx: &rusqlite::Transaction<'_>,
    profile_id: &str,
    model: &str,
    label: Option<Option<&str>>,
    make_default: bool,
    ts: &str,
) -> Result<()> {
    let known: bool = tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM companion_models WHERE profile_id = ?1 AND model = ?2)",
        params![profile_id, model],
        |r| r.get(0),
    )?;
    if !known {
        let n: i64 = tx.query_row(
            "SELECT count(*) FROM companion_models WHERE profile_id = ?1",
            [profile_id],
            |r| r.get(0),
        )?;
        if n as usize >= MAX_MODELS_PER_PROFILE {
            return Err(StoreError::TooMany {
                limit: MAX_MODELS_PER_PROFILE,
            });
        }
        tx.execute(
            "INSERT INTO companion_models (profile_id, model, label, is_default, created_utc)
             VALUES (?1, ?2, ?3, 0, ?4)",
            params![profile_id, model, clean_model_label(label.flatten())?, ts],
        )
        .map_err(check_invalid)?;
        if n == 0 {
            set_default_model_on(tx, profile_id, model)?;
        }
    } else if let Some(l) = label {
        tx.execute(
            "UPDATE companion_models SET label = ?3 WHERE profile_id = ?1 AND model = ?2",
            params![profile_id, model, clean_model_label(l)?],
        )?;
    }
    if make_default {
        set_default_model_on(tx, profile_id, model)?;
    }
    Ok(())
}

fn set_default_model_on(
    tx: &rusqlite::Transaction<'_>,
    profile_id: &str,
    model: &str,
) -> Result<()> {
    let known: bool = tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM companion_models WHERE profile_id = ?1 AND model = ?2)",
        params![profile_id, model],
        |r| r.get(0),
    )?;
    if !known {
        return Err(StoreError::NotFound);
    }
    tx.execute(
        "UPDATE companion_models SET is_default = 0 WHERE profile_id = ?1 AND model <> ?2",
        params![profile_id, model],
    )?;
    tx.execute(
        "UPDATE companion_models SET is_default = 1 WHERE profile_id = ?1 AND model = ?2",
        params![profile_id, model],
    )?;
    Ok(())
}

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
            "SELECT {PROFILE_COLS} FROM companion_profiles p ORDER BY p.is_default DESC, p.created_utc, p.id"
        ))?;
        let rows = stmt.query_map([], profile_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn companion_profile(&self, id: &str) -> Result<Option<CompanionProfile>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {PROFILE_COLS} FROM companion_profiles p WHERE p.id = ?1"),
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
        if p.model.trim().is_empty() {
            return Err(StoreError::InvalidInput("a provider needs a model"));
        }
        tx.execute(
            "INSERT INTO companion_profiles
               (id, kind, label, base_url, monthly_token_budget, is_default, created_utc, updated_utc)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
             ON CONFLICT (id) DO UPDATE SET kind = excluded.kind, label = excluded.label,
               base_url = excluded.base_url,
               monthly_token_budget = excluded.monthly_token_budget,
               is_default = CASE WHEN ?6 = 1 THEN 1 ELSE companion_profiles.is_default END,
               updated_utc = excluded.updated_utc",
            params![
                p.id,
                p.kind,
                p.label,
                p.base_url,
                p.monthly_token_budget,
                i64::from(make_default),
                ts
            ],
        )
        .map_err(check_invalid)?;
        put_model_on(&tx, &p.id, &p.model, None, true, &ts)?;
        let out = tx.query_row(
            &format!("SELECT {PROFILE_COLS} FROM companion_profiles p WHERE p.id = ?1"),
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

    // ---- models (decision 36) ------------------------------------------------------------

    /// Models of one provider (or of every provider), default first, then oldest first.
    pub fn companion_models(&self, profile_id: Option<&str>) -> Result<Vec<CompanionModel>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {MODEL_COLS} FROM companion_models
             WHERE ?1 IS NULL OR profile_id = ?1
             ORDER BY profile_id, is_default DESC, created_utc, id"
        ))?;
        let rows = stmt.query_map([profile_id], model_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn companion_model(&self, profile_id: &str, model: &str) -> Result<Option<CompanionModel>> {
        Ok(self
            .conn
            .query_row(
                &format!(
                    "SELECT {MODEL_COLS} FROM companion_models WHERE profile_id = ?1 AND model = ?2"
                ),
                params![profile_id, model],
                model_row,
            )
            .optional()?)
    }

    /// Adds a model to a provider, or updates its label if it is already listed. The first model
    /// of a provider, or one added with `make_default`, becomes its default. The caller checks
    /// the model id with the same rule as typed names.
    pub fn companion_add_model(
        &mut self,
        profile_id: &str,
        model: &str,
        label: Option<&str>,
        make_default: bool,
        now: OffsetDateTime,
    ) -> Result<CompanionModel> {
        let ts = fmt_ts(now)?;
        let tx = self.conn.transaction()?;
        let known: bool = tx.query_row(
            "SELECT EXISTS (SELECT 1 FROM companion_profiles WHERE id = ?1)",
            [profile_id],
            |r| r.get(0),
        )?;
        if !known {
            return Err(StoreError::NotFound);
        }
        put_model_on(&tx, profile_id, model, Some(label), make_default, &ts)?;
        let out = tx.query_row(
            &format!(
                "SELECT {MODEL_COLS} FROM companion_models WHERE profile_id = ?1 AND model = ?2"
            ),
            params![profile_id, model],
            model_row,
        )?;
        tx.commit()?;
        Ok(out)
    }

    /// Removes a model from a provider. A provider keeps at least one model; if the default goes,
    /// the oldest remaining model becomes the default.
    pub fn companion_remove_model(&mut self, profile_id: &str, model: &str) -> Result<()> {
        let tx = self.conn.transaction()?;
        let known: bool = tx.query_row(
            "SELECT EXISTS (SELECT 1 FROM companion_models WHERE profile_id = ?1 AND model = ?2)",
            params![profile_id, model],
            |r| r.get(0),
        )?;
        if !known {
            return Err(StoreError::NotFound);
        }
        let n: i64 = tx.query_row(
            "SELECT count(*) FROM companion_models WHERE profile_id = ?1",
            [profile_id],
            |r| r.get(0),
        )?;
        if n <= 1 {
            return Err(StoreError::InvalidInput(LAST_MODEL_REFUSED));
        }
        tx.execute(
            "DELETE FROM companion_models WHERE profile_id = ?1 AND model = ?2",
            params![profile_id, model],
        )?;
        tx.execute(
            "UPDATE companion_models SET is_default = 1
             WHERE id = (SELECT id FROM companion_models WHERE profile_id = ?1
                         ORDER BY created_utc, id LIMIT 1)
               AND NOT EXISTS (SELECT 1 FROM companion_models
                               WHERE profile_id = ?1 AND is_default = 1)",
            [profile_id],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Makes `model` the provider's default model.
    pub fn companion_set_default_model(&mut self, profile_id: &str, model: &str) -> Result<()> {
        let tx = self.conn.transaction()?;
        set_default_model_on(&tx, profile_id, model)?;
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
        self.companion_complete_send_booked(
            user_message,
            reply,
            input_tokens,
            output_tokens,
            input_tokens,
            output_tokens,
            now,
        )
    }

    /// Like [`Self::companion_complete_send`], for a reply that took several provider calls: the
    /// message shows the totals (`input_tokens`, `output_tokens`), while only the last call's
    /// tokens (`booked_*`) are recorded as usage here, because the earlier calls were booked as
    /// they finished.
    #[allow(clippy::too_many_arguments)]
    pub fn companion_complete_send_booked(
        &mut self,
        user_message: i64,
        reply: &str,
        input_tokens: i64,
        output_tokens: i64,
        booked_input: i64,
        booked_output: i64,
        now: OffsetDateTime,
    ) -> Result<CompanionMessage> {
        self.companion_complete_send_if(
            user_message,
            reply,
            input_tokens,
            output_tokens,
            booked_input,
            booked_output,
            now,
            &|| true,
        )?
        .ok_or(StoreError::NotFound)
    }

    /// Like [`Self::companion_complete_send_booked`], but `proceed` is asked inside the
    /// transaction, before anything is written: if it says no, nothing is stored (the user
    /// message stays as it was, no reply, no usage) and `None` comes back. A cancelled request
    /// passes its token check here so that a worker which was past an earlier check cannot book
    /// a reply after the user already sent a newer message.
    #[allow(clippy::too_many_arguments)]
    pub fn companion_complete_send_if(
        &mut self,
        user_message: i64,
        reply: &str,
        input_tokens: i64,
        output_tokens: i64,
        booked_input: i64,
        booked_output: i64,
        now: OffsetDateTime,
        proceed: &dyn Fn() -> bool,
    ) -> Result<Option<CompanionMessage>> {
        self.companion_complete_send_flagged(
            user_message,
            reply,
            (input_tokens, output_tokens),
            (booked_input, booked_output),
            now,
            false,
            proceed,
        )
    }

    /// Like [`Self::companion_complete_send_if`], with `no_stream` set on the reply when it
    /// arrived all at once because the request was not streamed (a delivery note, never part of
    /// the text). Tokens are `(shown, booked)` pairs of (input, output).
    pub fn companion_complete_send_flagged(
        &mut self,
        user_message: i64,
        reply: &str,
        shown: (i64, i64),
        booked: (i64, i64),
        now: OffsetDateTime,
        no_stream: bool,
        proceed: &dyn Fn() -> bool,
    ) -> Result<Option<CompanionMessage>> {
        let (input_tokens, output_tokens) = shown;
        let (booked_input, booked_output) = booked;
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
        if !proceed() {
            return Ok(None);
        }
        tx.execute(
            "UPDATE companion_messages SET status = 'sent', error_kind = NULL, error_message = NULL,
               error_reason_by = NULL, error_reason = NULL,
               input_tokens = ?2, output_tokens = ?3 WHERE id = ?1",
            params![user_message, input_tokens, output_tokens],
        )?;
        tx.execute(
            "INSERT INTO companion_messages
               (thread_id, role, text, scope_kind, status, output_tokens, profile_id, model, host,
                created_utc, no_stream)
             VALUES (?1, 'assistant', ?2, 'none', 'sent', ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                thread,
                reply,
                output_tokens,
                profile,
                model,
                host,
                ts,
                i64::from(no_stream)
            ],
        )
        .map_err(check_invalid)?;
        let id = tx.last_insert_rowid();
        if let Some(p) = &profile {
            record_usage_on(&tx, p, now, booked_input, booked_output)?;
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
        Ok(Some(out))
    }

    /// Stores a lookup the user approved or declined, for a user message of this thread.
    pub fn companion_add_tool_call(&self, c: &NewToolCall<'_>, now: OffsetDateTime) -> Result<i64> {
        self.conn
            .execute(
                "INSERT INTO companion_tool_calls
                   (message_id, round, tool, arguments, result_text, status, card_activity_id,
                    card_name, card_minutes, host, created_utc)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    c.message_id,
                    c.round,
                    c.tool,
                    c.arguments,
                    c.result_text,
                    c.status,
                    c.card_activity_id,
                    c.card_name,
                    c.card_minutes,
                    c.host,
                    fmt_ts(now)?
                ],
            )
            .map_err(check_invalid)?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Stores a notice for a call that was not run, for a user message of this thread.
    pub fn companion_add_tool_notice(
        &self,
        n: &NewToolNotice<'_>,
        now: OffsetDateTime,
    ) -> Result<i64> {
        self.conn
            .execute(
                "INSERT INTO companion_tool_notices (message_id, round, reason, tool, created_utc)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![n.message_id, n.round, n.reason, n.tool, fmt_ts(now)?],
            )
            .map_err(check_invalid)?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Every notice recorded in a thread, oldest first.
    pub fn companion_tool_notices(&self, thread_id: i64) -> Result<Vec<CompanionToolNotice>> {
        let mut stmt = self.conn.prepare(
            "SELECT n.id, n.message_id, n.round, n.reason, n.tool, n.created_utc
             FROM companion_tool_notices n JOIN companion_messages m ON m.id = n.message_id
             WHERE m.thread_id = ?1 ORDER BY n.id",
        )?;
        let rows = stmt.query_map([thread_id], |r| {
            Ok(CompanionToolNotice {
                id: r.get(0)?,
                message_id: r.get(1)?,
                round: r.get(2)?,
                reason: r.get(3)?,
                tool: r.get(4)?,
                created_utc: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Replaces the stored text of session overview lookups with a fixed note, because that text
    /// can carry a session label taken from captured task text. Called when task-text capture is
    /// turned off, when captured text is deleted, and (with `before`) when retention passes. The
    /// rows stay, so the conversation still shows that a lookup happened. Returns how many rows
    /// were cleared.
    pub fn companion_clear_overview_results(
        &self,
        before: Option<OffsetDateTime>,
    ) -> Result<usize> {
        let n = match before {
            Some(t) => self.conn.execute(
                "UPDATE companion_tool_calls SET result_text = ?1
                 WHERE tool = 'session_overview' AND result_text <> ?1
                   AND unixepoch(created_utc) < ?2",
                params![CLEARED_RESULT_TEXT, t.unix_timestamp()],
            )?,
            None => self.conn.execute(
                "UPDATE companion_tool_calls SET result_text = ?1
                 WHERE tool = 'session_overview' AND result_text <> ?1",
                [CLEARED_RESULT_TEXT],
            )?,
        };
        Ok(n)
    }

    /// Removes the context text attached to earlier messages that came from session summaries
    /// (which can hold captured task text). The messages and their own text stay.
    pub fn companion_clear_attached_session_context(&self) -> Result<usize> {
        Ok(self.conn.execute(
            "UPDATE companion_messages SET attached_context = NULL
             WHERE attached_context IS NOT NULL AND scope_kind = 'sessionSummary'",
            [],
        )?)
    }

    /// Every lookup recorded in a thread, oldest first.
    pub fn companion_tool_calls(&self, thread_id: i64) -> Result<Vec<CompanionToolCall>> {
        let mut stmt = self.conn.prepare(
            "SELECT t.id, t.message_id, t.round, t.tool, t.arguments, t.result_text, t.status,
                    t.card_activity_id, t.card_name, t.card_minutes, t.host, t.created_utc
             FROM companion_tool_calls t JOIN companion_messages m ON m.id = t.message_id
             WHERE m.thread_id = ?1 ORDER BY t.id",
        )?;
        let rows = stmt.query_map([thread_id], |r| {
            Ok(CompanionToolCall {
                id: r.get(0)?,
                message_id: r.get(1)?,
                round: r.get(2)?,
                tool: r.get(3)?,
                arguments: r.get(4)?,
                result_text: r.get(5)?,
                status: r.get(6)?,
                card_activity_id: r.get(7)?,
                card_name: r.get(8)?,
                card_minutes: r.get(9)?,
                host: r.get(10)?,
                created_utc: r.get(11)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Marks a user message `failed` with a fixed kind and friendly message.
    pub fn companion_fail_message(&self, id: i64, kind: &str, message: &str) -> Result<()> {
        self.companion_fail_message_with(id, kind, message, None)
    }

    /// Like [`Self::companion_fail_message`], and keeps what the provider said (`(speaker,
    /// text)`, already sanitised and at most 300 characters) apart from the fixed message.
    pub fn companion_fail_message_with(
        &self,
        id: i64,
        kind: &str,
        message: &str,
        reason: Option<(&str, &str)>,
    ) -> Result<()> {
        let (by, text) = reason.unzip();
        let n = self
            .conn
            .execute(
                "UPDATE companion_messages SET status = 'failed', error_kind = ?2, error_message = ?3,
                   error_reason_by = ?4, error_reason = ?5
                 WHERE id = ?1 AND role = 'user'",
                params![id, kind, message, by, text],
            )
            .map_err(check_invalid)?;
        if n == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Puts a failed user message back to `pending` for a retry.
    pub fn companion_mark_pending(&self, id: i64) -> Result<()> {
        let n = self.conn.execute(
            "UPDATE companion_messages SET status = 'pending', error_kind = NULL, error_message = NULL,
               error_reason_by = NULL, error_reason = NULL
             WHERE id = ?1 AND role = 'user' AND status = 'failed'",
            [id],
        )?;
        if n == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Like [`Self::companion_mark_pending`], and records the provider and model the retry goes to
    /// (decision 36: the user may retry with another model), so the reply is booked against them.
    pub fn companion_mark_pending_for(&self, id: i64, profile_id: &str, model: &str) -> Result<()> {
        let n = self.conn.execute(
            "UPDATE companion_messages SET status = 'pending', error_kind = NULL, error_message = NULL,
               error_reason_by = NULL, error_reason = NULL,
               profile_id = ?2, model = ?3
             WHERE id = ?1 AND role = 'user' AND status = 'failed'",
            params![id, profile_id, model],
        ).map_err(check_invalid)?;
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
