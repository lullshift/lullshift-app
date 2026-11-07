//! How long agents usually work (ADR-026 section 3): finished *working* stretches read from the
//! events already stored. Nothing new is recorded and no schema changes. The numbers are about
//! agents, never about the person, and are only used for the "fits your wait" suggestion.

use std::collections::BTreeMap;

use rusqlite::{OptionalExtension, params};
use time::{Duration, OffsetDateTime};

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

/// One stored event, reduced to what a stretch needs. Holds no task text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaitEvent {
    pub provider: String,
    pub session: String,
    pub kind: String,
    pub at: OffsetDateTime,
}

/// Folds one session's events (oldest first) into finished stretches. A stretch starts at
/// `working` and ends at the first needs-you, turn-ended, completed, failed, cancelled or
/// session-ended event. A `session_started` event resets it. If more than `max_gap` passes
/// between two events of a stretch (the agent went quiet and the app would call it stale), the
/// stretch is dropped and a new one starts at the next `working`, so idle time is never counted.
pub fn fold_stretches(events: &[(String, OffsetDateTime)], max_gap: Duration) -> Vec<i64> {
    let mut out = Vec::new();
    // Start of the stretch, and the last event seen inside it.
    let mut open: Option<(OffsetDateTime, OffsetDateTime)> = None;
    for (kind, ts) in events {
        match kind.as_str() {
            "session_started" => open = None,
            "working" => {
                open = match open {
                    Some((start, last)) if *ts - last <= max_gap => Some((start, *ts)),
                    _ => Some((*ts, *ts)),
                };
            }
            "needs_attention" | "turn_ended" | "completed" | "failed" | "cancelled"
            | "session_ended" => {
                if let Some((start, last)) = open.take() {
                    let secs = (*ts - start).whole_seconds();
                    if *ts - last <= max_gap && secs > 0 && secs <= MAX_STRETCH_SECONDS {
                        out.push(secs);
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// Finished stretches from events grouped by session (any order across sessions, oldest first
/// within one). Pure, so it can run after the store lock is released.
pub fn stretches_from_events(events: &[WaitEvent], max_gap: Duration) -> Vec<WorkStretch> {
    let mut by_session: BTreeMap<(&str, &str), Vec<(String, OffsetDateTime)>> = BTreeMap::new();
    for e in events {
        by_session
            .entry((&e.provider, &e.session))
            .or_default()
            .push((e.kind.clone(), e.at));
    }
    let mut out = Vec::new();
    for ((provider, _), list) in by_session {
        for seconds in fold_stretches(&list, max_gap) {
            out.push(WorkStretch {
                provider: provider.to_owned(),
                seconds,
            });
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

impl Store {
    /// The first `receipt_seq` received at or after `since`, found by binary search (receipts
    /// are written in time order), so only the window is scanned. None if there is none.
    fn first_seq_since(&self, since: OffsetDateTime) -> Result<Option<i64>> {
        let bounds: Option<(i64, i64)> = self
            .conn
            .query_row(
                "SELECT MIN(receipt_seq), MAX(receipt_seq) FROM events",
                [],
                |r| Ok((r.get::<_, Option<i64>>(0)?, r.get::<_, Option<i64>>(1)?)),
            )
            .map(|(lo, hi)| lo.zip(hi))?;
        let Some((mut lo, mut hi)) = bounds else {
            return Ok(None);
        };
        let received_at = |seq: i64| -> Result<Option<(i64, OffsetDateTime)>> {
            let row: Option<(i64, String)> = self
                .conn
                .query_row(
                    "SELECT receipt_seq, received_utc FROM events WHERE receipt_seq >= ?1
                     ORDER BY receipt_seq LIMIT 1",
                    [seq],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            row.map(|(s, t)| Ok((s, parse_ts(&t)?))).transpose()
        };
        // Smallest seq whose receipt time is not before `since`.
        let mut found = None;
        while lo <= hi {
            let mid = lo + (hi - lo) / 2;
            match received_at(mid)? {
                None => hi = mid - 1,
                Some((seq, t)) => {
                    if t >= since {
                        found = Some(seq);
                        hi = mid - 1;
                    } else {
                        lo = seq + 1;
                    }
                }
            }
        }
        Ok(found)
    }

    /// Working-related events received at or after `since` (no heartbeats), read-only. Only the
    /// window is scanned. Hand the result to [`stretches_from_events`] after releasing the lock.
    pub fn wait_events(&self, since: OffsetDateTime) -> Result<Vec<WaitEvent>> {
        let Some(first) = self.first_seq_since(since)? else {
            return Ok(Vec::new());
        };
        let mut stmt = self.conn.prepare(
            "SELECT provider, provider_session_id, event_type, timestamp_utc FROM events
             WHERE receipt_seq >= ?1 AND event_type <> 'heartbeat'
               AND unixepoch(received_utc) >= unixepoch(?2)
             ORDER BY receipt_seq",
        )?;
        let rows = stmt.query_map(params![first, fmt_ts(since)?], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?;
        rows.map(|r| {
            let (provider, session, kind, ts) = r?;
            Ok(WaitEvent {
                provider,
                session,
                kind,
                at: parse_ts(&ts)?,
            })
        })
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    const GAP: Duration = Duration::seconds(600);

    fn at(min: i64) -> OffsetDateTime {
        datetime!(2026-11-05 10:00:00 UTC) + Duration::minutes(min)
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
        assert_eq!(fold_stretches(&events, GAP), vec![360, 120]);
    }

    #[test]
    fn idle_time_without_an_end_event_is_never_counted() {
        // Working at 0, then nothing for an hour, then work again and a turn end at 62.
        let events = [ev("working", 0), ev("working", 60), ev("turn_ended", 62)];
        assert_eq!(fold_stretches(&events, GAP), vec![120]);
        // An end event after a long silence is not a stretch either.
        assert!(fold_stretches(&[ev("working", 0), ev("turn_ended", 62)], GAP).is_empty());
        // A gap exactly at the threshold still counts.
        assert_eq!(
            fold_stretches(
                &[ev("working", 0), ev("working", 10), ev("turn_ended", 11)],
                GAP
            ),
            vec![660]
        );
    }

    #[test]
    fn a_new_session_start_resets_an_open_stretch() {
        let events = [
            ev("working", 0),
            ev("session_started", 1),
            ev("working", 2),
            ev("turn_ended", 4),
        ];
        assert_eq!(fold_stretches(&events, GAP), vec![120]);
    }

    #[test]
    fn an_unfinished_or_overlong_stretch_is_not_counted() {
        assert!(fold_stretches(&[ev("working", 0)], GAP).is_empty());
        let long = Duration::hours(24);
        assert!(fold_stretches(&[ev("working", 0), ev("completed", 5 * 60)], long).is_empty());
        assert!(fold_stretches(&[ev("completed", 0), ev("failed", 1)], GAP).is_empty());
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

    fn send(store: &mut Store, n: u32, session: &str, kind: &str, min: i64, received: i64) {
        use agent_core::parse_event;
        let ts = fmt_ts(at(min)).expect("ts");
        let e = parse_event(&format!(
            r#"{{"schemaVersion":1,"eventId":"00000000-0000-4000-8000-{n:012}","provider":"claude-code","providerSessionId":"{session}","eventType":"{kind}","timestampUtc":"{ts}"}}"#
        ))
        .expect("event");
        store.ingest(&e, at(received)).expect("ingest");
    }

    fn secs(store: &Store, since: OffsetDateTime) -> Vec<i64> {
        let events = store.wait_events(since).expect("events");
        let mut got: Vec<i64> = stretches_from_events(&events, GAP)
            .iter()
            .map(|s| s.seconds)
            .collect();
        got.sort_unstable();
        got
    }

    #[test]
    fn stretches_come_from_stored_events_per_session_without_heartbeats() {
        let mut store = Store::open_in_memory().expect("store");
        send(&mut store, 1, "a", "working", 0, 0);
        send(&mut store, 2, "b", "working", 1, 1);
        send(&mut store, 3, "a", "heartbeat", 2, 2);
        send(&mut store, 4, "a", "turn_ended", 4, 4);
        send(&mut store, 5, "b", "completed", 3, 3);
        let events = store.wait_events(at(-10)).expect("events");
        assert!(events.iter().all(|e| e.kind != "heartbeat"));
        assert_eq!(secs(&store, at(-10)), vec![120, 240]);
        assert!(store.wait_events(at(100)).expect("none").is_empty());
        let empty = Store::open_in_memory().expect("empty");
        assert!(empty.wait_events(at(0)).expect("ok").is_empty());
    }

    #[test]
    fn only_events_inside_the_window_count_and_the_boundary_is_inclusive() {
        let mut store = Store::open_in_memory().expect("store");
        let day = 24 * 60;
        let since = at(0) - Duration::days(14);
        // Minutes relative to now: `edge` is exactly 14 days back.
        let m = |days_back: i64, extra: i64| -days_back * day + extra;
        // Before the window: skipped.
        send(&mut store, 1, "old", "working", m(14, -30), m(14, -30));
        send(&mut store, 2, "old", "turn_ended", m(14, -28), m(14, -28));
        // Received exactly at `since`: kept.
        send(&mut store, 3, "edge", "working", m(14, 0), m(14, 0));
        send(&mut store, 4, "edge", "turn_ended", m(14, 3), m(14, 3));
        // A stretch whose start fell before the window has no start in it: not counted.
        send(&mut store, 5, "straddle", "working", m(14, -1), m(14, -1));
        send(&mut store, 6, "straddle", "turn_ended", m(14, 2), m(14, 2));
        send(&mut store, 7, "new", "working", m(1, 0), m(1, 0));
        send(&mut store, 8, "new", "turn_ended", m(1, 5), m(1, 5));
        assert_eq!(secs(&store, since), vec![180, 300]);
    }
}
