//! Per-launch capability keys and request budgets for the `lullact` scheme (ADR-012).
//!
//! All activities share one origin, so a path alone can't keep one activity from loading
//! another's page or script. Each launch therefore gets a random 128-bit key from
//! `activity_launch`; the frame loads `/<key>/<id>/index.html`, and the scheme handler serves a
//! request only if its key is live and names that same activity id. A new launch of the same
//! activity, or `activity_launch_end`, revokes the older key. The key is a capability held by
//! the host and the one frame it was given to; it is never a secret from that activity.
//!
//! The same table holds a fixed-window request and byte budget per activity, so a loop of
//! requests for a large file can't keep the app reading and hashing files (HTTP 429).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Length of a launch key in hex digits (128 bits).
pub const KEY_LEN: usize = 32;

/// Requests one activity may make per window.
pub const WINDOW_REQUESTS: u32 = 400;
/// Bytes one activity may be sent per window.
pub const WINDOW_BYTES: u64 = 48 * 1024 * 1024;
/// Length of the budget window.
pub const WINDOW: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub requests: u32,
    pub bytes: u64,
    pub window: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            requests: WINDOW_REQUESTS,
            bytes: WINDOW_BYTES,
            window: WINDOW,
        }
    }
}

struct Window {
    start: Instant,
    requests: u32,
    bytes: u64,
}

#[derive(Default)]
struct Inner {
    /// key -> activity id
    keys: HashMap<String, String>,
    windows: HashMap<String, Window>,
    limits: Limits,
}

pub struct Launches {
    inner: Mutex<Inner>,
}

impl Default for Launches {
    fn default() -> Self {
        Self::with_limits(Limits::default())
    }
}

/// True if `key` looks like a launch key (lowercase hex, [`KEY_LEN`] digits).
pub fn valid_key(key: &str) -> bool {
    key.len() == KEY_LEN && key.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

impl Launches {
    pub fn with_limits(limits: Limits) -> Self {
        Self {
            inner: Mutex::new(Inner {
                limits,
                ..Inner::default()
            }),
        }
    }

    /// Replaces the budget limits. For tests.
    #[cfg(test)]
    pub fn set_limits(&self, limits: Limits) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.limits = limits;
            inner.windows.clear();
        }
    }

    /// A fresh key for a launch of `id`. Earlier keys for the same activity stop working.
    pub fn begin(&self, id: &str) -> Result<String, String> {
        let mut buf = [0_u8; KEY_LEN / 2];
        getrandom::fill(&mut buf).map_err(|_| "internal".to_owned())?;
        let key: String = buf.iter().map(|b| format!("{b:02x}")).collect();
        let mut inner = self.inner.lock().map_err(|_| "internal".to_owned())?;
        inner.keys.retain(|_, v| v != id);
        inner.keys.insert(key.clone(), id.to_owned());
        Ok(key)
    }

    /// Revokes every key of `id` (the activity closed or paused).
    pub fn end(&self, id: &str) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.keys.retain(|_, v| v != id);
        }
    }

    /// The activity a live key was issued for.
    pub fn id_of(&self, key: &str) -> Option<String> {
        if !valid_key(key) {
            return None;
        }
        self.inner.lock().ok()?.keys.get(key).cloned()
    }

    /// Counts one request against `id`'s budget. False when the request or byte budget of the
    /// current window is already used up.
    pub fn admit(&self, id: &str, now: Instant) -> bool {
        let Ok(mut inner) = self.inner.lock() else {
            return false;
        };
        let limits = inner.limits;
        let w = inner.windows.entry(id.to_owned()).or_insert(Window {
            start: now,
            requests: 0,
            bytes: 0,
        });
        if now.saturating_duration_since(w.start) >= limits.window {
            *w = Window {
                start: now,
                requests: 0,
                bytes: 0,
            };
        }
        if w.requests >= limits.requests || w.bytes >= limits.bytes {
            return false;
        }
        w.requests += 1;
        true
    }

    /// Counts bytes sent for `id` in the current window.
    pub fn charge(&self, id: &str, bytes: usize) {
        if let Ok(mut inner) = self.inner.lock()
            && let Some(w) = inner.windows.get_mut(id)
        {
            w.bytes = w.bytes.saturating_add(bytes as u64);
        }
    }
}

