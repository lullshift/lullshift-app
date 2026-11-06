//! Third-party code activities: installed only with the user's explicit consent (owner decision
//! 27.5, ADR-015 and ADR-012 amendments; TECHNICAL_ARCHITECTURE 7.3 step 6).
//!
//! Code (`kind: app`) from a marketplace other than the built-in one runs in the same activity
//! sandbox as bundled activities (ADR-012): the same CSP, the same `lullact:` scheme, the same
//! WebRTC block and the same permissions model. Nothing here loosens that sandbox. What changed
//! is the install gate: instead of a per-OS constant that kept such code out, every install, and
//! every update that changes its pinned commit, needs the user to accept the risk in the review
//! ([`needs_code_consent`]). The host refuses the install or update without that acceptance.
//!
//! What the host knows about the WebRTC block is kept here as information only
//! ([`set_webrtc_block`]). It decides which honest warning the review shows
//! ([`code_risk_status`]); it never opens or closes the gate, and no state is described as a
//! guarantee.

use std::sync::atomic::{AtomicU8, Ordering};

use serde::Serialize;

use crate::pack::Kind;

/// The platform family the WebRTC block depends on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    /// WebView2.
    Windows,
    /// WKWebView.
    MacOs,
    /// WebKitGTK (and anything else that isn't Windows or macOS).
    Linux,
}

impl Os {
    /// The platform this was built for.
    pub const fn current() -> Self {
        if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Linux
        }
    }
}

/// What the host knows about the WebRTC block in its webview. Mirrors the desktop crate's
/// `webrtc::State`; set with [`set_webrtc_block`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum WebrtcBlock {
    /// Not checked yet.
    Unknown = 0,
    /// Turned off and read back from the webview (macOS, Linux).
    ReadBack = 1,
    /// Could not be turned off, or could not be confirmed.
    Failed = 2,
    /// Set when the webview was created, with no way to read it back (Windows).
    Asserted = 3,
}

static WEBRTC_BLOCK: AtomicU8 = AtomicU8::new(WebrtcBlock::Unknown as u8);

/// Records what the host knows about the WebRTC block. Information for the review only.
pub fn set_webrtc_block(state: WebrtcBlock) {
    WEBRTC_BLOCK.store(state as u8, Ordering::SeqCst);
}

/// What the host last recorded about the WebRTC block.
pub fn webrtc_block() -> WebrtcBlock {
    match WEBRTC_BLOCK.load(Ordering::SeqCst) {
        1 => WebrtcBlock::ReadBack,
        2 => WebrtcBlock::Failed,
        3 => WebrtcBlock::Asserted,
        _ => WebrtcBlock::Unknown,
    }
}

/// Which verification note the code-risk warning shows. None of them is a guarantee.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CodeRiskStatus {
    /// Windows: the full sandbox proof passed on a test machine. The WebRTC block is set by
    /// browser arguments that Lullshift can't read back on this computer.
    WindowsNotReadBack,
    /// macOS or Linux: the WebRTC block is in place and was read back, but the sandbox proof has
    /// not yet been run on this platform.
    Unverified,
    /// The WebRTC block could not be applied (or confirmed) on this computer.
    BlockFailed,
    /// Lullshift hasn't checked the block yet, or found a state it didn't expect here.
    NotChecked,
}

/// The note for a platform and a recorded state. A failure always wins; a state that doesn't
/// belong to the platform (say a read-back on Windows) is reported as not checked rather than
/// as something better than it is.
pub const fn code_risk_status_for(os: Os, block: WebrtcBlock) -> CodeRiskStatus {
    match (os, block) {
        (_, WebrtcBlock::Failed) => CodeRiskStatus::BlockFailed,
        (Os::Windows, WebrtcBlock::Asserted) => CodeRiskStatus::WindowsNotReadBack,
        (Os::MacOs | Os::Linux, WebrtcBlock::ReadBack) => CodeRiskStatus::Unverified,
        _ => CodeRiskStatus::NotChecked,
    }
}

/// The note for this computer, now.
pub fn code_risk_status() -> CodeRiskStatus {
    code_risk_status_for(Os::current(), webrtc_block())
}

