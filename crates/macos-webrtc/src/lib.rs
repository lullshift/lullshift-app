//! Turns off WebRTC in a macOS `WKWebView` (ADR-012, S12-T13).
//!
//! This is one of the two crates in the workspace that allow `unsafe` (the other is
//! `windows-package`, decision 33); `lullshift-desktop` forbids it
//! and calls [`disable_peer_connection`] instead. The crate is empty on other operating systems.
//!
//! The function body was checked against `objc2` 0.6.5 (`msg_send!`, `Retained`, `Bool`,
//! `sel!`) in Sprint 12 and moved here unchanged apart from taking the raw view pointer instead
//! of a Tauri type. It has not been run on macOS: that is a manual check (see
//! `docs/MANUAL_TESTS.md`).

#[cfg(target_os = "macos")]
use objc2::rc::Retained;
#[cfg(target_os = "macos")]
use objc2::runtime::{AnyObject, Bool};
#[cfg(target_os = "macos")]
use objc2::{msg_send, sel};

/// Turns off WebKit's `PeerConnectionEnabled` preference and reads it back. True only if both
/// WebKit selectors exist and the value read back is off.
///
/// `view` must be null or point to a live `WKWebView` that stays alive for the call; Tauri's
/// `PlatformWebview::inner()` is that, and the call must be made on the main thread (inside
/// `with_webview`). A null pointer returns false. This is the only precondition the caller must
/// uphold; the function is safe to call so that the app crate needs no `unsafe`, which is
/// acceptable only because this crate is private to the app (`publish = false`).
#[cfg(target_os = "macos")]
pub fn disable_peer_connection(view: *mut std::ffi::c_void) -> bool {
    let ptr = view.cast::<AnyObject>();
    if ptr.is_null() {
        return false;
    }
    // SAFETY: Tauri documents `inner()` as the `WKWebView` of a live webview; this runs on
    // the main thread inside `with_webview`, while Tauri holds that view. `configuration`
    // and `preferences` are public WebKit properties that return objects (autoreleased,
    // retained here). `_setPeerConnectionEnabled:` and `_peerConnectionEnabled` are WebKit
    // SPI (WKPreferencesPrivate.h, macOS 10.13.4+) taking and returning a BOOL; they are sent
    // only after `respondsToSelector:` confirms both exist.
    unsafe {
        let view: &AnyObject = &*ptr;
        let config: Option<Retained<AnyObject>> = msg_send![view, configuration];
        let Some(config) = config else { return false };
        let prefs: Option<Retained<AnyObject>> = msg_send![&*config, preferences];
        let Some(prefs) = prefs else { return false };
        let can_set: Bool =
            msg_send![&*prefs, respondsToSelector: sel!(_setPeerConnectionEnabled:)];
        let can_get: Bool = msg_send![&*prefs, respondsToSelector: sel!(_peerConnectionEnabled)];
        if !can_set.as_bool() || !can_get.as_bool() {
            return false;
        }
        let _: () = msg_send![&*prefs, _setPeerConnectionEnabled: Bool::NO];
        let on: Bool = msg_send![&*prefs, _peerConnectionEnabled];
        !on.as_bool()
    }
}
