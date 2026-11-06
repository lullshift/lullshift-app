//! Marketplace and install records (ADR-015, migration 0006).
//!
//! The store only records; fetching, URL policy beyond "no credentials" and file handling live in
//! `crates/marketplace`. Everything here is still validated, because the rows are read back by
//! code that trusts them: commits are 40 lowercase hex, digests 64, ids are length-capped, and a
//! URL carrying userinfo is refused so a token can't be stored by accident.

use rusqlite::{OptionalExtension, Row, params};
use time::OffsetDateTime;

use crate::{Result, Store, StoreError, fmt_ts, parse_ts};

/// The built-in marketplace id. Always present, never removable.
pub const BUILTIN_MARKETPLACE_ID: &str = "lullshift";
pub const MAX_MARKETPLACE_ID_CHARS: usize = 64;
pub const MAX_ACTIVITY_ID_CHARS: usize = 128;
pub const MAX_URL_CHARS: usize = 2048;
pub const MAX_PATH_CHARS: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketplaceRecord {
    pub id: String,
    /// Scrubbed: never contains userinfo.
    pub url: String,
    pub added_at: OffsetDateTime,
    pub last_fetched_commit: Option<String>,
    pub last_fetched_at: Option<OffsetDateTime>,
    pub builtin: bool,
    /// The user confirmed that this marketplace is on their own computer or network
    /// (decision 17); its address may be, or resolve to, a private address.
    pub private_host: bool,
}

/// Whether an installed activity is a code activity or a content pack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityKind {
    App,
    Content,
}

impl ActivityKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::App => "app",
            Self::Content => "content",
        }
    }

    fn parse(s: &str) -> Result<Self> {
        match s {
            "app" => Ok(Self::App),
            "content" => Ok(Self::Content),
            _ => Err(StoreError::Corrupt("stored activity kind")),
        }
    }
}

/// What the installer knows when it installs or updates an activity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewInstalledActivity {
    pub activity_id: String,
    pub marketplace_id: String,
    pub source_url: String,
    pub source_path: String,
    pub commit_sha: String,
    pub manifest_version: String,
    pub kind: ActivityKind,
    /// `id@major`; content packs only.
    pub engine: Option<String>,
    pub permissions: Vec<String>,
    pub digest: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledActivity {
    pub activity_id: String,
    pub marketplace_id: String,
    pub source_url: String,
    pub source_path: String,
    pub commit_sha: String,
    pub manifest_version: String,
    pub kind: ActivityKind,
    pub engine: Option<String>,
    pub permissions: Vec<String>,
    pub digest: String,
    pub size_bytes: u64,
    pub enabled: bool,
    pub installed_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

fn is_lower_hex(s: &str, len: usize) -> bool {
    s.len() == len && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn has_bad_chars(s: &str) -> bool {
    s.chars().any(|c| c.is_control() || c.is_whitespace())
}

fn check_marketplace_id(id: &str) -> Result<()> {
    let ok = !id.is_empty()
        && id.len() <= MAX_MARKETPLACE_ID_CHARS
        && id.bytes().next().is_some_and(|b| b.is_ascii_alphanumeric())
        && id.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_' | b'.')
        });
    if ok {
        Ok(())
    } else {
        Err(StoreError::InvalidInput("marketplace id"))
    }
}

fn check_activity_id(id: &str) -> Result<()> {
    if id.is_empty() || id.len() > MAX_ACTIVITY_ID_CHARS || has_bad_chars(id) {
        return Err(StoreError::InvalidInput("activity id"));
    }
    Ok(())
}

/// Accepts a URL only if it carries no credentials: no userinfo in a `scheme://` URL (any `@`
/// is refused there), and for the scp form `user@host:path` a user part without `:` or `/`.
fn check_url(url: &str) -> Result<()> {
    if url.is_empty() || url.len() > MAX_URL_CHARS || has_bad_chars(url) {
        return Err(StoreError::InvalidInput("url"));
    }
    if url.contains("://") {
        if url.contains('@') {
            return Err(StoreError::InvalidInput("url must not contain credentials"));
        }
    } else if let Some((user, _)) = url.split_once('@')
        && (user.contains(':') || user.contains('/'))
    {
        return Err(StoreError::InvalidInput("url must not contain credentials"));
    }
    Ok(())
}

fn check_commit(commit: &str) -> Result<()> {
    if is_lower_hex(commit, 40) {
        Ok(())
    } else {
        Err(StoreError::InvalidInput("commit must be 40 lowercase hex"))
    }
}

