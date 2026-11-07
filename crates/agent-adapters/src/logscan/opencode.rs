//! OpenCode 2.x keeps sessions in a SQLite database, not in log files: `opencode.db` in its data
//! folder (`$XDG_DATA_HOME/opencode`, else `~/.local/share/opencode`, on every platform; or
//! `$OPENCODE_DB`). Source: github.com/anomalyco/opencode tag v2.2.5,
//! `packages/cli/src/database-path.ts`, `packages/util/src/global-roots.ts` and the first
//! migration in `packages/core/src/database/migration/` (table `session`: `id`, `parent_id`,
//! `directory`, `title`, `time_updated` in ms, `time_archived`). The session `id` is the
//! `sessionID` the plugin forwards. OpenCode is not installed on the development machine:
//! **unverified** against a real database, tested with fixture databases only.
//!
//! The database is untrusted input (security review M2, L1):
//! - opened read-only and without following a final link; with `immutable=1` when no `-wal`
//!   file exists, so nothing is created in OpenCode's folder (a WAL database opened plainly
//!   read-only gets new `-shm`/`-wal` files);
//! - defensive mode on, `trusted_schema` off, memory mapping off, `query_only`;
//! - `session` must be an ordinary table (not a view, a virtual table or anything else), so no
//!   schema-supplied SQL runs;
//! - a progress handler aborts the query at the scan's deadline or when the user cancels;
//! - only the newest top-level, non-archived sessions are read, ids over 128 bytes are skipped
//!   in SQL and text columns are cut in SQL.
//!
//! A pre-release channel build may use `opencode-<channel>.db`, which is not looked for. Message
//! parts are not read, so the guess is only Working (updated in the last 2 minutes) or Unknown.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use rusqlite::config::DbConfig;
use rusqlite::{Connection, OpenFlags};
use time::OffsetDateTime;

use super::{Found, Guess, clean_path, clean_title, is_session_id};

/// The URI for an immutable read-only open: every byte that is not plainly safe is
/// percent-encoded, so `?`, `#` and `%` in a folder name can't add URI parameters.
fn immutable_uri(db: &Path) -> Option<String> {
    let text = db.to_str()?;
    let mut path = text.replace('\\', "/");
    // `C:/x` becomes `/C:/x`, the form SQLite expects for a Windows drive path.
    if !path.starts_with('/') {
        path.insert(0, '/');
    }
    let mut out = String::from("file:");
    for b in path.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'/' | b':' | b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out.push_str("?mode=ro&immutable=1");
    Some(out)
}

/// Opens `db` for reading with every guard on. The progress handler stops any statement once
/// `until` has passed or `cancel` is set.
pub(super) fn open_guarded(
    db: &Path,
    until: Instant,
    cancel: Option<Arc<AtomicBool>>,
) -> Result<Connection, ()> {
    let base = OpenFlags::SQLITE_OPEN_READ_ONLY
        | OpenFlags::SQLITE_OPEN_NO_MUTEX
        | OpenFlags::SQLITE_OPEN_NOFOLLOW;
    let mut wal = db.as_os_str().to_owned();
    wal.push("-wal");
    let mut conn = if Path::new(&wal).exists() {
        // A live WAL holds recent writes; immutable would ignore them.
        Connection::open_with_flags(db, base).map_err(|_| ())?
    } else {
        let uri = immutable_uri(db).ok_or(())?;
        // URI parsing is enabled for this one call only, on a URI built above.
        Connection::open_with_flags(uri, base | OpenFlags::SQLITE_OPEN_URI).map_err(|_| ())?
    };
    conn.set_db_config(DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)
        .map_err(|_| ())?;
    conn.set_db_config(DbConfig::SQLITE_DBCONFIG_TRUSTED_SCHEMA, false)
        .map_err(|_| ())?;
    conn.progress_handler(
        1_000,
        Some(move || {
            Instant::now() >= until || cancel.as_ref().is_some_and(|c| c.load(Ordering::Relaxed))
        }),
    )
    .map_err(|_| ())?;
    conn.busy_timeout(Duration::from_millis(500))
        .map_err(|_| ())?;
    for (k, v) in [
        ("trusted_schema", "OFF"),
        ("mmap_size", "0"),
        ("query_only", "ON"),
    ] {
        conn.pragma_update(None, k, v).map_err(|_| ())?;
    }
    Ok(conn)
}

/// True only when `name` is an ordinary table in the main database.
fn is_plain_table(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT type FROM pragma_table_list WHERE schema = 'main' AND name = ?1",
        [name],
        |r| r.get::<_, String>(0),
    )
    .is_ok_and(|t| t == "table")
}

