//! Data-free "last hook failure" marker (decision 33, ADR-010 amendment).
//!
//! The hook is silent by design, so a hook that can't reach the app leaves no trace. When
//! sending fails because the app refused the token, or because the endpoint named in
//! `endpoint.json` couldn't be reached, the forwarder writes `ipc/last-hook-failure.json` in
//! its data directory:
//!
//! ```json
//! { "adapter": "claude-code", "reason": "rejected", "utc": "2026-11-07T13:05:00Z" }
//! ```
//!
//! Nothing else: no session id, no task data, no paths. The write is atomic (temporary file
//! then rename), bounded in size, skipped if the same adapter and reason were written less than
//! [`RATE_LIMIT`] ago, and every error is ignored. The app treats the file as untrusted: see
//! [`parse`].

use std::fs;
use std::io::{Read, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::{claude_code, codex, cursor, opencode};

/// The marker's file name, inside `<data-dir>/ipc/`.
pub const MARKER_FILE: &str = "last-hook-failure.json";
/// Larger markers are refused unread (a real one is under 100 bytes).
pub const MAX_MARKER_BYTES: u64 = 512;
/// The same adapter and reason are not written again within this time.
pub const RATE_LIMIT: time::Duration = time::Duration::seconds(60);
/// Adapters that may appear in a marker.
pub const KNOWN_ADAPTERS: [&str; 4] = [
    claude_code::ADAPTER_ID,
    codex::ADAPTER_ID,
    cursor::ADAPTER_ID,
    opencode::ADAPTER_ID,
];

/// Why the last hook couldn't deliver its event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FailureReason {
    /// The app answered but refused the token (an out-of-date `token`).
    Rejected,
    /// `endpoint.json` existed but nothing answered at its address, or the exchange failed.
    Unreachable,
}

/// One parsed, validated marker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailureMarker {
    pub adapter: &'static str,
    pub reason: FailureReason,
    pub utc: OffsetDateTime,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Wire {
    adapter: String,
    reason: FailureReason,
    utc: String,
}

/// Parses marker bytes strictly: at most [`MAX_MARKER_BYTES`], exactly the three fields, a known
/// adapter, a known reason and an RFC 3339 time between 2020 and 2200. Anything else is `None`.
pub fn parse(bytes: &[u8]) -> Option<FailureMarker> {
    if bytes.len() as u64 > MAX_MARKER_BYTES {
        return None;
    }
    let wire: Wire = serde_json::from_slice(bytes).ok()?;
    let adapter = KNOWN_ADAPTERS.into_iter().find(|a| *a == wire.adapter)?;
    if wire.utc.len() > 40 {
        return None;
    }
    let utc = OffsetDateTime::parse(&wire.utc, &Rfc3339).ok()?;
    if !(2020..=2200).contains(&utc.year()) {
        return None;
    }
    Some(FailureMarker {
        adapter,
        reason: wire.reason,
        utc,
    })
}

/// Reads and parses the marker in `ipc_dir` if it is a regular file (not a link) of bounded
/// size. Any problem reads as no marker.
pub fn read(ipc_dir: &Path) -> Option<FailureMarker> {
    let path = ipc_dir.join(MARKER_FILE);
    let meta = fs::symlink_metadata(&path).ok()?;
    if !meta.file_type().is_file() || meta.len() > MAX_MARKER_BYTES {
        return None;
    }
    let mut bytes = Vec::new();
    fs::File::open(&path)
        .ok()?
        .take(MAX_MARKER_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    parse(&bytes)
}

/// The marker text for one failure.
pub fn render(adapter: &str, reason: FailureReason, now: OffsetDateTime) -> Option<Vec<u8>> {
    let wire = Wire {
        adapter: adapter.to_owned(),
        reason,
        utc: now
            .replace_nanosecond(0)
            .ok()?
            .to_offset(time::UtcOffset::UTC)
            .format(&Rfc3339)
            .ok()?,
    };
    let bytes = serde_json::to_vec(&wire).ok()?;
    (bytes.len() as u64 <= MAX_MARKER_BYTES).then_some(bytes)
}

/// What [`record`] did; for tests only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recorded {
    Written,
    RateLimited,
    Skipped,
}

