//! WebRTC is turned off in the app's webview on every OS (S12-T13, ADR-012).
//!
//! CSP has no directive that governs WebRTC, so without this an activity frame could open a
//! peer connection and send data out. Removing `RTCPeerConnection` with script is not enough: an
//! activity can append an `<iframe>` and take fresh constructors from its blank document. No page
//! in the app needs WebRTC (the Companion and Git run in Rust), so it is turned off in the engine
//! for the whole webview:
//!
//! - **Windows (WebView2):** browser arguments set in `tauri.conf.json` ([`WINDOWS_ARGS`]).
//!   WebView2 has no setting to turn WebRTC off, so the arguments take away its routes out:
//!   WebRTC may use only proxied connections (no UDP, STUN or UDP relay), and the webview's only
//!   proxy is a closed local port, so its TCP connections (ICE-TCP, TURN over TCP) fail too,
//!   loopback included. The app page makes no network requests of its own; its `http://` hosts
//!   are served by the app and skip the proxy. WebRTC asks the proxy about `https://` addresses
//!   only, so those `http://` exceptions never apply to it.
//! - **macOS (WKWebView):** WebKit's `PeerConnectionEnabled` preference is turned off. It hides
//!   `RTCPeerConnection` and `RTCDataChannel` in every frame of the page, nested ones included.
//!   It is WebKit SPI (`-[WKPreferences _setPeerConnectionEnabled:]`, macOS 10.13.4+), so the
//!   selector is checked first and the value read back.
//! - **Linux (WebKitGTK):** `WebKitSettings:enable-webrtc` is set to false (also its default) and
//!   read back. It covers every frame of the page.
//!
//! The sandbox proof (`tests/sandbox/run-denial-tests.mjs`) checks the result from inside the
//! activity, from a nested frame and from outside with local listeners.

use std::sync::atomic::{AtomicU8, Ordering};

/// The WebView2 browser arguments the app must start with, in `tauri.conf.json`
/// (`app.windows[0].additionalBrowserArgs`). Setting them replaces wry's defaults, so its
/// `--disable-features` list is repeated here. WebView2 appends any arguments from the
/// `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` variable or its registry policy after these.
#[cfg_attr(not(test), allow(dead_code))] // Applied through tauri.conf.json; a test keeps them equal.
pub const WINDOWS_ARGS: &[&str] = &[
    // wry's defaults: no mini menu, PDF mini menu or SmartScreen.
    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection",
    // WebRTC may use only proxied connections: no UDP, STUN or UDP relay. The Edge runtime
    // reads this name (it maps to the `webrtc.ip_handling_policy` preference).
    "--webrtc-ip-handling-policy=disable_non_proxied_udp",
    // The same under Chromium's name, which the Edge runtime (156) does not read today.
    "--force-webrtc-ip-handling-policy=disable_non_proxied_udp",
    // The only proxy is a closed local port, so proxied (TCP) connections fail as well.
    "--proxy-server=http://127.0.0.1:9",
    // No implicit loopback bypass (`<-loopback>`), so WebRTC can't reach this machine either.
    // The app's own `http://` hosts, served by the app (and the dev server), skip the proxy.
    "--proxy-bypass-list=<-loopback>;http://tauri.localhost;http://ipc.localhost;\
     http://lullact.localhost;http://localhost:1420",
];

/// Whether WebRTC was confirmed off in the main webview: unknown until checked.
static STATE: AtomicU8 = AtomicU8::new(State::Unknown as u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum State {
    /// Not checked yet.
    Unknown = 0,
    /// Turned off and read back (macOS, Linux), or set at webview creation (Windows).
    Off = 1,
    /// Could not be turned off or confirmed.
    Failed = 2,
}

/// What is known about WebRTC in the main webview.
#[allow(dead_code)] // Read by tests; the gate for third-party code is a constant (ADR-015).
pub fn state() -> State {
    match STATE.load(Ordering::SeqCst) {
        1 => State::Off,
        2 => State::Failed,
        _ => State::Unknown,
    }
}

fn record(ok: bool) {
    let s = if ok { State::Off } else { State::Failed };
    STATE.store(s as u8, Ordering::SeqCst);
    if !ok {
        // A fixed message: nothing about the user or their data.
        eprintln!("lullshift: could not turn WebRTC off in the webview (ADR-012)");
    }
}

/// Turns WebRTC off in `window`'s webview where that is done at run time (macOS, Linux). On
/// Windows it was set when the webview was created ([`WINDOWS_ARGS`]).
pub fn disable(window: &tauri::WebviewWindow) {
    #[cfg(windows)]
    {
        let _ = window;
        record(true);
    }
    #[cfg(not(windows))]
    {
        if window
            .with_webview(|wv| record(platform::disable(&wv)))
            .is_err()
        {
            record(false);
        }
    }
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)] // The only unsafe code in the app: Objective-C messages to WebKit, see below.
mod platform {
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, Bool};
    use objc2::{msg_send, sel};

    /// Turns off WebKit's `PeerConnectionEnabled` preference and reads it back.
    pub fn disable(wv: &tauri::webview::PlatformWebview) -> bool {
        let ptr = wv.inner().cast::<AnyObject>();
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
            let can_get: Bool =
                msg_send![&*prefs, respondsToSelector: sel!(_peerConnectionEnabled)];
            if !can_set.as_bool() || !can_get.as_bool() {
                return false;
            }
            let _: () = msg_send![&*prefs, _setPeerConnectionEnabled: Bool::NO];
            let on: Bool = msg_send![&*prefs, _peerConnectionEnabled];
            !on.as_bool()
        }
    }
}

