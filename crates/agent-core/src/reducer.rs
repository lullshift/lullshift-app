//! Session state machine (ADR-008). Pure functions: persistence and deduplication by `eventId`
//! live in the store, which calls [`apply`] once per newly stored event.

use serde::Serialize;
use time::{Duration, OffsetDateTime};

use crate::event::{AgentEvent, EventType};
use crate::session::SessionState;

/// Default silence after which a non-terminal session is shown as stale.
pub const DEFAULT_STALE_AFTER: Duration = Duration::minutes(10);

/// What the store keeps per `(provider, providerSessionId)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRecord {
    pub provider: String,
    pub provider_session_id: String,
    /// Last state reported by the provider. `Unknown` only if every event so far was a heartbeat.
    pub state: SessionState,
    /// Provider timestamp of the event that set `state` (`None` while `state` is `Unknown`).
    pub state_timestamp_utc: Option<OffsetDateTime>,
    /// Newest provider timestamp of any event, heartbeats included.
    pub last_seen_utc: OffsetDateTime,
    /// Local clock when the newest event was received. Drives restart and stale rules, so a
    /// skewed provider clock cannot make a session look fresh.
    pub last_received_utc: OffsetDateTime,
}

/// Maps an event type to the state it reports; `None` for liveness-only events.
pub fn reported_state(event_type: EventType) -> Option<SessionState> {
    match event_type {
        EventType::SessionStarted | EventType::TurnEnded => Some(SessionState::Idle),
        EventType::Working => Some(SessionState::Working),
        EventType::NeedsAttention => Some(SessionState::NeedsAttention),
        EventType::Completed => Some(SessionState::Completed),
        EventType::Failed => Some(SessionState::Failed),
        EventType::Cancelled => Some(SessionState::Cancelled),
        EventType::SessionEnded => Some(SessionState::Ended),
        EventType::Heartbeat => None,
    }
}

pub fn is_terminal(state: SessionState) -> bool {
    matches!(
        state,
        SessionState::Completed
            | SessionState::Failed
            | SessionState::Cancelled
            | SessionState::Ended
    )
}

/// Applies one new (already deduplicated) event, received at `received_utc`.
///
/// The state changes only if the event's timestamp is not older than the one that set the
/// current state. Equal timestamps are resolved by receipt order: the event applied later wins.
///
/// A provider timestamp later than the local receipt time is capped at the receipt time, so a
/// future-dated event (skewed clock or hostile hook) can't freeze the state against later events.
pub fn apply(
    current: Option<SessionRecord>,
    event: &AgentEvent,
    received_utc: OffsetDateTime,
) -> SessionRecord {
    let ts = event.timestamp_utc.min(received_utc);
    let mut rec = current.unwrap_or_else(|| SessionRecord {
        provider: event.provider.clone(),
        provider_session_id: event.provider_session_id.clone(),
        state: SessionState::Unknown,
        state_timestamp_utc: None,
        last_seen_utc: ts,
        last_received_utc: received_utc,
    });
    rec.last_seen_utc = rec.last_seen_utc.max(ts);
    rec.last_received_utc = rec.last_received_utc.max(received_utc);
    if let Some(next) = reported_state(event.event_type) {
        let newer = rec.state_timestamp_utc.is_none_or(|prev| ts >= prev);
        if newer {
            rec.state = next;
            rec.state_timestamp_utc = Some(ts);
        }
    }
    rec
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewReason {
    /// The shown state was reported and is current.
    Observed,
    /// Reported before this app started; nothing has confirmed it since.
    UnconfirmedAfterRestart,
    /// No event for longer than the stale timeout.
    Stale,
    /// Only heartbeats so far: alive, state never reported.
    NotReported,
}

/// What the dashboard shows. Derived on demand, never stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionView {
    pub provider: String,
    pub provider_session_id: String,
    pub state: SessionState,
    pub reason: ViewReason,
    pub last_known_state: SessionState,
    #[serde(with = "time::serde::rfc3339")]
    pub last_seen_utc: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub last_received_utc: OffsetDateTime,
}

