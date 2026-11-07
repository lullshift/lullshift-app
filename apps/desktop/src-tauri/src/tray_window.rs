//! The tray window (ADR-026 section 6): a small second webview next to the tray icon.
//!
//! - It is declared in `tauri.conf.json` (label `tray`, hidden), so it gets exactly the same
//!   browser arguments, CSP and WebRTC block as `main` by construction (a test keeps every window's
//!   arguments equal). It loads its own page (`tray.html`), never the main App.
//! - It may only read (dashboard, activity list, wait estimates, interface preferences) and call
//!   the two commands here; `capabilities/tray.json` grants nothing else and `acl_check.rs` pins that.
//! - Activities never run in it. `open_main` shows the main window and tells it what to open; the
//!   main App starts the activity through its normal host and sandbox.
//! - Left click toggles it next to the icon (position from the click's icon rectangle and the
//!   monitor's work area, so it stays fully on screen for taskbars on any edge). It hides when it
//!   loses focus. Linux tray backends often deliver no click events; there the right-click menu
//!   ("Show Lullshift") is the way in.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize};

use crate::{Core, activities, context, show_main};

pub const LABEL: &str = "tray";
const MAIN_LABEL: &str = "main";
/// Emitted to the tray window each time it is shown: refresh and focus the first control.
const SHOWN: &str = "tray-shown";
/// Emitted to the main window by `open_main`.
const OPEN_MAIN: &str = "open-main";
/// A click on the icon within this long after the window hid itself on blur is the click that
/// took the focus away: it closes the window, it must not reopen it.
const REOPEN_GUARD: Duration = Duration::from_millis(400);
/// Gap between the icon and the window, in physical pixels.
const GAP: f64 = 8.0;

static HID_ON_BLUR: Mutex<Option<Instant>> = Mutex::new(None);

// ---- placement ---------------------------------------------------------------------------

/// A rectangle in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    fn right(&self) -> f64 {
        self.x + self.w
    }
    fn bottom(&self) -> f64 {
        self.y + self.h
    }
    fn cx(&self) -> f64 {
        self.x + self.w / 2.0
    }
    fn cy(&self) -> f64 {
        self.y + self.h / 2.0
    }
}

/// Top-left corner for a `win`-sized window next to `icon`, kept fully inside `area` (the
/// monitor's work area, which excludes the taskbar and menu bar). The icon's side of the work
/// area tells where the bar is: below it, above it, or to a side. An icon inside the work area
/// (an overflow flyout, or a backend that reports only a point) opens below or above it,
/// whichever half of the area it is in.
pub fn place(icon: Rect, win: (f64, f64), area: Rect) -> (f64, f64) {
    let (w, h) = win;
    let (x, y) = if icon.cy() >= area.bottom() {
        (icon.cx() - w / 2.0, icon.y - h - GAP)
    } else if icon.cy() < area.y {
        (icon.cx() - w / 2.0, icon.bottom() + GAP)
    } else if icon.cx() < area.x {
        (icon.right() + GAP, icon.cy() - h / 2.0)
    } else if icon.cx() >= area.right() {
        (icon.x - w - GAP, icon.cy() - h / 2.0)
    } else if icon.cy() > area.cy() {
        (icon.cx() - w / 2.0, icon.y - h - GAP)
    } else {
        (icon.cx() - w / 2.0, icon.bottom() + GAP)
    };
    // Keep it fully on screen. A window larger than the area sticks to the top-left.
    let max_x = (area.right() - w).max(area.x);
    let max_y = (area.bottom() - h).max(area.y);
    (x.clamp(area.x, max_x), y.clamp(area.y, max_y))
}

// ---- the click ---------------------------------------------------------------------------