#[cfg(all(not(windows), not(target_os = "macos")))]
mod platform {
    use webkit2gtk::{SettingsExt, WebViewExt};

    /// Sets `enable-webrtc` to false and reads it back.
    pub fn disable(wv: &tauri::webview::PlatformWebview) -> bool {
        let Some(settings) = wv.inner().settings() else {
            return false;
        };
        settings.set_enable_webrtc(false);
        !settings.enables_webrtc()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_args() -> String {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("tauri.conf.json");
        let windows = conf["app"]["windows"].as_array().expect("windows");
        assert_eq!(windows.len(), 1, "one window; each needs the arguments");
        windows[0]["additionalBrowserArgs"]
            .as_str()
            .expect("additionalBrowserArgs is set")
            .to_owned()
    }

    #[test]
    fn the_window_starts_with_exactly_the_webrtc_arguments() {
        let args = config_args();
        let got: Vec<&str> = args.split_whitespace().collect();
        assert_eq!(
            got, WINDOWS_ARGS,
            "tauri.conf.json and WINDOWS_ARGS must agree"
        );
    }

    #[test]
    fn the_arguments_leave_no_direct_route_and_keep_wry_defaults() {
        let has = |a: &str| WINDOWS_ARGS.contains(&a);
        assert!(has("--webrtc-ip-handling-policy=disable_non_proxied_udp"));
        assert!(has(
            "--force-webrtc-ip-handling-policy=disable_non_proxied_udp"
        ));
        // A fixed proxy on a closed loopback port, with no "direct" fallback.
        let proxy: Vec<&&str> = WINDOWS_ARGS
            .iter()
            .filter(|a| a.starts_with("--proxy"))
            .collect();
        assert_eq!(proxy.len(), 2, "{proxy:?}");
        assert_eq!(proxy[0], &"--proxy-server=http://127.0.0.1:9");
        // The bypass list removes the implicit loopback bypass, and every exception is an
        // `http://` rule for one of the app's own hosts. WebRTC resolves its TCP connections as
        // `https://<host>:<port>`, so no exception may be a bare host, a wildcard or `https://`.
        let rules = proxy[1]
            .strip_prefix("--proxy-bypass-list=")
            .expect("bypass list");
        let rules: Vec<&str> = rules.split(';').collect();
        assert_eq!(rules[0], "<-loopback>");
        assert_eq!(
            &rules[1..],
            [
                "http://tauri.localhost",
                "http://ipc.localhost",
                "http://lullact.localhost",
                "http://localhost:1420",
            ]
        );
        assert!(
            !WINDOWS_ARGS.iter().any(|a| a.contains("direct")
                || a.starts_with("--no-proxy")
                || a.contains("remote-debugging")),
            "no direct fallback, proxy opt-out or debug port"
        );
        // Only one --disable-features: Chromium keeps the last one, which would drop wry's.
        let features: Vec<&&str> = WINDOWS_ARGS
            .iter()
            .filter(|a| a.starts_with("--disable-features=") || a.starts_with("--enable-features="))
            .collect();
        assert_eq!(
            features,
            [&"--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection"]
        );
    }

    #[test]
    fn unsafe_code_is_allowed_in_one_place_only() {
        // The crate denies unsafe code (Cargo.toml); this module's macOS part is the exception.
        let marker = concat!("allow(", "unsafe_code)");
        let mut found = Vec::new();
        let mut dirs = vec![std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(&dir).expect("src dir") {
                let path = entry.expect("entry").path();
                if path.is_dir() {
                    dirs.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let src = std::fs::read_to_string(&path).expect("source");
                    for _ in src.matches(marker) {
                        found.push(
                            path.file_name()
                                .expect("name")
                                .to_string_lossy()
                                .into_owned(),
                        );
                    }
                }
            }
        }
        assert_eq!(found, ["webrtc.rs"]);
    }

    #[test]
    fn state_starts_unknown_and_records_the_outcome() {
        // Other tests don't touch STATE; this one runs alone in its own process slot.
        assert_eq!(state(), State::Unknown);
        record(true);
        assert_eq!(state(), State::Off);
        record(false);
        assert_eq!(state(), State::Failed);
        STATE.store(State::Unknown as u8, Ordering::SeqCst);
    }
}