/// Derives the displayed state. Terminal states are facts and never go stale.
pub fn view(
    rec: &SessionRecord,
    app_started_utc: OffsetDateTime,
    now_utc: OffsetDateTime,
    stale_after: Duration,
) -> SessionView {
    let reason = if rec.state == SessionState::Unknown {
        ViewReason::NotReported
    } else if is_terminal(rec.state) {
        ViewReason::Observed
    } else if rec.last_received_utc < app_started_utc {
        ViewReason::UnconfirmedAfterRestart
    } else if now_utc - rec.last_received_utc > stale_after {
        ViewReason::Stale
    } else {
        ViewReason::Observed
    };
    SessionView {
        provider: rec.provider.clone(),
        provider_session_id: rec.provider_session_id.clone(),
        state: if reason == ViewReason::Observed {
            rec.state
        } else {
            SessionState::Unknown
        },
        reason,
        last_known_state: rec.state,
        last_seen_utc: rec.last_seen_utc,
        last_received_utc: rec.last_received_utc,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::parse_event;
    use time::macros::datetime;

    const T0: OffsetDateTime = datetime!(2026-11-05 10:00:00 UTC);
    /// Receipt times in `run` start here, after every provider timestamp the tests use.
    const R0: OffsetDateTime = datetime!(2026-11-05 11:00:00 UTC);

    fn ev(kind: &str, ts: OffsetDateTime) -> AgentEvent {
        let ts = ts
            .format(&time::format_description::well_known::Rfc3339)
            .expect("format");
        parse_event(&format!(
            r#"{{"schemaVersion":1,"eventId":"6f1c2a52-8a63-4f53-9a0e-1d1f6b7e2c11","provider":"p","providerSessionId":"s","eventType":"{kind}","timestampUtc":"{ts}"}}"#
        ))
        .expect("valid")
    }

    fn run(events: &[(&str, OffsetDateTime)]) -> SessionRecord {
        let mut rec = None;
        for (i, (kind, ts)) in events.iter().enumerate() {
            rec = Some(apply(rec, &ev(kind, *ts), R0 + Duration::seconds(i as i64)));
        }
        rec.expect("at least one event")
    }

    #[test]
    fn maps_event_types_to_states() {
        let s = Duration::seconds(1);
        assert_eq!(run(&[("session_started", T0)]).state, SessionState::Idle);
        assert_eq!(
            run(&[("working", T0), ("turn_ended", T0 + s)]).state,
            SessionState::Idle,
            "turn end is never completion"
        );
        assert_eq!(
            run(&[("working", T0), ("needs_attention", T0 + s)]).state,
            SessionState::NeedsAttention
        );
        for (kind, state) in [
            ("completed", SessionState::Completed),
            ("failed", SessionState::Failed),
            ("cancelled", SessionState::Cancelled),
            ("session_ended", SessionState::Ended),
        ] {
            assert_eq!(run(&[("working", T0), (kind, T0 + s)]).state, state);
        }
    }

    #[test]
    fn older_events_do_not_change_state_but_ties_follow_receipt_order() {
        let s = Duration::seconds(1);
        let rec = run(&[("needs_attention", T0 + s), ("working", T0)]);
        assert_eq!(rec.state, SessionState::NeedsAttention);
        assert_eq!(rec.last_seen_utc, T0 + s);
        assert_eq!(
            run(&[("working", T0), ("needs_attention", T0)]).state,
            SessionState::NeedsAttention
        );
    }

    #[test]
    fn heartbeat_refreshes_liveness_only() {
        let rec = run(&[("working", T0), ("heartbeat", T0 + Duration::minutes(5))]);
        assert_eq!(rec.state, SessionState::Working);
        assert_eq!(rec.state_timestamp_utc, Some(T0));
        assert_eq!(rec.last_seen_utc, T0 + Duration::minutes(5));
        let only = run(&[("heartbeat", T0)]);
        assert_eq!(only.state, SessionState::Unknown);
        let v = view(&only, T0, T0, DEFAULT_STALE_AFTER);
        assert_eq!(
            (v.state, v.reason),
            (SessionState::Unknown, ViewReason::NotReported)
        );
    }

    #[test]
    fn newer_non_terminal_event_reopens_terminal_session() {
        let s = Duration::seconds(1);
        assert_eq!(
            run(&[("completed", T0), ("working", T0 + s)]).state,
            SessionState::Working
        );
        assert_eq!(
            run(&[("completed", T0 + s), ("working", T0)]).state,
            SessionState::Completed
        );
    }

    #[test]
    fn view_is_unknown_after_restart_and_when_stale() {
        let rec = run(&[("working", T0)]);
        let restart = R0 + Duration::hours(1);
        let v = view(&rec, restart, restart, DEFAULT_STALE_AFTER);
        assert_eq!(v.state, SessionState::Unknown);
        assert_eq!(v.reason, ViewReason::UnconfirmedAfterRestart);
        assert_eq!(v.last_known_state, SessionState::Working);

        let fresh = view(&rec, T0, R0 + Duration::minutes(10), DEFAULT_STALE_AFTER);
        assert_eq!(
            (fresh.state, fresh.reason),
            (SessionState::Working, ViewReason::Observed)
        );
        let stale = view(&rec, T0, R0 + Duration::minutes(11), DEFAULT_STALE_AFTER);
        assert_eq!(
            (stale.state, stale.reason),
            (SessionState::Unknown, ViewReason::Stale)
        );
    }

    #[test]
    fn terminal_states_never_go_stale_or_unconfirmed() {
        let rec = run(&[("failed", T0)]);
        let later = T0 + Duration::days(3);
        let v = view(&rec, later, later, DEFAULT_STALE_AFTER);
        assert_eq!(
            (v.state, v.reason),
            (SessionState::Failed, ViewReason::Observed)
        );
    }

    #[test]
    fn future_dated_event_cannot_freeze_state() {
        let far = datetime!(9999-12-31 23:59:59 UTC);
        let mut rec = Some(apply(None, &ev("working", far), T0));
        rec = Some(apply(
            rec,
            &ev("needs_attention", T0 + Duration::seconds(1)),
            T0 + Duration::seconds(1),
        ));
        let rec = rec.expect("record");
        assert_eq!(rec.state, SessionState::NeedsAttention);
        assert!(rec.last_seen_utc <= T0 + Duration::seconds(1));
    }

    #[test]
    fn skewed_provider_clock_does_not_keep_session_fresh() {
        // Provider claims a timestamp far in the future; staleness still follows local receipt.
        let rec = run(&[("working", T0 + Duration::days(365))]);
        let v = view(&rec, T0, R0 + Duration::minutes(30), DEFAULT_STALE_AFTER);
        assert_eq!(v.reason, ViewReason::Stale);
    }
}
