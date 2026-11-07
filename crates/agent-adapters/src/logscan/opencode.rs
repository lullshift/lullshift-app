//! OpenCode 2.x keeps sessions in a SQLite database, not in log files: `opencode.db` in its data
//! folder (`$XDG_DATA_HOME/opencode`, else `~/.local/share/opencode`, on every platform; or
//! `$OPENCODE_DB`). Source: github.com/anomalyco/opencode tag v2.2.5,
//! `packages/cli/src/database-path.ts`, `packages/util/src/global-roots.ts` and the first
//! migration in `packages/core/src/database/migration/` (table `session`: `id`, `parent_id`,
//! `directory`, `title`, `time_updated` in ms, `time_archived`). The session `id` is the
//! `sessionID` the plugin forwards. OpenCode is not installed on the development machine:
//! **unverified** against a real database, tested with a fixture database only.
//!
//! The database is opened read-only (`query_only`), with a short busy timeout, and only the
//! newest top-level, non-archived sessions are read; text columns are cut in SQL. A pre-release
//! channel build may use `opencode-<channel>.db`, which is not looked for. Message parts are not
//! read, so the guess is only Working (updated in the last 2 minutes) or Unknown.

use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};
use time::OffsetDateTime;

use super::{Found, Guess, clean_path, clean_title, is_session_id};

pub(super) fn read(db: &Path, max: usize, now: OffsetDateTime) -> Result<Vec<Found>, ()> {
    let conn = Connection::open_with_flags(
        db,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .map_err(|_| ())?;
    conn.busy_timeout(Duration::from_millis(500))
        .map_err(|_| ())?;
    conn.pragma_update(None, "query_only", "ON")
        .map_err(|_| ())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, substr(directory, 1, 1024), substr(title, 1, 2000), time_updated
             FROM session
             WHERE parent_id IS NULL AND time_archived IS NULL
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
        let Ok((id, dir, title, updated_ms)) = row else {
            continue;
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
            title: title.as_deref().and_then(clean_title),
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