/// Left click on the tray icon: the quick window or the main window, by the saved setting.
pub fn on_left_click(app: &AppHandle, position: PhysicalPosition<f64>, icon: tauri::Rect) {
    let quick = app
        .try_state::<Arc<Core>>()
        .map(|core| context::read_ui_prefs(&core).tray_click == context::TrayClick::Quick)
        .unwrap_or(false);
    if !quick {
        show_main(app);
        return;
    }
    if !toggle(app, position, icon) {
        // No tray window (it failed to build): never leave the click doing nothing.
        show_main(app);
    }
}

/// Shows or hides the tray window. False if there is no tray window.
fn toggle(app: &AppHandle, position: PhysicalPosition<f64>, icon: tauri::Rect) -> bool {
    let Some(win) = app.get_webview_window(LABEL) else {
        return false;
    };
    if win.is_visible().unwrap_or(false) {
        let _ = win.hide();
        return true;
    }
    let blurred_just_now = HID_ON_BLUR
        .lock()
        .ok()
        .and_then(|t| *t)
        .is_some_and(|t| t.elapsed() < REOPEN_GUARD);
    if blurred_just_now {
        return true;
    }
    let monitor = app
        .monitor_from_point(position.x, position.y)
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten());
    if let Some(m) = monitor {
        let scale = m.scale_factor();
        let size = win
            .outer_size()
            .unwrap_or_else(|_| PhysicalSize::new((380.0 * scale) as u32, (560.0 * scale) as u32));
        let (ip, is) = (
            icon.position.to_physical::<f64>(scale),
            icon.size.to_physical::<f64>(scale),
        );
        // A backend that reports no rectangle gives a point: the click.
        let icon_rect = if is.width > 0.0 && is.height > 0.0 {
            Rect {
                x: ip.x,
                y: ip.y,
                w: is.width,
                h: is.height,
            }
        } else {
            Rect {
                x: position.x,
                y: position.y,
                w: 0.0,
                h: 0.0,
            }
        };
        let wa = m.work_area();
        let area = Rect {
            x: f64::from(wa.position.x),
            y: f64::from(wa.position.y),
            w: f64::from(wa.size.width),
            h: f64::from(wa.size.height),
        };
        let (x, y) = place(
            icon_rect,
            (f64::from(size.width), f64::from(size.height)),
            area,
        );
        let _ = win.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
    }
    let _ = win.show();
    let _ = win.set_focus();
    let _ = app.emit_to(LABEL, SHOWN, ());
    true
}

/// The tray window lost focus: hide it, and remember when so the click that caused it doesn't
/// reopen it.
pub fn on_blur(window: &tauri::Window) {
    if window.label() != LABEL || !window.is_visible().unwrap_or(false) {
        return;
    }
    if let Ok(mut t) = HID_ON_BLUR.lock() {
        *t = Some(Instant::now());
    }
    let _ = window.hide();
}

// ---- commands ----------------------------------------------------------------------------

/// The views the tray may ask the main window to open. A fixed list: anything else is refused
/// when the arguments are read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MainView {
    Home,
    Sessions,
    Play,
    Rest,
    Continue,
    Companion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionRef {
    pub provider: String,
    pub session_id: String,
}

/// What the main window is told to do. Built only from validated input.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct OpenMain {
    view: MainView,
    #[serde(skip_serializing_if = "Option::is_none")]
    activity_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_key: Option<SessionRef>,
}

/// Refuses anything that is not a plain view, an enabled installed verified activity in a library
/// view, or an existing session in Sessions.
fn validate(
    core: &Core,
    view: MainView,
    activity_id: Option<&str>,
    session: Option<&SessionRef>,
) -> Result<(), String> {
    if activity_id.is_some() && session.is_some() {
        return Err("invalid_request".into());
    }
    if let Some(id) = activity_id {
        if !matches!(view, MainView::Play | MainView::Rest | MainView::Continue) {
            return Err("invalid_request".into());
        }
        // The same list the library shows: enabled, installed and verified.
        let listed = activities::list_activities(core)?;
        if !listed.iter().any(|a| a.id == id) {
            return Err("unknown_activity".into());
        }
    }
    if let Some(s) = session {
        if view != MainView::Sessions {
            return Err("invalid_request".into());
        }
        if !context::session_exists(core, &s.provider, &s.session_id)? {
            return Err("not_found".into());
        }
    }
    Ok(())
}

