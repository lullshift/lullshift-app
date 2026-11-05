//! Sprint 00 mock agent feed: fixed sample sessions so the UI can be built before real adapters
//! exist. Clearly synthetic; replaced by adapter-fed state in Sprints 01-02.

use crate::session::{SessionSnapshot, SessionState};
use time::{Duration, OffsetDateTime};

/// Sample sessions covering the states the UI must render, relative to `now`.
pub fn sample_sessions(now: OffsetDateTime) -> Vec<SessionSnapshot> {
    let at = |mins: i64| Some(now - Duration::minutes(mins));
    let s = |id: &str, title: &str, state, last| SessionSnapshot {
        provider: "mock".into(),
        provider_session_id: id.into(),
        title: title.into(),
        state,
        last_event_utc: last,
    };
    vec![
        s(
            "mock-1",
            "Refactor payment parser",
            SessionState::Working,
            at(2),
        ),
        s(
            "mock-2",
            "Write migration tests",
            SessionState::NeedsAttention,
            at(1),
        ),
        s("mock-3", "Update README", SessionState::Completed, at(25)),
        s(
            "mock-4",
            "Upgrade dependencies",
            SessionState::Failed,
            at(40),
        ),
        s("mock-5", "Unreachable session", SessionState::Unknown, None),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn covers_attention_and_unknown_states() {
        let sessions = sample_sessions(OffsetDateTime::UNIX_EPOCH + Duration::days(1));
        assert!(
            sessions
                .iter()
                .any(|s| s.state == SessionState::NeedsAttention)
        );
        assert!(
            sessions
                .iter()
                .any(|s| s.state == SessionState::Unknown && s.last_event_utc.is_none())
        );
    }

    #[test]
    fn serializes_with_ui_field_names() {
        let json = serde_json::to_value(&sample_sessions(OffsetDateTime::UNIX_EPOCH)[1])
            .expect("serialize");
        assert_eq!(json["providerSessionId"], "mock-2");
        assert_eq!(json["state"], "needs_attention");
    }
}