/// Whether installing this, or updating it to a new commit, needs the user's explicit
/// acceptance of the code risk: code (`kind: app`) from any marketplace but the built-in one.
pub const fn needs_code_consent(kind: Kind, third_party: bool) -> bool {
    matches!(kind, Kind::App) && third_party
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_BLOCKS: [WebrtcBlock; 4] = [
        WebrtcBlock::Unknown,
        WebrtcBlock::ReadBack,
        WebrtcBlock::Failed,
        WebrtcBlock::Asserted,
    ];

    #[test]
    fn windows_notes() {
        use CodeRiskStatus as S;
        let w = |b| code_risk_status_for(Os::Windows, b);
        assert_eq!(w(WebrtcBlock::Asserted), S::WindowsNotReadBack);
        assert_eq!(w(WebrtcBlock::Failed), S::BlockFailed);
        assert_eq!(w(WebrtcBlock::Unknown), S::NotChecked);
        // Windows has no read-back; claiming one is not trusted as better news.
        assert_eq!(w(WebrtcBlock::ReadBack), S::NotChecked);
    }

    #[test]
    fn macos_and_linux_notes() {
        use CodeRiskStatus as S;
        for os in [Os::MacOs, Os::Linux] {
            let n = |b| code_risk_status_for(os, b);
            assert_eq!(n(WebrtcBlock::ReadBack), S::Unverified, "{os:?}");
            assert_eq!(n(WebrtcBlock::Failed), S::BlockFailed, "{os:?}");
            assert_eq!(n(WebrtcBlock::Unknown), S::NotChecked, "{os:?}");
            // Only Windows asserts without reading back.
            assert_eq!(n(WebrtcBlock::Asserted), S::NotChecked, "{os:?}");
        }
    }

    #[test]
    fn a_failure_is_reported_on_every_platform() {
        for os in [Os::Windows, Os::MacOs, Os::Linux] {
            assert_eq!(
                code_risk_status_for(os, WebrtcBlock::Failed),
                CodeRiskStatus::BlockFailed
            );
        }
    }

    #[test]
    fn every_state_maps_to_a_note_and_none_is_unconditional() {
        // No platform and state combination yields a note that isn't one of the four honest
        // ones; in particular nothing maps to "verified".
        for os in [Os::Windows, Os::MacOs, Os::Linux] {
            for b in ALL_BLOCKS {
                let s = code_risk_status_for(os, b);
                assert!(matches!(
                    s,
                    CodeRiskStatus::WindowsNotReadBack
                        | CodeRiskStatus::Unverified
                        | CodeRiskStatus::BlockFailed
                        | CodeRiskStatus::NotChecked
                ));
            }
        }
    }

    #[test]
    fn notes_serialize_as_camel_case() {
        let json = |s| serde_json::to_value(s).expect("json");
        assert_eq!(
            json(CodeRiskStatus::WindowsNotReadBack),
            "windowsNotReadBack"
        );
        assert_eq!(json(CodeRiskStatus::Unverified), "unverified");
        assert_eq!(json(CodeRiskStatus::BlockFailed), "blockFailed");
        assert_eq!(json(CodeRiskStatus::NotChecked), "notChecked");
    }

    #[test]
    fn consent_is_needed_for_third_party_code_only() {
        assert!(needs_code_consent(Kind::App, true));
        assert!(!needs_code_consent(Kind::App, false), "bundled code");
        assert!(
            !needs_code_consent(Kind::Content, true),
            "packs have no code"
        );
        assert!(!needs_code_consent(Kind::Content, false));
    }

    #[test]
    fn the_recorded_state_round_trips_and_starts_unknown() {
        // Only this test touches the global (tests run in parallel).
        assert_eq!(webrtc_block(), WebrtcBlock::Unknown);
        for b in ALL_BLOCKS {
            set_webrtc_block(b);
            assert_eq!(webrtc_block(), b);
            assert_eq!(code_risk_status(), code_risk_status_for(Os::current(), b));
        }
        set_webrtc_block(WebrtcBlock::Unknown);
    }
}
