//! How long agents usually work (ADR-026 section 3): finished *working* stretches read from the
//! events already stored. Nothing new is recorded and no schema changes. The numbers are about
//! agents, never about the person, and are only used for the "fits your wait" suggestion.

use std::collections::BTreeMap;

use rusqlite::params;
use time::OffsetDateTime;

use crate::{Result, Store, fmt_ts, parse_ts};

/// Fewest finished stretches a tool needs before an estimate is given.
pub const MIN_STRETCHES: usize = 5;
/// A stretch longer than this is not a wait (an agent left running overnight), so it is skipped.
const MAX_STRETCH_SECONDS: i64 = 4 * 60 * 60;

/// One finished working stretch: from a `working` event to the next event that ended it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkStretch {
    pub provider: String,
    pub seconds: i64,
}

/// A tool's typical working stretch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaitEstimate {
    pub provider: String,
    pub median_seconds: i64,
    pub stretches: usize,
}

/// Folds one session's events (oldest first) into finished stretches. A stretch starts at
/// `working` and ends at the first needs-you, turn-ended, completed, failed, cancelled or
/// session-ended event. Heartbeats and session starts say nothing about work and are ignored.
pub fn fold_stretches(events: &[(String, OffsetDateTime)]) -> Vec<i64> {
    let mut out = Vec::new();
    let mut started: Option<OffsetDateTime> = None;
    for (kind, ts) in events {
        match kind.as_str() {
            "working" => {
                started.get_or_insert(*ts);
            }
            "needs_attention" | "turn_ended" | "completed" | "failed" | "cancelled"
            | "session_ended" => {
                if let Some(start) = started.take() {
                    let secs = (*ts - start).whole_seconds();
                    if secs > 0 && secs <= MAX_STRETCH_SECONDS {
                        out.push(secs);
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// Median per tool, only for tools with at least [`MIN_STRETCHES`] stretches. Sorted by tool.
pub fn estimates(stretches: &[WorkStretch]) -> Vec<WaitEstimate> {
    let mut by_tool: BTreeMap<&str, Vec<i64>> = BTreeMap::new();
    for s in stretches {
        by_tool.entry(&s.provider).or_default().push(s.seconds);
    }
    by_tool
        .into_iter()
        .filter(|(_, v)| v.len() >= MIN_STRETCHES)
        .map(|(provider, mut v)| {
            v.sort_unstable();
            let n = v.len();
            let median = if n % 2 == 1 {
                v[n / 2]
            } else {
                (v[n / 2 - 1] + v[n / 2]) / 2
            };
            WaitEstimate {
                provider: provider.to_owned(),
                median_seconds: median,
                stretches: n,
            }
        })
        .collect()
}

type Batch = Vec<(String, OffsetDateTime)>;

fn flush(key: &Option<(String, String)>, batch: &mut Batch, out: &mut Vec<WorkStretch>) {
    if let Some((provider, _)) = key {
        for seconds in fold_stretches(batch) {
            out.push(WorkStretch {
                provider: provider.clone(),
                seconds,
            });
        }
    }
    batch.clear();
}

impl Store {
    /// Finished working stretches of events received at or after `since`, read-only.
    pub fn working_stretches(&self, since: OffsetDateTime) -> Result<Vec<WorkStretch>> {
        let mut stmt = self.conn.prepare(
            "SELECT provider, provider_session_id, event_type, timestamp_utc FROM events
             WHERE unixepoch(received_utc) >= unixepoch(?1)
             ORDER BY provider, provider_session_id, receipt_seq",
        )?;
        let rows = stmt.query_map(params![fmt_ts(since)?], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?;
        let mut out = Vec::new();
        let mut key: Option<(String, String)> = None;
        let mut batch: Batch = Vec::new();
        for row in rows {
            let (provider, session, kind, ts) = row?;
            let next = (provider, session);
            if key.as_ref() != Some(&next) {
                flush(&key, &mut batch, &mut out);
                key = Some(next);
            }
            batch.push((kind, parse_ts(&ts)?));
        }
        flush(&key, &mut batch, &mut out);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    fn at(min: i64) -> OffsetDateTime {
        datetime!(2026-11-05 10:00:00 UTC) + time::Duration::minutes(min)
    }
    fn ev(kind: &str, min: i64) -> (String, OffsetDateTime) {
        (kind.to_owned(), at(min))
    }

    #[test]
    fn a_stretch_runs_from_the_first_working_event_to_the_one_that_ends_it() {
        let events = [
            ev("session_started", 0),
            ev("working", 1),
            ev("heartbeat", 2),
            ev("working", 3),
            ev("needs_attention", 7),
            ev("working", 8),
            ev("turn_ended", 10),
        ];
        assert_eq!(fold_stretches(&events), vec![360, 120]);
    }

    #[test]
    fn an_unfinished_or_overlong_stretch_is_not_counted() {
        assert!(fold_stretches(&[ev("working", 0)]).is_empty());
        assert!(fold_stretches(&[ev("working", 0), ev("completed", 5 * 60)]).is_empty());
        assert!(fold_stretches(&[ev("completed", 0), ev("failed", 1)]).is_empty());
    }

    #[test]
    fn an_estimate_needs_five_stretches() {
        let s = |p: &str, n: i64| WorkStretch {
            provider: p.to_owned(),
            seconds: n,
        };
        let mut all: Vec<_> = [60, 120, 180, 240].iter().map(|n| s("codex", *n)).collect();
        assert!(estimates(&all).is_empty(), "four is not enough");
        all.push(s("codex", 600));
        all.extend([100, 100, 100].map(|n| s("cursor", n)));
        let e = estimates(&all);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].provider, "codex");
        assert_eq!(e[0].median_seconds, 180);
        assert_eq!(e[0].stretches, 5);
    }

    #[test]
    fn an_even_count_takes_the_middle_pair() {
        let all: Vec<_> = [60, 100, 200, 300, 400, 900]
            .iter()
            .map(|n| WorkStretch {
                provider: "claude-code".to_owned(),
                seconds: *n,
            })
            .collect();
        assert_eq!(estimates(&all)[0].median_seconds, 250);
    }

    #[test]
    fn stretches_come_from_stored_events_per_session() {
        use agent_core::parse_event;
        let mut store = Store::open_in_memory().expect("store");
        let send = |store: &mut Store, n: u32, session: &str, kind: &str, min: i64| {
            let ts = fmt_ts(at(min)).expect("ts");
            let e = parse_event(&format!(
                r#"{{"schemaVersion":1,"eventId":"00000000-0000-4000-8000-{n:012}","provider":"claude-code","providerSessionId":"{session}","eventType":"{kind}","timestampUtc":"{ts}"}}"#
            ))
            .expect("event");
            store.ingest(&e, at(min)).expect("ingest");
        };
        send(&mut store, 1, "a", "working", 0);
        send(&mut store, 2, "b", "working", 1);
        send(&mut store, 3, "a", "turn_ended", 4);
        send(&mut store, 4, "b", "completed", 3);
        let mut got: Vec<i64> = store
            .working_stretches(at(-10))
            .expect("stretches")
            .iter()
            .map(|s| s.seconds)
            .collect();
        got.sort_unstable();
        assert_eq!(got, vec![120, 240]);
        assert!(store.working_stretches(at(100)).expect("none").is_empty());
    }
}