/// Only the tray window may call the tray commands. (The ACL already grants them to no other
/// window; this keeps that true if a capability is ever widened by mistake.)
fn from_tray(window: &tauri::WebviewWindow) -> Result<(), String> {
    if window.label() == LABEL {
        Ok(())
    } else {
        Err("forbidden".into())
    }
}

/// Shows and focuses the main window, hides the tray window, and tells the main App to open
/// `view` (optionally starting `activity_id`, or opening `session_key`'s panel).
#[tauri::command]
pub async fn open_main(
    window: tauri::WebviewWindow,
    app: AppHandle,
    core: tauri::State<'_, Arc<Core>>,
    view: MainView,
    activity_id: Option<String>,
    session_key: Option<SessionRef>,
) -> Result<(), String> {
    from_tray(&window)?;
    let core = Arc::clone(&core);
    let (aid, key) = (activity_id.clone(), session_key.clone());
    tauri::async_runtime::spawn_blocking(move || {
        validate(&core, view, aid.as_deref(), key.as_ref())
    })
    .await
    .map_err(|_| "internal".to_owned())??;
    let _ = window.hide();
    show_main(&app);
    app.emit_to(
        MAIN_LABEL,
        OPEN_MAIN,
        OpenMain {
            view,
            activity_id,
            session_key,
        },
    )
    .map_err(|_| "internal".to_owned())
}

