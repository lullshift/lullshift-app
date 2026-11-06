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
//!   selector is checked first and the value read back. The Objective-C messages are in the
//!   `macos-webrtc` crate, the only place in the workspace that allows unsafe code.
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

/// What is known about WebRTC in the main webview: unknown until checked.
static STATE: AtomicU8 = AtomicU8::new(State::Unknown as u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum State {
    /// Not checked yet.
    Unknown = 0,
    /// Turned off and read back (macOS, Linux). The only state that opens the third-party code
    /// gate (`marketplace::third_party_code_allowed`).
    Off = 1,
    /// Could not be turned off or confirmed.
    Failed = 2,
    /// Set when the webview was created but not read back (Windows: browser arguments that
    /// WebView2 gives no way to read). The sandbox proof checks the result from outside.
    /// Not `Off`: nothing is confirmed.
    Asserted = 3,
}

/// What is known about WebRTC in the main webview.
#[allow(dead_code)] // Read by tests; the third-party code gate is told through `record`.
pub fn state() -> State {
    match STATE.load(Ordering::SeqCst) {
        1 => State::Off,
        2 => State::Failed,
        3 => State::Asserted,
        _ => State::Unknown,
    }
}

/// Stores `state` and tells the third-party code gate whether WebRTC is confirmed off: only
/// `Off` counts.
fn set_state(state: State) {
    STATE.store(state as u8, Ordering::SeqCst);
    marketplace::set_webrtc_confirmed_off(state == State::Off);
}

fn record(ok: bool) {
    set_state(if ok { State::Off } else { State::Failed });
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
        // Not read back, so not `Off`: the gate stays closed (see `State::Asserted`).
        let _ = window;
        set_state(State::Asserted);
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
mod platform {
    /// Turns off WebKit's `PeerConnectionEnabled` preference and reads it back (in the
    /// `macos-webrtc` crate, which holds the only unsafe code).
    pub fn disable(wv: &tauri::webview::PlatformWebview) -> bool {
        // Tauri documents `inner()` as the `WKWebView` of a live webview, and this runs on the
        // main thread inside `with_webview`, which is what the function requires.
        macos_webrtc::disable_peer_connection(wv.inner())
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

    /// Unsafe code is forbidden for this crate by inheriting the workspace lints, and only
    /// `macos-webrtc` may opt out. `forbid` cannot be overridden by an attribute, so what has
    /// to be checked is the manifests: this crate and every other workspace crate inherit the
    /// workspace lints, the workspace forbids unsafe code, and no manifest lowers the lint.
    #[test]
    fn only_the_macos_crate_may_allow_unsafe_code() {
        let desktop = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let root = desktop.join("../../..");
        let root_toml = std::fs::read_to_string(root.join("Cargo.toml")).expect("root manifest");
        assert!(
            root_toml.contains("[workspace.lints.rust]\nunsafe_code = \"forbid\"")
                || root_toml.contains("[workspace.lints.rust]\r\nunsafe_code = \"forbid\""),
            "the workspace must forbid unsafe code"
        );
        let mut manifests = vec![desktop.join("Cargo.toml")];
        for entry in std::fs::read_dir(root.join("crates")).expect("crates dir") {
            let path = entry.expect("entry").path();
            if path.join("Cargo.toml").is_file() {
                manifests.push(path.join("Cargo.toml"));
            }
        }
        let mut exceptions = Vec::new();
        for path in manifests {
            let text = std::fs::read_to_string(&path).expect("manifest");
            // A table header followed by the key, with any spacing.
            let lines: Vec<&str> = text.lines().map(str::trim).collect();
            let inherits = lines
                .windows(2)
                .any(|w| w[0] == "[lints]" && w[1].replace(' ', "") == "workspace=true");
            let lowers = lines.iter().any(|l| {
                let l = l.replace(' ', "");
                l.starts_with("unsafe_code=") && !l.starts_with("unsafe_code=\"forbid\"")
            });
            if !inherits || lowers {
                let name = path
                    .parent()
                    .and_then(|p| p.file_name())
                    .expect("dir")
                    .to_string_lossy()
                    .into_owned();
                exceptions.push(name);
            }
        }
        exceptions.sort();
        assert_eq!(
            exceptions,
            ["macos-webrtc"],
            "only macos-webrtc may differ from the workspace lints"
        );
        // And that crate is the only place with an unsafe block or an allow attribute.
        let mut found = Vec::new();
        let mut dirs = vec![desktop.join("src"), desktop.join("build.rs")];
        for entry in std::fs::read_dir(root.join("crates")).expect("crates dir") {
            let path = entry.expect("entry").path();
            if path.file_name().is_some_and(|n| n != "macos-webrtc") {
                dirs.push(path.join("src"));
                dirs.push(path.join("tests"));
                dirs.push(path.join("build.rs"));
            }
        }
        dirs.push(desktop.join("tests"));
        while let Some(path) = dirs.pop() {
            if path.is_dir() {
                for entry in std::fs::read_dir(&path).expect("dir") {
                    dirs.push(entry.expect("entry").path());
                }
            } else if path.is_file() && path.extension().is_some_and(|e| e == "rs") {
                let src = std::fs::read_to_string(&path).expect("source");
                // Any mention of the lint name inside an attribute, or an `unsafe {` / `unsafe fn`
                // / `unsafe impl`, outside a comment.
                for line in src.lines() {
                    let code = line.split("//").next().unwrap_or("");
                    let attr =
                        code.trim_start().starts_with("#[") || code.trim_start().starts_with("#![");
                    // The keyword as a whole word (`ps_unsafe {` is a name, not the keyword).
                    let word = ["unsafe {", "unsafe fn", "unsafe impl", "unsafe extern"]
                        .iter()
                        .any(|w| {
                            code.match_indices(w).any(|(i, _)| {
                                code[..i]
                                    .chars()
                                    .next_back()
                                    .is_none_or(|c| !(c.is_alphanumeric() || c == '_'))
                            })
                        });
                    if (attr && code.contains(concat!("unsafe", "_code"))) || word {
                        found.push(path.display().to_string());
                    }
                }
            }
        }
        // This test's own source names the patterns above in string literals.
        found.retain(|f| !f.ends_with("webrtc.rs"));
        assert!(
            found.is_empty(),
            "unsafe code outside macos-webrtc: {found:?}"
        );
    }

    #[test]
    fn state_starts_unknown_and_records_the_outcome() {
        // No other test touches STATE (tests run on parallel threads, so keep it that way).
        assert_eq!(state(), State::Unknown);
        record(true);
        assert_eq!(state(), State::Off);
        record(false);
        assert_eq!(state(), State::Failed);
        // Windows: set at creation, not read back. Not `Off`, so the gate gets no confirmation.
        set_state(State::Asserted);
        assert_eq!(state(), State::Asserted);
        assert_ne!(state(), State::Off);
        assert!(!marketplace::third_party_code_allowed());
        set_state(State::Unknown);
    }
}
