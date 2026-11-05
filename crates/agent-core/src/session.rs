//! Session state. `reducer` derives it from events (ADR-008).

use serde::{Deserialize, Serialize};

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

impl SessionState {
    pub const ALL: [SessionState; 7] = [
        Self::Unknown,
        Self::Idle,
        Self::Working,
        Self::NeedsAttention,
        Self::Completed,
        Self::Failed,
        Self::Cancelled,
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
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.as_str() == name)
    }
}
