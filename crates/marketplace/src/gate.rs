//! The per-OS gate for third-party code activities (ADR-015, TECHNICAL_ARCHITECTURE 7.3 step 6).
//!
//! Code (`kind: app`) from a marketplace other than the built-in one runs in the activity
//! sandbox (ADR-012). The Sprint 03 WebRTC gap was closed in Sprint 12 (S12-T13): the full
//! sandbox proof passes on Windows, and the macOS and Linux settings are in place but their proof
//! has not run. Every OS stays blocked in this release. Flipping a constant is a product-owner
//! decision made after that OS's proof passes; nothing else in the crate reads a setting, a file
//! or the network to decide this.
//!
//! Two things must both hold: the per-OS constant below, **and** the host's confirmation that
//! WebRTC was read back as off in the webview ([`set_webrtc_confirmed_off`], set by the desktop
//! crate only for a state it verified). So flipping a constant alone cannot enable third-party
//! code; a platform whose WebRTC setting is only asserted (Windows: browser arguments that
//! cannot be read back) stays blocked until a read-back exists.

use std::sync::atomic::{AtomicBool, Ordering};

/// Set by the host once it has confirmed, by reading it back, that WebRTC is off. Starts false.
static WEBRTC_CONFIRMED_OFF: AtomicBool = AtomicBool::new(false);

/// Records whether the host confirmed WebRTC is off in the main webview. Call with `false`
/// whenever the check fails or cannot be made.
pub fn set_webrtc_confirmed_off(confirmed: bool) {
    WEBRTC_CONFIRMED_OFF.store(confirmed, Ordering::SeqCst);
}

/// Windows: blocked. The sandbox proof passes in full with WebRTC closed (ADR-012, Sprint 12);
/// unblocking is the product owner's call.
pub const THIRD_PARTY_CODE_WINDOWS: bool = false;
/// macOS: blocked (WebRTC is turned off, but the sandbox proof has not run on macOS).
pub const THIRD_PARTY_CODE_MACOS: bool = false;
/// Linux and everything else: blocked (WebRTC is turned off, but the sandbox proof has not run
/// on Linux).
pub const THIRD_PARTY_CODE_LINUX: bool = false;

/// The per-OS constant for the OS this was built for.
const fn os_policy() -> bool {
    if cfg!(target_os = "windows") {
        THIRD_PARTY_CODE_WINDOWS
    } else if cfg!(target_os = "macos") {
        THIRD_PARTY_CODE_MACOS
    } else {
        THIRD_PARTY_CODE_LINUX
    }
}

/// Both conditions are required.
const fn combine(policy: bool, webrtc_confirmed_off: bool) -> bool {
    policy && webrtc_confirmed_off
}

/// Whether third-party code activities may be installed on the OS this was built for: the OS
/// constant is on **and** WebRTC was confirmed off in the webview.
pub fn third_party_code_allowed() -> bool {
    combine(os_policy(), WEBRTC_CONFIRMED_OFF.load(Ordering::SeqCst))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocked_on_every_os_by_default() {
        const {
            assert!(!THIRD_PARTY_CODE_WINDOWS && !THIRD_PARTY_CODE_MACOS && !THIRD_PARTY_CODE_LINUX)
        };
        assert!(!third_party_code_allowed());
    }

    #[test]
    fn needs_the_constant_and_the_webrtc_confirmation() {
        assert!(!combine(false, false));
        assert!(!combine(false, true), "confirmation alone is not enough");
        assert!(
            !combine(true, false),
            "flipping the constant alone is not enough"
        );
        assert!(combine(true, true));
    }

    #[test]
    fn confirmation_is_off_until_the_host_sets_it_and_can_be_withdrawn() {
        // Only this test touches the flag; the constants are all false, so the answer stays
        // false either way.
        assert!(!WEBRTC_CONFIRMED_OFF.load(Ordering::SeqCst));
        set_webrtc_confirmed_off(true);
        assert!(WEBRTC_CONFIRMED_OFF.load(Ordering::SeqCst));
        assert!(!third_party_code_allowed(), "constants are still false");
        set_webrtc_confirmed_off(false);
        assert!(!WEBRTC_CONFIRMED_OFF.load(Ordering::SeqCst));
    }
}