/// Storage writes one activity may make per [`STORAGE_WINDOW`].
pub const STORAGE_WRITES: u32 = 240;
/// Length of the storage write window.
pub const STORAGE_WINDOW: Duration = Duration::from_secs(60);

/// A fixed-window write budget per activity for `activity_storage_set` and `_remove`, enforced
/// in Rust (the webview's own limits are a convenience). Refusals use the error code
/// `rate_limited`.
pub struct WriteBudget {
    windows: Mutex<HashMap<String, (Instant, u32)>>,
    writes: u32,
    window: Duration,
}

impl Default for WriteBudget {
    fn default() -> Self {
        Self::with_limit(STORAGE_WRITES, STORAGE_WINDOW)
    }
}

impl WriteBudget {
    pub fn with_limit(writes: u32, window: Duration) -> Self {
        Self {
            windows: Mutex::new(HashMap::new()),
            writes,
            window,
        }
    }

    /// Counts one write for `id`; false if the budget of the current window is used up.
    pub fn admit(&self, id: &str, now: Instant) -> bool {
        let Ok(mut windows) = self.windows.lock() else {
            return false;
        };
        let entry = windows.entry(id.to_owned()).or_insert((now, 0));
        if now.saturating_duration_since(entry.0) >= self.window {
            *entry = (now, 0);
        }
        if entry.1 >= self.writes {
            return false;
        }
        entry.1 += 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_writes_are_budgeted_per_activity_per_window() {
        let b = WriteBudget::with_limit(2, Duration::from_secs(60));
        let t0 = Instant::now();
        assert!(b.admit("a", t0) && b.admit("a", t0));
        assert!(!b.admit("a", t0));
        assert!(b.admit("b", t0));
        assert!(b.admit("a", t0 + Duration::from_secs(61)));
    }

    #[test]
    fn keys_are_random_hex_and_bound_to_one_activity() {
        let l = Launches::default();
        let a = l.begin("app.a").unwrap();
        let b = l.begin("app.b").unwrap();
        assert!(valid_key(&a) && valid_key(&b) && a != b);
        assert_eq!(l.id_of(&a).as_deref(), Some("app.a"));
        assert_eq!(l.id_of(&b).as_deref(), Some("app.b"));
        assert_eq!(l.id_of(&"0".repeat(KEY_LEN)), None);
        assert_eq!(l.id_of("short"), None);
    }

    #[test]
    fn a_new_launch_or_an_end_revokes_the_old_key() {
        let l = Launches::default();
        let first = l.begin("app.a").unwrap();
        let second = l.begin("app.a").unwrap();
        assert_eq!(l.id_of(&first), None);
        assert_eq!(l.id_of(&second).as_deref(), Some("app.a"));
        l.end("app.a");
        assert_eq!(l.id_of(&second), None);
    }

    #[test]
    fn the_budget_is_per_activity_and_resets_after_the_window() {
        let l = Launches::with_limits(Limits {
            requests: 3,
            bytes: 100,
            window: Duration::from_secs(10),
        });
        let t0 = Instant::now();
        for _ in 0..3 {
            assert!(l.admit("app.a", t0));
        }
        assert!(!l.admit("app.a", t0));
        assert!(l.admit("app.b", t0));
        assert!(l.admit("app.a", t0 + Duration::from_secs(11)));
        // Bytes: the request that crosses the limit is served, the next is refused.
        assert!(l.admit("app.c", t0));
        l.charge("app.c", 150);
        assert!(!l.admit("app.c", t0));
    }
}
