//! Session state as shown to the user. The state machine that derives it from events is Sprint 01.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// `unknown -> idle -> working -> needs_attention | completed | failed | cancelled` (PRD section 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Unknown,
    Idle,
    Working,
    NeedsAttention,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSnapshot {
    pub provider: String,
    pub provider_session_id: String,
    pub title: String,
    pub state: SessionState,
    #[serde(with = "time::serde::rfc3339::option")]
    pub last_event_utc: Option<OffsetDateTime>,
}