fn validate(a: &NewInstalledActivity) -> Result<()> {
    check_activity_id(&a.activity_id)?;
    check_marketplace_id(&a.marketplace_id)?;
    check_url(&a.source_url)?;
    if a.source_path.len() > MAX_PATH_CHARS || a.source_path.chars().any(char::is_control) {
        return Err(StoreError::InvalidInput("source path"));
    }
    check_commit(&a.commit_sha)?;
    if !is_lower_hex(&a.digest, 64) {
        return Err(StoreError::InvalidInput("digest must be 64 lowercase hex"));
    }
    if a.manifest_version.is_empty() || a.manifest_version.len() > 64 {
        return Err(StoreError::InvalidInput("manifest version"));
    }
    match (a.kind, &a.engine) {
        (ActivityKind::App, Some(_)) => {
            return Err(StoreError::InvalidInput(
                "only content packs name an engine",
            ));
        }
        (_, Some(e)) if e.is_empty() || e.len() > MAX_ACTIVITY_ID_CHARS || has_bad_chars(e) => {
            return Err(StoreError::InvalidInput("engine"));
        }
        _ => {}
    }
    if a.permissions.len() > 64
        || a.permissions
            .iter()
            .any(|p| p.is_empty() || p.len() > 64 || has_bad_chars(p))
    {
        return Err(StoreError::InvalidInput("permissions"));
    }
    if i64::try_from(a.size_bytes).is_err() {
        return Err(StoreError::InvalidInput("size"));
    }
    Ok(())
}

const MARKETPLACE_COLS: &str =
    "id, url, added_at, last_fetched_commit, last_fetched_at, builtin, private_host";

type RawMarketplace = (
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    i64,
    i64,
);

fn marketplace_row(r: &Row<'_>) -> rusqlite::Result<RawMarketplace> {
    Ok((
        r.get(0)?,
        r.get(1)?,
        r.get(2)?,
        r.get(3)?,
        r.get(4)?,
        r.get(5)?,
        r.get(6)?,
    ))
}

fn decode_marketplace(
    (id, url, added, commit, fetched, builtin, private_host): RawMarketplace,
) -> Result<MarketplaceRecord> {
    Ok(MarketplaceRecord {
        id,
        url,
        added_at: parse_ts(&added)?,
        last_fetched_commit: commit,
        last_fetched_at: fetched.as_deref().map(parse_ts).transpose()?,
        builtin: builtin != 0,
        private_host: private_host != 0,
    })
}

const INSTALL_COLS: &str = "activity_id, marketplace_id, source_url, source_path, commit_sha, \
     manifest_version, kind, engine, permissions, digest, size_bytes, enabled, installed_at, \
     updated_at";

type RawInstall = (
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    String,
    String,
    i64,
    i64,
    String,
    String,
);

fn install_row(r: &Row<'_>) -> rusqlite::Result<RawInstall> {
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
        r.get(12)?,
        r.get(13)?,
    ))
}

fn decode_install(raw: RawInstall) -> Result<InstalledActivity> {
    let (
        activity_id,
        marketplace_id,
        source_url,
        source_path,
        commit_sha,
        manifest_version,
        kind,
        engine,
        permissions,
        digest,
        size,
        enabled,
        installed,
        updated,
    ) = raw;
    Ok(InstalledActivity {
        activity_id,
        marketplace_id,
        source_url,
        source_path,
        commit_sha,
        manifest_version,
        kind: ActivityKind::parse(&kind)?,
        engine,
        permissions: serde_json::from_str(&permissions)
            .map_err(|_| StoreError::Corrupt("stored permissions"))?,
        digest,
        size_bytes: u64::try_from(size).map_err(|_| StoreError::Corrupt("stored size"))?,
        enabled: enabled != 0,
        installed_at: parse_ts(&installed)?,
        updated_at: parse_ts(&updated)?,
    })
}

/// Kept saved state for an activity id that has no install record, as seen by an install.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SavedState {
    /// No saved state exists.
    None,
    /// It belongs to this same marketplace and source; the install will use it.
    Own,
    /// It was recorded as belonging to a different marketplace or source; the install clears it.
    Foreign,
    /// State exists but nothing records whose it is (it predates the owner record); the install
    /// keeps it and the review says so.
    Unknown,
}

const STATE_TABLES: [&str; 3] = ["activity_storage", "activity_snapshots", "activity_play"];

fn has_saved_state(conn: &rusqlite::Connection, activity_id: &str) -> Result<bool> {
    for table in STATE_TABLES {
        let found: bool = conn.query_row(
            &format!("SELECT EXISTS (SELECT 1 FROM {table} WHERE activity_id = ?1)"),
            [activity_id],
            |r| r.get(0),
        )?;
        if found {
            return Ok(true);
        }
    }
    Ok(false)
}