/// Hides the tray window (Esc).
#[tauri::command]
pub fn tray_hide(window: tauri::WebviewWindow) -> Result<(), String> {
    from_tray(&window)?;
    let _ = window.hide();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: Rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 1920.0,
        h: 1040.0,
    };
    const WIN: (f64, f64) = (380.0, 560.0);

    fn icon(x: f64, y: f64) -> Rect {
        Rect {
            x,
            y,
            w: 24.0,
            h: 24.0,
        }
    }

    fn on_screen(p: (f64, f64), area: Rect) {
        assert!(p.0 >= area.x && p.0 + WIN.0 <= area.right(), "{p:?}");
        assert!(p.1 >= area.y && p.1 + WIN.1 <= area.bottom(), "{p:?}");
    }

    #[test]
    fn a_bottom_taskbar_opens_above_the_icon_centred_and_kept_on_screen() {
        // Icon in the taskbar below the work area, near the right edge.
        let p = place(icon(1800.0, 1050.0), WIN, AREA);
        assert!(p.1 + WIN.1 <= 1050.0 - GAP + 0.1, "above the icon: {p:?}");
        assert_eq!(p.0, 1920.0 - WIN.0, "clamped to the right edge");
        on_screen(p, AREA);
        // Mid-screen icon: centred on it.
        let p = place(icon(900.0, 1050.0), WIN, AREA);
        assert_eq!(p.0, 912.0 - WIN.0 / 2.0);
    }

    #[test]
    fn a_top_menu_bar_opens_below_the_icon() {
        let area = Rect {
            y: 25.0,
            h: 1055.0,
            ..AREA
        };
        let p = place(icon(1500.0, 0.0), WIN, area);
        assert_eq!(p.1, 24.0 + GAP);
        on_screen(p, area);
    }

    #[test]
    fn left_and_right_taskbars_open_beside_the_icon() {
        let left = Rect {
            x: 48.0,
            w: 1872.0,
            ..AREA
        };
        let p = place(icon(10.0, 900.0), WIN, left);
        assert_eq!(p.0, 48.0, "beside the icon, then kept out of the bar");
        on_screen(p, left);
        let right = Rect { w: 1872.0, ..AREA };
        let p = place(icon(1890.0, 100.0), WIN, right);
        assert_eq!(p.0, 1890.0 - WIN.0 - GAP);
        on_screen(p, right);
    }

    #[test]
    fn an_icon_inside_the_work_area_or_a_bare_point_still_fits() {
        // Overflow flyout in the lower half: above; in the upper half: below.
        let low = place(icon(1000.0, 900.0), WIN, AREA);
        assert!(low.1 + WIN.1 <= 900.0);
        let high = place(icon(1000.0, 100.0), WIN, AREA);
        assert!(high.1 >= 124.0);
        // A backend that reports only the click point (zero size) in a corner.
        let p = place(
            Rect {
                x: 1919.0,
                y: 1079.0,
                w: 0.0,
                h: 0.0,
            },
            WIN,
            AREA,
        );
        on_screen(p, AREA);
        // A second monitor to the left with a negative origin.
        let left_screen = Rect {
            x: -1920.0,
            y: 0.0,
            w: 1920.0,
            h: 1040.0,
        };
        on_screen(place(icon(-100.0, 1050.0), WIN, left_screen), left_screen);
    }

    #[test]
    fn a_window_larger_than_the_area_sticks_to_its_corner() {
        let small = Rect {
            x: 10.0,
            y: 20.0,
            w: 300.0,
            h: 400.0,
        };
        assert_eq!(place(icon(100.0, 420.0), WIN, small), (10.0, 20.0));
    }

    #[test]
    fn the_view_is_a_fixed_list() {
        let ok = |v: &str| serde_json::from_str::<MainView>(&format!("\"{v}\"")).is_ok();
        for v in ["home", "sessions", "play", "rest", "continue", "companion"] {
            assert!(ok(v), "{v}");
        }
        for v in ["settings", "marketplace", "Home", "", "../x"] {
            assert!(!ok(v), "{v}");
        }
        assert!(
            serde_json::from_str::<SessionRef>(r#"{"provider":"p","sessionId":"s","x":1}"#)
                .is_err()
        );
    }

    #[test]
    fn open_main_refuses_unknown_activities_sessions_and_odd_combinations() {
        let dir = tempfile::tempdir().expect("dir");
        let core = Core::for_tests(dir.path());
        let id = activities::list_ids(&core)
            .expect("list")
            .into_iter()
            .next()
            .expect("a bundled activity");
        let sref = |s: &str| SessionRef {
            provider: "claude-code".into(),
            session_id: s.into(),
        };

        assert!(validate(&core, MainView::Home, None, None).is_ok());
        assert!(validate(&core, MainView::Play, Some(&id), None).is_ok());
        assert!(validate(&core, MainView::Continue, Some(&id), None).is_ok());
        for bad in ["nope", "", "../x", "game-that-is-not-installed"] {
            assert_eq!(
                validate(&core, MainView::Play, Some(bad), None),
                Err("unknown_activity".into()),
                "{bad}"
            );
        }
        // An activity belongs with a library view; never with Sessions or both at once.
        assert_eq!(
            validate(&core, MainView::Sessions, Some(&id), None),
            Err("invalid_request".into())
        );
        assert_eq!(
            validate(&core, MainView::Sessions, Some(&id), Some(&sref("s"))),
            Err("invalid_request".into())
        );

        // Sessions: only one that exists.
        assert_eq!(
            validate(&core, MainView::Sessions, None, Some(&sref("s"))),
            Err("not_found".into())
        );
        let event = agent_core::parse_event(
            r#"{"schemaVersion":1,"eventId":"00000000-0000-4000-8000-000000000001","provider":"claude-code","providerSessionId":"s","eventType":"working","timestampUtc":"2026-11-07T10:00:00Z"}"#,
        )
        .expect("event");
        core.store
            .lock()
            .expect("lock")
            .as_mut()
            .expect("store")
            .ingest(&event, time::OffsetDateTime::now_utc())
            .expect("ingest");
        assert!(validate(&core, MainView::Sessions, None, Some(&sref("s"))).is_ok());
        assert_eq!(
            validate(&core, MainView::Play, None, Some(&sref("s"))),
            Err("invalid_request".into())
        );
    }
}
