//! The per-OS gate for third-party code activities (ADR-015, TECHNICAL_ARCHITECTURE 7.3 step 6).
//!
//! Code (`kind: app`) from a marketplace other than the built-in one runs in the Sprint 03
//! sandbox, whose proof has an open WebRTC gap on Windows and has not run on macOS or Linux. So
//! every OS is blocked in this release. Flipping a constant is a product-owner decision made
//! after the gap is closed; nothing else in the crate reads a setting, a file or the network to
//! decide this.

/// Windows: blocked (open WebRTC gap in the Sprint 03 sandbox proof).
pub const THIRD_PARTY_CODE_WINDOWS: bool = false;
/// macOS: blocked (Sprint 03 sandbox proof has not run).
pub const THIRD_PARTY_CODE_MACOS: bool = false;
/// Linux and everything else: blocked (Sprint 03 sandbox proof has not run).
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