/// Writes the marker for `adapter` under `<data_dir>/ipc/`, unless the same adapter and reason
/// were written less than [`RATE_LIMIT`] before `now`. The `ipc` directory must already exist
/// (the forwarder only reports failures after it found connection files there); it is never
/// created. Errors are swallowed: the hook's zero-effect contract comes first.
pub fn record(
    data_dir: &Path,
    adapter: &str,
    reason: FailureReason,
    now: OffsetDateTime,
) -> Recorded {
    if !KNOWN_ADAPTERS.contains(&adapter) {
        return Recorded::Skipped;
    }
    let ipc = data_dir.join(agent_ipc::transport::IPC_DIR);
    if !fs::symlink_metadata(&ipc).is_ok_and(|m| m.file_type().is_dir()) {
        return Recorded::Skipped;
    }
    if let Some(prev) = read(&ipc)
        && prev.adapter == adapter
        && prev.reason == reason
        && now - prev.utc < RATE_LIMIT
        && now >= prev.utc
    {
        return Recorded::RateLimited;
    }
    let Some(bytes) = render(adapter, reason, now) else {
        return Recorded::Skipped;
    };
    // A temporary name of our own (process id and random bytes), created new so an existing
    // file or link of that name is never written through: concurrent hooks can't interleave,
    // and the rename publishes one complete marker.
    let mut nonce = [0u8; 8];
    if getrandom::fill(&mut nonce).is_err() {
        return Recorded::Skipped;
    }
    let hex: String = nonce.iter().map(|b| format!("{b:02x}")).collect();
    let tmp = ipc.join(format!("{MARKER_FILE}.{}.{hex}.tmp", std::process::id()));
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        opts.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let Ok(mut file) = opts.open(&tmp) else {
        return Recorded::Skipped;
    };
    let written = file.write_all(&bytes);
    drop(file);
    if written
        .and_then(|()| fs::rename(&tmp, ipc.join(MARKER_FILE)))
        .is_err()
    {
        let _ = fs::remove_file(&tmp);
        return Recorded::Skipped;
    }
    Recorded::Written
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    fn data_dir() -> tempfile::TempDir {
        let d = tempfile::tempdir().expect("tempdir");
        fs::create_dir(d.path().join("ipc")).expect("ipc");
        d
    }

    #[test]
    fn writes_only_adapter_reason_and_time() {
        let d = data_dir();
        let now = datetime!(2026-11-07 13:05:00.123 UTC);
        assert_eq!(
            record(d.path(), "claude-code", FailureReason::Rejected, now),
            Recorded::Written
        );
        let text = fs::read_to_string(d.path().join("ipc").join(MARKER_FILE)).expect("marker");
        assert_eq!(
            text,
            r#"{"adapter":"claude-code","reason":"rejected","utc":"2026-11-07T13:05:00Z"}"#
        );
        assert!(
            !d.path()
                .join("ipc")
                .join("last-hook-failure.json.tmp")
                .exists()
        );
        let m = read(&d.path().join("ipc")).expect("parsed");
        assert_eq!(
            (m.adapter, m.reason, m.utc),
            (
                "claude-code",
                FailureReason::Rejected,
                datetime!(2026-11-07 13:05:00 UTC)
            )
        );
    }

    #[test]
    fn same_reason_within_a_minute_is_not_rewritten() {
        let d = data_dir();
        let t = datetime!(2026-11-07 13:05:00 UTC);
        let r = |reason, adapter, at| record(d.path(), adapter, reason, at);
        assert_eq!(r(FailureReason::Unreachable, "codex", t), Recorded::Written);
        assert_eq!(
            r(
                FailureReason::Unreachable,
                "codex",
                t + time::Duration::seconds(59)
            ),
            Recorded::RateLimited
        );
        // A different reason or adapter is news and is written at once.
        assert_eq!(
            r(
                FailureReason::Rejected,
                "codex",
                t + time::Duration::seconds(1)
            ),
            Recorded::Written
        );
        assert_eq!(
            r(
                FailureReason::Rejected,
                "cursor",
                t + time::Duration::seconds(2)
            ),
            Recorded::Written
        );
        // After the minute the same reason is written again (a fresh time for the app).
        assert_eq!(
            r(
                FailureReason::Rejected,
                "cursor",
                t + time::Duration::seconds(70)
            ),
            Recorded::Written
        );
        let m = read(&d.path().join("ipc")).expect("marker");
        assert_eq!(m.utc, t + time::Duration::seconds(70));
    }

    /// Review L1: concurrent hooks never leave a marker that doesn't parse, nor temporary files.
    #[test]
    fn concurrent_writers_leave_a_valid_marker() {
        let d = data_dir();
        let base = datetime!(2026-11-07 13:00:00 UTC);
        std::thread::scope(|s| {
            for reason in [FailureReason::Rejected, FailureReason::Unreachable] {
                let dir = d.path();
                s.spawn(move || {
                    for i in 0..1000i64 {
                        // A minute apart, so the rate limit never skips a write.
                        let at = base + time::Duration::seconds(61 * i);
                        let _ = record(dir, "claude-code", reason, at);
                    }
                });
            }
        });
        let ipc = d.path().join("ipc");
        let bytes = fs::read(ipc.join(MARKER_FILE)).expect("marker");
        assert!(
            parse(&bytes).is_some(),
            "{}",
            String::from_utf8_lossy(&bytes)
        );
        let names: Vec<String> = fs::read_dir(&ipc)
            .expect("dir")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, [MARKER_FILE], "no temporary files left");
    }

    #[test]
    fn never_creates_the_ipc_directory_or_writes_unknown_adapters() {
        let d = tempfile::tempdir().expect("tempdir");
        let now = OffsetDateTime::now_utc();
        assert_eq!(
            record(d.path(), "claude-code", FailureReason::Unreachable, now),
            Recorded::Skipped
        );
        assert!(!d.path().join("ipc").exists());
        let d = data_dir();
        assert_eq!(
            record(d.path(), "../evil", FailureReason::Unreachable, now),
            Recorded::Skipped
        );
        assert!(!d.path().join("ipc").join(MARKER_FILE).exists());
    }

    #[test]
    fn parse_is_strict() {
        let ok = br#"{"adapter":"opencode","reason":"unreachable","utc":"2026-11-07T13:05:00Z"}"#;
        assert!(parse(ok).is_some());
        for bad in [
            &br#"{"adapter":"other","reason":"unreachable","utc":"2026-11-07T13:05:00Z"}"#[..],
            br#"{"adapter":"codex","reason":"maybe","utc":"2026-11-07T13:05:00Z"}"#,
            br#"{"adapter":"codex","reason":"rejected","utc":"yesterday"}"#,
            br#"{"adapter":"codex","reason":"rejected","utc":"1999-01-01T00:00:00Z"}"#,
            br#"{"adapter":"codex","reason":"rejected","utc":"2999-01-01T00:00:00Z"}"#,
            br#"{"adapter":"codex","reason":"rejected","utc":"2026-11-07T13:05:00Z","session":"s"}"#,
            br#"{"adapter":"codex","reason":"rejected"}"#,
            br#"[]"#,
            b"\xff\xfe",
        ] {
            assert!(parse(bad).is_none(), "{}", String::from_utf8_lossy(bad));
        }
        let mut big = ok.to_vec();
        big.extend(std::iter::repeat_n(b' ', MAX_MARKER_BYTES as usize));
        assert!(parse(&big).is_none(), "oversized");
    }

    #[test]
    fn oversized_or_non_file_markers_read_as_none() {
        let d = data_dir();
        let ipc = d.path().join("ipc");
        fs::write(ipc.join(MARKER_FILE), vec![b' '; 4096]).expect("write");
        assert!(read(&ipc).is_none());
        fs::remove_file(ipc.join(MARKER_FILE)).expect("rm");
        fs::create_dir(ipc.join(MARKER_FILE)).expect("dir");
        assert!(read(&ipc).is_none());
    }
}
