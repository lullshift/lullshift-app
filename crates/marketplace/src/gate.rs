//! The per-OS gate for third-party code activities (ADR-015, TECHNICAL_ARCHITECTURE 7.3 step 6).
//!
//! Code (`kind: app`) from a marketplace other than the built-in one runs in the activity
//! sandbox (ADR-012). The Sprint 03 WebRTC gap was closed in Sprint 12 (S12-T13): the full
//! sandbox proof passes on Windows, and the macOS and Linux settings are in place but their proof
//! has not run. Every OS stays blocked in this release. Flipping a constant is a product-owner
//! decision made after that OS's proof passes; nothing else in the crate reads a setting, a file
//! or the network to decide this.

/// Windows: blocked. The sandbox proof passes in full with WebRTC closed (ADR-012, Sprint 12);
/// unblocking is the product owner's call.
pub const THIRD_PARTY_CODE_WINDOWS: bool = false;
/// macOS: blocked (WebRTC is turned off, but the sandbox proof has not run on macOS).
pub const THIRD_PARTY_CODE_MACOS: bool = false;
/// Linux and everything else: blocked (WebRTC is turned off, but the sandbox proof has not run
/// on Linux).
pub const THIRD_PARTY_CODE_LINUX: bool = false;

/// Whether third-party code activities may be installed on the OS this was built for.
pub const fn third_party_code_allowed() -> bool {
    if cfg!(target_os = "windows") {
        THIRD_PARTY_CODE_WINDOWS
    } else if cfg!(target_os = "macos") {
        THIRD_PARTY_CODE_MACOS
    } else {
        THIRD_PARTY_CODE_LINUX
    }
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
}