fn saved_state_for(
    conn: &rusqlite::Connection,
    activity_id: &str,
    marketplace_id: &str,
    source_url: &str,
) -> Result<SavedState> {
    if !has_saved_state(conn, activity_id)? {
        return Ok(SavedState::None);
    }
    let owner: Option<(String, String)> = conn
        .query_row(
            "SELECT marketplace_id, source_url FROM activity_state_owner WHERE activity_id = ?1",
            [activity_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    Ok(match owner {
        None => SavedState::Unknown,
        Some((m, u)) if m == marketplace_id && u == source_url => SavedState::Own,
        Some(_) => SavedState::Foreign,
    })
}

fn clear_state_tx(tx: &rusqlite::Transaction<'_>, activity_id: &str) -> Result<()> {
    for table in STATE_TABLES {
        tx.execute(
            &format!("DELETE FROM {table} WHERE activity_id = ?1"),
            [activity_id],
        )?;
    }
    tx.execute(
        "DELETE FROM activity_state_owner WHERE activity_id = ?1",
        [activity_id],
    )?;
    Ok(())
}

impl Store {
    /// Adds a marketplace. `lullshift` is reserved. An id that is already active is refused
    /// ([`StoreError::AlreadyExists`]); one that was removed but kept for its installs is
    /// revived with a clean fetch record, but only from the URL it had: a different URL is also
    /// `AlreadyExists`, so installs never silently move to another source.
    pub fn add_marketplace(
        &mut self,
        id: &str,
        url: &str,
        now: OffsetDateTime,
    ) -> Result<MarketplaceRecord> {
        self.add_marketplace_with_host(id, url, false, now)
    }

    /// Like [`Store::add_marketplace`], recording whether the user confirmed the address as being
    /// on their own computer or network (`private_host`, decision 17). A revived row takes the
    /// new answer.
    pub fn add_marketplace_with_host(
        &mut self,
        id: &str,
        url: &str,
        private_host: bool,
        now: OffsetDateTime,
    ) -> Result<MarketplaceRecord> {
        check_marketplace_id(id)?;
        check_url(url)?;
        if id == BUILTIN_MARKETPLACE_ID {
            return Err(StoreError::InvalidInput("marketplace id is reserved"));
        }
        let at = fmt_ts(now)?;
        let tx = self.conn.transaction()?;
        let existing: Option<Option<String>> = tx
            .query_row(
                "SELECT removed_at FROM marketplaces WHERE id = ?1",
                [id],
                |r| r.get(0),
            )
            .optional()?;
        match existing {
            None => {
                tx.execute(
                    "INSERT INTO marketplaces (id, url, added_at, builtin, private_host)
                     VALUES (?1, ?2, ?3, 0, ?4)",
                    params![id, url, at, i64::from(private_host)],
                )?;
            }
            Some(None) => return Err(StoreError::AlreadyExists),
            Some(Some(_)) => {
                // Installs of a removed marketplace take their updates from this row's URL, so
                // a different URL would silently change where they come from (review M1).
                let stored: String =
                    tx.query_row("SELECT url FROM marketplaces WHERE id = ?1", [id], |r| {
                        r.get(0)
                    })?;
                if stored != url {
                    return Err(StoreError::AlreadyExists);
                }
                tx.execute(
                    "UPDATE marketplaces SET added_at = ?2, removed_at = NULL,
                       last_fetched_commit = NULL, last_fetched_at = NULL, private_host = ?3
                     WHERE id = ?1",
                    params![id, at, i64::from(private_host)],
                )?;
            }
        }
        tx.commit()?;
        self.marketplace(id)?.ok_or(StoreError::NotFound)
    }

    /// An active (not removed) marketplace.
    pub fn marketplace(&self, id: &str) -> Result<Option<MarketplaceRecord>> {
        self.conn
            .query_row(
                &format!(
                    "SELECT {MARKETPLACE_COLS} FROM marketplaces WHERE id = ?1 AND removed_at IS NULL"
                ),
                [id],
                marketplace_row,
            )
            .optional()?
            .map(decode_marketplace)
            .transpose()
    }

    /// Active marketplaces, built-in first, then by id.
    pub fn marketplaces(&self) -> Result<Vec<MarketplaceRecord>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {MARKETPLACE_COLS} FROM marketplaces WHERE removed_at IS NULL
             ORDER BY builtin DESC, id"
        ))?;
        let rows = stmt.query_map([], marketplace_row)?;
        rows.map(|r| r.map_err(StoreError::from).and_then(decode_marketplace))
            .collect()
    }

    /// Removes a marketplace. Its installs stay (pinned copies): with installs the row is kept
    /// marked removed, without any it is deleted. The built-in marketplace is refused
    /// ([`StoreError::BuiltinProtected`]); an unknown or already-removed id is `NotFound`.
    pub fn remove_marketplace(&mut self, id: &str, now: OffsetDateTime) -> Result<()> {
        if id == BUILTIN_MARKETPLACE_ID {
            return Err(StoreError::BuiltinProtected);
        }
        let at = fmt_ts(now)?;
        let tx = self.conn.transaction()?;
        let builtin: Option<i64> = tx
            .query_row(
                "SELECT builtin FROM marketplaces WHERE id = ?1 AND removed_at IS NULL",
                [id],
                |r| r.get(0),
            )
            .optional()?;
        match builtin {
            None => return Err(StoreError::NotFound),
            Some(b) if b != 0 => return Err(StoreError::BuiltinProtected),
            Some(_) => {}
        }
        let installs: i64 = tx.query_row(
            "SELECT count(*) FROM installed_activities WHERE marketplace_id = ?1",
            [id],
            |r| r.get(0),
        )?;
        if installs == 0 {
            tx.execute("DELETE FROM marketplaces WHERE id = ?1", [id])?;
        } else {
            tx.execute(
                "UPDATE marketplaces SET removed_at = ?2 WHERE id = ?1",
                params![id, at],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Records the commit a fetch of an active marketplace resolved to.
    pub fn record_fetch(&self, id: &str, commit: &str, at: OffsetDateTime) -> Result<()> {
        check_commit(commit)?;
        let n = self.conn.execute(
            "UPDATE marketplaces SET last_fetched_commit = ?2, last_fetched_at = ?3
             WHERE id = ?1 AND removed_at IS NULL",
            params![id, commit, fmt_ts(at)?],
        )?;
        if n == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Records an install, or replaces the record of an installed activity (the update swap) in
    /// one statement, so a reader sees the old record or the new one. A replace keeps
    /// `installed_at` and the user's enabled choice. An unknown marketplace is `NotFound`.
    pub fn put_installed_activity(
        &self,
        a: &NewInstalledActivity,
        now: OffsetDateTime,
    ) -> Result<InstalledActivity> {
        validate(a)?;
        let permissions = serde_json::to_string(&a.permissions)
            .map_err(|_| StoreError::InvalidInput("permissions"))?;
        let at = fmt_ts(now)?;
        let size = i64::try_from(a.size_bytes).map_err(|_| StoreError::InvalidInput("size"))?;
        self.conn
            .execute(
                "INSERT INTO installed_activities
                   (activity_id, marketplace_id, source_url, source_path, commit_sha,
                    manifest_version, kind, engine, permissions, digest, size_bytes, enabled,
                    installed_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 1, ?12, ?12)
                 ON CONFLICT (activity_id) DO UPDATE SET
                   marketplace_id = excluded.marketplace_id, source_url = excluded.source_url,
                   source_path = excluded.source_path, commit_sha = excluded.commit_sha,
                   manifest_version = excluded.manifest_version, kind = excluded.kind,
                   engine = excluded.engine, permissions = excluded.permissions,
                   digest = excluded.digest, size_bytes = excluded.size_bytes,
                   updated_at = excluded.updated_at",
                params![
                    a.activity_id,
                    a.marketplace_id,
                    a.source_url,
                    a.source_path,
                    a.commit_sha,
                    a.manifest_version,
                    a.kind.as_str(),
                    a.engine,
                    permissions,
                    a.digest,
                    size,
                    at
                ],
            )
            .map_err(|e| match e {
                rusqlite::Error::SqliteFailure(f, _)
                    if f.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_FOREIGNKEY =>
                {
                    StoreError::NotFound
                }
                other => other.into(),
            })?;
        self.installed_activity(&a.activity_id)?
            .ok_or(StoreError::NotFound)
    }

    pub fn installed_activity(&self, activity_id: &str) -> Result<Option<InstalledActivity>> {
        self.conn
            .query_row(
                &format!("SELECT {INSTALL_COLS} FROM installed_activities WHERE activity_id = ?1"),
                [activity_id],
                install_row,
            )
            .optional()?
            .map(decode_install)
            .transpose()
    }

    /// All installed activities, by id.
    pub fn installed_activities(&self) -> Result<Vec<InstalledActivity>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {INSTALL_COLS} FROM installed_activities ORDER BY activity_id"
        ))?;
        let rows = stmt.query_map([], install_row)?;
        rows.map(|r| r.map_err(StoreError::from).and_then(decode_install))
            .collect()
    }

    pub fn set_installed_enabled(
        &self,
        activity_id: &str,
        enabled: bool,
        now: OffsetDateTime,
    ) -> Result<()> {
        let n = self.conn.execute(
            "UPDATE installed_activities SET enabled = ?2, updated_at = ?3 WHERE activity_id = ?1",
            params![activity_id, i64::from(enabled), fmt_ts(now)?],
        )?;
        if n == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Deletes an install record, and a removed marketplace left with no installs. Saved state
    /// is not deleted, but if any exists the install's marketplace and source are recorded as its
    /// owner (review L7). Returns whether a record existed.
    pub fn remove_installed_activity(
        &mut self,
        activity_id: &str,
        now: OffsetDateTime,
    ) -> Result<bool> {
        let tx = self.conn.transaction()?;
        let owner: Option<(String, String)> = tx
            .query_row(
                "SELECT marketplace_id, source_url FROM installed_activities WHERE activity_id = ?1",
                [activity_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let Some((market, source_url)) = owner else {
            return Ok(false);
        };
        tx.execute(
            "DELETE FROM installed_activities WHERE activity_id = ?1",
            [activity_id],
        )?;
        if has_saved_state(&tx, activity_id)? {
            tx.execute(
                "INSERT OR REPLACE INTO activity_state_owner
                   (activity_id, marketplace_id, source_url, recorded_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![activity_id, market, source_url, fmt_ts(now)?],
            )?;
        }
        tx.execute(
            "DELETE FROM marketplaces WHERE id = ?1 AND removed_at IS NOT NULL AND builtin = 0
               AND NOT EXISTS (SELECT 1 FROM installed_activities WHERE marketplace_id = ?1)",
            [&market],
        )?;
        tx.commit()?;
        Ok(true)
    }

    /// Deletes everything the host keeps for one activity: its key-value storage, its saved
    /// state and its play time, in one transaction. Used by "uninstall and delete saved state";
    /// nothing else calls it, and other activities are untouched.
    pub fn clear_activity_state(&mut self, activity_id: &str) -> Result<()> {
        check_activity_id(activity_id)?;
        let tx = self.conn.transaction()?;
        clear_state_tx(&tx, activity_id)?;
        tx.commit()?;
        Ok(())
    }

    /// How kept saved state for `activity_id` relates to an install from `marketplace_id` and
    /// `source_url` (review L7). Read-only.
    pub fn saved_state_for(
        &self,
        activity_id: &str,
        marketplace_id: &str,
        source_url: &str,
    ) -> Result<SavedState> {
        saved_state_for(&self.conn, activity_id, marketplace_id, source_url)
    }

    /// Called when an install is about to be recorded. State recorded as belonging to a different
    /// marketplace or source is deleted (with everything else the host keeps for the id); state
    /// that is the same install's, or whose owner is unknown, is kept. The owner row is dropped
    /// either way, because the new install record is the owner from now on. One transaction.
    pub fn claim_saved_state(
        &mut self,
        activity_id: &str,
        marketplace_id: &str,
        source_url: &str,
    ) -> Result<SavedState> {
        check_activity_id(activity_id)?;
        let tx = self.conn.transaction()?;
        let found = saved_state_for(&tx, activity_id, marketplace_id, source_url)?;
        if found == SavedState::Foreign {
            clear_state_tx(&tx, activity_id)?;
        }
        tx.execute(
            "DELETE FROM activity_state_owner WHERE activity_id = ?1",
            [activity_id],
        )?;
        tx.commit()?;
        Ok(found)
    }

    /// Hides (`disabled`) or shows a bundled activity in the library. Idempotent.
    pub fn set_builtin_disabled(
        &self,
        activity_id: &str,
        disabled: bool,
        now: OffsetDateTime,
    ) -> Result<()> {
        check_activity_id(activity_id)?;
        if disabled {
            self.conn.execute(
                "INSERT INTO activity_disabled (activity_id, disabled_at) VALUES (?1, ?2)
                 ON CONFLICT (activity_id) DO NOTHING",
                params![activity_id, fmt_ts(now)?],
            )?;
        } else {
            self.conn.execute(
                "DELETE FROM activity_disabled WHERE activity_id = ?1",
                [activity_id],
            )?;
        }
        Ok(())
    }

    pub fn is_builtin_disabled(&self, activity_id: &str) -> Result<bool> {
        Ok(self.conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM activity_disabled WHERE activity_id = ?1)",
            [activity_id],
            |r| r.get(0),
        )?)
    }

    /// Ids of disabled bundled activities, sorted.
    pub fn disabled_builtins(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT activity_id FROM activity_disabled ORDER BY activity_id")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}
