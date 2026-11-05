//! Notification priorities and quiet hours (S03-T01). Pure decisions, unit-tested; `lib.rs`
//! shows the notification.
//!
//! Only three things ever notify, each on a change into that state: a session needs you, a
//! session reported that it finished, or it stopped with an error. A turn ending (Claude Code's
//! `Stop`) is not completion and never notifies (S03-A02). Text names the agent only, never task
//! content.

use agent_core::SessionState;
use serde::{Deserialize, Serialize};

/// Preference key in the store.
pub const PREFS_KEY: &str = "notifications";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationPrefs {
    /// A session needs you (permission prompt, question). On by default.
    pub needs_you: bool,
    /// A session reported that it finished, or stopped with an error. On by default.
    pub finished: bool,
    pub quiet_hours: QuietHours,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QuietHours {
    pub enabled: bool,
    /// Local time, `HH:MM` (24-hour).
    pub start: String,
    pub end: String,
    /// During quiet hours, still tell the user when a session needs them.
    pub allow_needs_you: bool,
}

impl Default for NotificationPrefs {
    fn default() -> Self {
        Self {
            needs_you: true,
            finished: true,
            quiet_hours: QuietHours {
                enabled: false,
                start: "22:00".to_owned(),
                end: "08:00".to_owned(),
                allow_needs_you: true,
            },
        }
    }
}

impl NotificationPrefs {
    /// Checks values that serde can't (time format). Invalid preferences are never stored.
    pub fn validate(&self) -> Result<(), &'static str> {
        minutes(&self.quiet_hours.start).ok_or("invalid_time")?;
        minutes(&self.quiet_hours.end).ok_or("invalid_time")?;
        Ok(())
    }

    /// Stored JSON, or defaults if missing or unreadable (a bad row must not silence alerts).
    pub fn from_stored(json: Option<&str>) -> Self {
        json.and_then(|j| serde_json::from_str::<Self>(j).ok())
            .filter(|p| p.validate().is_ok())
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Notice {
    NeedsYou,
    Completed,
    Failed,
}

impl Notice {
    /// The notice for a session that has just changed into `state`, if any.
    pub fn for_change(state: SessionState) -> Option<Self> {
        match state {
            SessionState::NeedsAttention => Some(Self::NeedsYou),
            SessionState::Completed => Some(Self::Completed),
            SessionState::Failed => Some(Self::Failed),
            // Idle (a turn ended), working, cancelled, ended and unknown never notify.
            _ => None,
        }
    }

    pub fn text(self, provider: &str) -> String {
        let agent = match provider {
            "claude-code" => "A Claude Code session",
            "codex" => "A Codex session",
            "cursor" => "A Cursor session",
            _ => "An agent session",
        };
        match self {
            Self::NeedsYou => format!("{agent} needs you."),
            Self::Completed => format!("{agent} reported that it finished."),
            Self::Failed => format!("{agent} stopped with an error."),
        }
    }
}

/// Whether to show `notice` at local wall-clock minute `now_minutes` (0..1440).
pub fn should_notify(prefs: &NotificationPrefs, notice: Notice, now_minutes: u16) -> bool {
    let wanted = match notice {
        Notice::NeedsYou => prefs.needs_you,
        Notice::Completed | Notice::Failed => prefs.finished,
    };
    if !wanted {
        return false;
    }
    let q = &prefs.quiet_hours;
    if !q.enabled || !in_window(&q.start, &q.end, now_minutes) {
        return true;
    }
    notice == Notice::NeedsYou && q.allow_needs_you
}

/// `HH:MM` to minutes since midnight.
fn minutes(hhmm: &str) -> Option<u16> {
    let (h, m) = hhmm.split_once(':')?;
    if h.len() != 2 || m.len() != 2 {
        return None;
    }
    let (h, m): (u16, u16) = (h.parse().ok()?, m.parse().ok()?);
    (h < 24 && m < 60).then_some(h * 60 + m)
}

/// Start inclusive, end exclusive; a window may cross midnight. Equal start and end is empty.
fn in_window(start: &str, end: &str, now: u16) -> bool {
    let (Some(s), Some(e)) = (minutes(start), minutes(end)) else {
        return false;
    };
    if s <= e {
        (s..e).contains(&now)
    } else {
        now >= s || now < e
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn at(h: u16, m: u16) -> u16 {
        h * 60 + m
    }

    fn quiet(start: &str, end: &str, allow: bool) -> NotificationPrefs {
        NotificationPrefs {
            quiet_hours: QuietHours {
                enabled: true,
                start: start.into(),
                end: end.into(),
                allow_needs_you: allow,
            },
            ..NotificationPrefs::default()
        }
    }

    #[test]
    fn turn_end_and_other_states_never_notify() {
        for s in [
            SessionState::Idle,
            SessionState::Working,
            SessionState::Cancelled,
            SessionState::Ended,
            SessionState::Unknown,
        ] {
            assert_eq!(Notice::for_change(s), None, "{s:?}");
        }
        assert_eq!(
            Notice::for_change(SessionState::NeedsAttention),
            Some(Notice::NeedsYou)
        );
    }

    #[test]
    fn defaults_notify_everything_at_any_time() {
        let p = NotificationPrefs::default();
        for n in [Notice::NeedsYou, Notice::Completed, Notice::Failed] {
            assert!(should_notify(&p, n, at(3, 0)));
        }
    }

    #[test]
    fn switches_turn_each_kind_off() {
        let mut p = NotificationPrefs {
            finished: false,
            ..NotificationPrefs::default()
        };
        assert!(should_notify(&p, Notice::NeedsYou, at(12, 0)));
        assert!(!should_notify(&p, Notice::Completed, at(12, 0)));
        assert!(!should_notify(&p, Notice::Failed, at(12, 0)));
        p.needs_you = false;
        assert!(!should_notify(&p, Notice::NeedsYou, at(12, 0)));
    }

    #[test]
    fn overnight_quiet_hours_hold_back_all_but_needs_you() {
        let p = quiet("22:00", "08:00", true);
        assert!(!should_notify(&p, Notice::Completed, at(23, 30)));
        assert!(!should_notify(&p, Notice::Failed, at(2, 0)));
        assert!(should_notify(&p, Notice::NeedsYou, at(2, 0)));
        // Edges: start is quiet, end is not.
        assert!(!should_notify(&p, Notice::Completed, at(22, 0)));
        assert!(should_notify(&p, Notice::Completed, at(8, 0)));
        assert!(should_notify(&p, Notice::Completed, at(12, 0)));
    }

    #[test]
    fn strict_quiet_hours_hold_back_needs_you_too() {
        let p = quiet("13:00", "14:00", false);
        assert!(!should_notify(&p, Notice::NeedsYou, at(13, 30)));
        assert!(should_notify(&p, Notice::NeedsYou, at(14, 0)));
    }

    #[test]
    fn equal_start_and_end_is_an_empty_window() {
        let p = quiet("09:00", "09:00", false);
        assert!(should_notify(&p, Notice::Completed, at(9, 0)));
    }

    #[test]
    fn invalid_times_are_rejected_and_bad_rows_fall_back_to_defaults() {
        for bad in ["24:00", "7:00", "07:60", "ab:cd", "0700", ""] {
            assert!(quiet(bad, "08:00", true).validate().is_err(), "{bad}");
        }
        assert_eq!(
            NotificationPrefs::from_stored(Some(r#"{"needsYou":false}"#)),
            NotificationPrefs::default(),
            "incomplete row"
        );
        assert_eq!(
            NotificationPrefs::from_stored(Some("not json")),
            NotificationPrefs::default()
        );
        let off = NotificationPrefs {
            needs_you: false,
            ..NotificationPrefs::default()
        };
        let json = serde_json::to_string(&off).expect("json");
        assert_eq!(NotificationPrefs::from_stored(Some(&json)), off);
    }

    #[test]
    fn text_names_only_the_agent() {
        assert_eq!(
            Notice::NeedsYou.text("claude-code"),
            "A Claude Code session needs you."
        );
        assert_eq!(
            Notice::Failed.text("other"),
            "An agent session stopped with an error."
        );
    }
}
