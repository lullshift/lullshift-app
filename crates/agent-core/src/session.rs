//! Session state. `reducer` derives it from events (ADR-008).

use serde::{Deserialize, Serialize};

/// `unknown -> idle -> working -> needs_attention | completed | failed | cancelled | ended`
/// (PRD section 3; `ended` added in Sprint 02, ADR-008).
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
    /// Closed by the provider without a completion signal.
    Ended,
}

impl SessionState {
    pub const ALL: [SessionState; 8] = [
        Self::Unknown,
        Self::Idle,
        Self::Working,
        Self::NeedsAttention,
        Self::Completed,
        Self::Failed,
        Self::Cancelled,
        Self::Ended,
    ];

    /// Stable storage name (same as the serde wire name).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Idle => "idle",
            Self::Working => "working",
            Self::NeedsAttention => "needs_attention",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Ended => "ended",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.as_str() == name)
    }
}
