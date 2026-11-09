//! Model-written sets (ADR-028 section 7): engine data the user's own Companion model wrote on
//! request, kept so it replays offline and resumes like any activity.
//!
//! A set is keyed by a host-made activity id under the reserved `lullshift.generated.`
//! namespace. Its snapshot, activity storage and play time live in the ordinary activity tables
//! under that id; deleting a set removes them in the same transaction. The topic, the data and
//! the model name are never logged: `Debug` for the row type hides them, and this module has no
//! logging.

use std::fmt;

use rusqlite::{OptionalExtension, params};
use time::OffsetDateTime;

use crate::marketplaces::clear_state_tx;
use crate::{Result, Store, StoreError, fmt_ts, parse_ts};

/// The reserved activity-id prefix of every model-written set. Installs and bundling refuse it.
pub const GENERATED_ID_PREFIX: &str = "lullshift.generated.";
/// Sets kept per engine by [`Store::prune_generated_sets`] (owner decision, ADR-028).
pub const KEEP_GENERATED_SETS_PER_ENGINE: usize = 10;

const MAX_ID_CHARS: usize = 128;
const MAX_ENGINE_CHARS: usize = 64;
const MAX_TOPIC_CHARS: usize = 200;
const MAX_DATA_BYTES: usize = 262_144;
const MAX_SKIP_INDEX: u32 = 9_999;

/// A new set to store. The data must already have passed the host's checks.
pub struct NewGeneratedSet<'a> {
    /// The host-made activity id (must start with [`GENERATED_ID_PREFIX`]).
    pub id: &'a str,
    /// The engine as `id@major`, for example `lullshift.quiz@1`.
    pub engine: &'a str,
    pub topic: &'a str,
    pub difficulty: &'a str,
    /// The checked engine data as JSON text.
    pub data_json: &'a str,
    pub created: OffsetDateTime,
    pub provider_host: &'a str,
    pub model: Option<&'a str>,
    /// The pack the set was started from, if any.
    pub origin_pack: Option<&'a str>,
}

/// A stored set.
#[derive(Clone, PartialEq, Eq)]
pub struct GeneratedSet {
    pub id: String,
    pub engine: String,
    pub topic: String,
    pub difficulty: String,
    pub data_json: String,
    pub created: OffsetDateTime,
    pub provider_host: String,
    pub model: Option<String>,
    pub origin_pack: Option<String>,
    /// Set when a load-time check failed; the set is never served again.
    pub unusable: bool,
}

impl fmt::Debug for GeneratedSet {
    /// Topic, data and model name stay out of any debug output or log.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GeneratedSet")
            .field("id", &self.id)
            .field("engine", &self.engine)
            .field("unusable", &self.unusable)
            .finish_non_exhaustive()
    }
}

const COLUMNS: &str = "id, engine, topic, difficulty, data_json, created_utc, provider_host, \
                       model, origin_pack, unusable";

type Raw = (
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    bool,
);

fn raw(r: &rusqlite::Row<'_>) -> rusqlite::Result<Raw> {
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
    ))
}

fn build(r: Raw) -> Result<GeneratedSet> {
    Ok(GeneratedSet {
        id: r.0,
        engine: r.1,
        topic: r.2,
        difficulty: r.3,
        data_json: r.4,
        created: parse_ts(&r.5)?,
        provider_host: r.6,
        model: r.7,
        origin_pack: r.8,
        unusable: r.9,
    })
}

fn check_id(id: &str) -> Result<()> {
    // The manifest id pattern: dot-separated, non-empty segments of [a-z0-9-].
    let ok = id.len() <= MAX_ID_CHARS
        && id.strip_prefix(GENERATED_ID_PREFIX).is_some_and(|t| {
            t.split('.').all(|seg| {
                !seg.is_empty()
                    && seg
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            })
        });
    if ok {
        Ok(())
    } else {
        Err(StoreError::InvalidInput("generated set id"))
    }
}