pub(super) fn read(
    db: &Path,
    max: usize,
    now: OffsetDateTime,
    until: Instant,
    cancel: Option<Arc<AtomicBool>>,
) -> Result<Vec<Found>, ()> {
    let conn = open_guarded(db, until, cancel)?;
    if !is_plain_table(&conn, "session") {
        return Err(());
    }
    let mut stmt = conn
        .prepare(
            "SELECT id, substr(directory, 1, 1024), substr(title, 1, 2000), time_updated
             FROM session
             WHERE parent_id IS NULL AND time_archived IS NULL AND length(CAST(id AS BLOB)) <= 128
             ORDER BY time_updated DESC LIMIT ?1",
        )
        .map_err(|_| ())?;
    let limit = i64::try_from(max).unwrap_or(i64::MAX);
    let rows = stmt
        .query_map([limit], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })
        .map_err(|_| ())?;
    let mut out = Vec::new();
    for row in rows {
        // An interrupted query (deadline or cancel) ends the read as unreadable, not partial.
        let (id, dir, title, updated_ms) = match row {
            Ok(r) => r,
            Err(rusqlite::Error::SqliteFailure(e, _))
                if e.code == rusqlite::ErrorCode::OperationInterrupted =>
            {
                return Err(());
            }
            Err(_) => continue,
        };
        if !is_session_id(&id) {
            continue;
        }
        let Ok(updated) = OffsetDateTime::from_unix_timestamp_nanos(
            i128::from(updated_ms).saturating_mul(1_000_000),
        ) else {
            continue;
        };
        let last_activity = updated.min(now);
        let age = Duration::try_from(now - last_activity).unwrap_or_default();
        out.push(Found {
            provider: crate::opencode::ADAPTER_ID,
            session_id: id,
            project_path: dir.as_deref().and_then(clean_path),
            agent_title: title.as_deref().and_then(clean_title),
            first_prompt: None,
            last_activity,
            guess: if age < super::WORKING_WITHIN {
                Guess::Working
            } else {
                Guess::Unknown
            },
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_escapes_everything_that_could_add_parameters() {
        let p = if cfg!(windows) {
            Path::new(r"C:\a b\x?y#z%\opencode.db")
        } else {
            Path::new("/a b/x?y#z%/opencode.db")
        };
        let uri = immutable_uri(p).expect("uri");
        assert!(uri.starts_with("file:/"));
        assert!(uri.ends_with("/opencode.db?mode=ro&immutable=1"));
        let path_part = uri.strip_suffix("?mode=ro&immutable=1").expect("suffix");
        assert!(!path_part.contains(['?', '#', ' ']));
        assert!(
            path_part.contains("%3F") && path_part.contains("%23") && path_part.contains("%25")
        );
    }

    #[test]
    fn a_runaway_query_stops_at_the_deadline() {
        let dir = tempfile::tempdir().expect("tmp");
        let db = dir.path().join("opencode.db");
        Connection::open(&db)
            .expect("db")
            .execute_batch("CREATE TABLE t (x)")
            .expect("schema");
        let started = Instant::now();
        let conn = open_guarded(&db, started + Duration::from_millis(300), None).expect("open");
        let r: rusqlite::Result<i64> = conn.query_row(
            "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c) SELECT count(*) FROM c",
            [],
            |r| r.get(0),
        );
        assert!(r.is_err());
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn a_cancel_stops_a_query() {
        let dir = tempfile::tempdir().expect("tmp");
        let db = dir.path().join("opencode.db");
        Connection::open(&db)
            .expect("db")
            .execute_batch("CREATE TABLE t (x)")
            .expect("schema");
        let cancel = Arc::new(AtomicBool::new(true));
        let conn = open_guarded(&db, Instant::now() + Duration::from_secs(60), Some(cancel))
            .expect("open");
        let r: rusqlite::Result<i64> = conn.query_row(
            "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c) SELECT count(*) FROM c",
            [],
            |r| r.get(0),
        );
        assert!(r.is_err());
    }

    #[test]
    fn the_guarded_connection_cannot_write() {
        let dir = tempfile::tempdir().expect("tmp");
        let db = dir.path().join("opencode.db");
        Connection::open(&db)
            .expect("db")
            .execute_batch("CREATE TABLE t (x)")
            .expect("schema");
        let conn = open_guarded(&db, Instant::now() + Duration::from_secs(5), None).expect("open");
        assert!(conn.execute("INSERT INTO t VALUES (1)", []).is_err());
    }
}