impl Store {
    /// Stores a new set. The id must be under the reserved namespace and unused
    /// ([`StoreError::AlreadyExists`] otherwise); the data must be JSON within the size cap.
    pub fn insert_generated_set(&mut self, new: &NewGeneratedSet<'_>) -> Result<()> {
        check_id(new.id)?;
        if new.engine.is_empty() || new.engine.len() > MAX_ENGINE_CHARS {
            return Err(StoreError::InvalidInput("engine"));
        }
        if new.topic.chars().count() > MAX_TOPIC_CHARS {
            return Err(StoreError::InvalidInput("topic"));
        }
        if new.data_json.len() > MAX_DATA_BYTES
            || serde_json::from_str::<serde_json::Value>(new.data_json).is_err()
        {
            return Err(StoreError::InvalidInput("set data"));
        }
        let created = fmt_ts(new.created)?;
        let exists: bool = self.conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM generated_sets WHERE id = ?1)",
            [new.id],
            |r| r.get(0),
        )?;
        if exists {
            return Err(StoreError::AlreadyExists);
        }
        self.conn
            .execute(
                "INSERT INTO generated_sets
                   (id, engine, topic, difficulty, data_json, created_utc, provider_host,
                    model, origin_pack, unusable)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0)",
                params![
                    new.id,
                    new.engine,
                    new.topic,
                    new.difficulty,
                    new.data_json,
                    created,
                    new.provider_host,
                    new.model,
                    new.origin_pack
                ],
            )
            .map_err(|e| match e {
                rusqlite::Error::SqliteFailure(f, _)
                    if f.code == rusqlite::ErrorCode::ConstraintViolation =>
                {
                    StoreError::InvalidInput("set fields")
                }
                other => StoreError::Sqlite(other),
            })?;
        Ok(())
    }

    /// One set by id, or `None`.
    pub fn generated_set(&self, id: &str) -> Result<Option<GeneratedSet>> {
        self.conn
            .query_row(
                &format!("SELECT {COLUMNS} FROM generated_sets WHERE id = ?1"),
                [id],
                raw,
            )
            .optional()?
            .map(build)
            .transpose()
    }

    /// A set's engine (`id@major`) and whether it is marked unusable, without reading its data.
    /// For the per-request checks (the scheme handler asks on every file).
    pub fn generated_set_state(&self, id: &str) -> Result<Option<(String, bool)>> {
        Ok(self
            .conn
            .query_row(
                "SELECT engine, unusable FROM generated_sets WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
    }

    /// Saved sets, newest first (by insertion order, so the order does not depend on how a
    /// timestamp string sorts); all, or only those of one engine (`id@major`).
    pub fn generated_sets(&self, engine: Option<&str>) -> Result<Vec<GeneratedSet>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {COLUMNS} FROM generated_sets
             WHERE (?1 IS NULL OR engine = ?1)
             ORDER BY rowid DESC"
        ))?;
        let rows = stmt.query_map([engine], raw)?;
        rows.map(|r| build(r?)).collect()
    }

    /// Marks a set as failing its load-time check. It is never served again; the user can still
    /// delete it. Returns whether the set exists.
    pub fn mark_generated_set_unusable(&self, id: &str) -> Result<bool> {
        Ok(self
            .conn
            .execute("UPDATE generated_sets SET unusable = 1 WHERE id = ?1", [id])?
            > 0)
    }

    /// Records that the user marked item `index` of the set as wrong ("This looks wrong"). A
    /// repeat is a no-op. [`StoreError::NotFound`] if the set does not exist.
    pub fn add_generated_skip(&self, set_id: &str, index: u32) -> Result<()> {
        if index > MAX_SKIP_INDEX {
            return Err(StoreError::InvalidInput("item index"));
        }
        let exists: bool = self.conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM generated_sets WHERE id = ?1)",
            [set_id],
            |r| r.get(0),
        )?;
        if !exists {
            return Err(StoreError::NotFound);
        }
        self.conn.execute(
            "INSERT OR IGNORE INTO generated_set_skips (set_id, item_index) VALUES (?1, ?2)",
            params![set_id, index],
        )?;
        Ok(())
    }

    /// The item indexes marked wrong for a set, ascending.
    pub fn generated_skips(&self, set_id: &str) -> Result<Vec<u32>> {
        let mut stmt = self.conn.prepare(
            "SELECT item_index FROM generated_set_skips WHERE set_id = ?1 ORDER BY item_index",
        )?;
        let rows = stmt.query_map([set_id], |r| r.get::<_, u32>(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Deletes one set with its skips, snapshot, activity storage and play time in one
    /// transaction. Returns whether the set existed. Other activities are untouched.
    pub fn delete_generated_set(&mut self, id: &str) -> Result<bool> {
        let tx = self.conn.transaction()?;
        let removed = delete_set_tx(&tx, id)?;
        tx.commit()?;
        Ok(removed)
    }

    /// Deletes every set, with the same cascade as [`Store::delete_generated_set`], in one
    /// transaction. Returns the ids removed.
    pub fn delete_all_generated_sets(&mut self) -> Result<Vec<String>> {
        let tx = self.conn.transaction()?;
        let ids = ids_tx(&tx, "SELECT id FROM generated_sets", [])?;
        for id in &ids {
            delete_set_tx(&tx, id)?;
        }
        tx.commit()?;
        Ok(ids)
    }

    /// Keeps at most `keep` sets of `engine` (`id@major`): while there are more, the oldest set
    /// **without a saved snapshot** is removed together with its storage, play time and skips.
    /// Sets with a saved snapshot are never removed, though they count toward `keep`. Returns
    /// the ids removed, oldest first.
    pub fn prune_generated_sets(&mut self, engine: &str, keep: usize) -> Result<Vec<String>> {
        let tx = self.conn.transaction()?;
        let total: i64 = tx.query_row(
            "SELECT count(*) FROM generated_sets WHERE engine = ?1",
            [engine],
            |r| r.get(0),
        )?;
        let excess = usize::try_from(total).unwrap_or(0).saturating_sub(keep);
        let mut removed = Vec::new();
        if excess > 0 {
            let candidates = ids_tx(
                &tx,
                "SELECT s.id FROM generated_sets s
                 WHERE s.engine = ?1
                   AND NOT EXISTS (SELECT 1 FROM activity_snapshots a WHERE a.activity_id = s.id)
                 ORDER BY s.rowid ASC
                 LIMIT ?2",
                params![engine, i64::try_from(excess).unwrap_or(i64::MAX)],
            )?;
            for id in candidates {
                delete_set_tx(&tx, &id)?;
                removed.push(id);
            }
        }
        tx.commit()?;
        Ok(removed)
    }
}

fn ids_tx(
    tx: &rusqlite::Transaction<'_>,
    sql: &str,
    p: impl rusqlite::Params,
) -> Result<Vec<String>> {
    let mut stmt = tx.prepare(sql)?;
    let rows = stmt.query_map(p, |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// The set row (its skips cascade) and everything the host keeps for that activity id.
fn delete_set_tx(tx: &rusqlite::Transaction<'_>, id: &str) -> Result<bool> {
    let n = tx.execute("DELETE FROM generated_sets WHERE id = ?1", [id])?;
    // Only ids in the reserved namespace: this can never clear a real activity's state. Done even
    // if the row is already gone, so an orphaned save of a deleted set cannot linger.
    if id.starts_with(GENERATED_ID_PREFIX) {
        clear_state_tx(tx, id)?;
    }
    Ok(n > 0)
}
