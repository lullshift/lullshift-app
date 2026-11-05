//! Activity host, native side (S03-T02, S03-T03, ADR-012).
//!
//! Bundled activities are compiled into the binary and served from their own URL scheme,
//! `lullact`, which the app CSP allows only as a frame source (S03-T06). The UI loads each one
//! in a sandboxed iframe (`sandbox="allow-scripts"`, so its origin is opaque) and talks to it
//! over a nonce-bound `postMessage` bridge. Activities never call these commands themselves:
//! the trusted UI calls them on an activity's behalf, after validating its message, and names the
//! activity from its own state, never from the message.
//!
//! Every file is served with a strict CSP: own scripts and styles only, no network, no frames,
//! no workers, no forms, and only the app may frame it.

use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

use include_dir::{Dir, include_dir};
use serde::{Deserialize, Serialize};
use tauri::http::{Response, StatusCode, header};
use time::OffsetDateTime;

use crate::Core;

/// The URL scheme activities are served from.
pub const SCHEME: &str = "lullact";

/// Each activity's key-value namespace, in bytes (keys plus values).
pub const STORAGE_QUOTA: usize = 256 * 1024;
/// One stored value.
pub const VALUE_MAX: usize = 64 * 1024;
/// One saved state.
pub const SNAPSHOT_MAX: usize = 256 * 1024;
const KEY_MAX: usize = 64;

static BUNDLED: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/../../../activities");
/// Debug builds also carry the sandbox probe used by the denial tests (S03-T05).
#[cfg(debug_assertions)]
static PROBE: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/sandbox-probe");

/// The subset of `reference/ACTIVITY_MANIFEST_V1.schema.json` the host relies on. Unknown fields
/// are accepted here (the schema is checked in `pnpm check`), but every field used is checked.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    schema_version: u32,
    kind: String,
    id: String,
    version: String,
    name: String,
    #[serde(default)]
    description: String,
    category: String,
    permissions: Vec<String>,
    entrypoint: Option<String>,
    runtime: Option<String>,
    interruptibility: Option<Interruptibility>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Interruptibility {
    pause: bool,
    resume: bool,
    persist_state: bool,
}

#[derive(Clone)]
struct Activity {
    manifest: Manifest,
    dir: &'static Dir<'static>,
    /// Shown in the Activities list. The sandbox probe is listed too, but exists only in
    /// debug builds.
    listed: bool,
}

/// What the UI shows for one activity.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityInfo {
    id: String,
    name: String,
    description: String,
    category: String,
    version: String,
    url: String,
    storage: bool,
    saved_utc: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    snapshot: String,
    saved_utc: String,
}

fn catalog() -> &'static BTreeMap<String, Activity> {
    static CATALOG: OnceLock<BTreeMap<String, Activity>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let mut map = BTreeMap::new();
        for (dir, listed) in roots() {
            collect(dir, listed, 0, &mut map);
        }
        map
    })
}

fn roots() -> Vec<(&'static Dir<'static>, bool)> {
    #[cfg(debug_assertions)]
    return vec![(&BUNDLED, true), (&PROBE, true)];
    #[cfg(not(debug_assertions))]
    return vec![(&BUNDLED, true)];
}

/// Finds activity folders (those with a valid `manifest.json`) up to two levels down.
fn collect(
    dir: &'static Dir<'static>,
    listed: bool,
    depth: u8,
    out: &mut BTreeMap<String, Activity>,
) {
    if let Some(file) = dir.get_file(dir.path().join("manifest.json")) {
        if let Some(manifest) = file.contents_utf8().and_then(parse_manifest) {
            // A duplicate ID is a packaging bug; keep the first and never serve both.
            out.entry(manifest.id.clone()).or_insert(Activity {
                manifest,
                dir,
                listed,
            });
        }
        return;
    }
    if depth < 2 {
        for sub in dir.dirs() {
            collect(sub, listed, depth + 1, out);
        }
    }
}

/// A manifest the host can run: a sandboxed web app that supports host-owned interruption.
fn parse_manifest(json: &str) -> Option<Manifest> {
    let m: Manifest = serde_json::from_str(json).ok()?;
    let interruptible = m
        .interruptibility
        .as_ref()
        .is_some_and(|i| i.pause && i.resume && i.persist_state);
    let ok = m.schema_version == 1
        && m.kind == "app"
        && valid_id(&m.id)
        && m.entrypoint.as_deref() == Some("index.html")
        && m.runtime.as_deref() == Some("isolated-web-v1")
        && interruptible
        && !m.name.is_empty()
        && m.name.len() <= 100
        && m.description.len() <= 500
        && ["arcade", "break", "productivity", "interactive"].contains(&m.category.as_str())
        && m.permissions.iter().all(|p| p == "activity.storage");
    ok.then_some(m)
}

/// Manifest ID pattern `^[a-z0-9]+(\.[a-z0-9-]+)+$`, at most 160 characters.
fn valid_id(id: &str) -> bool {
    let mut parts = id.split('.');
    let first_ok = parts.next().is_some_and(|p| {
        !p.is_empty()
            && p.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
    });
    let rest: Vec<&str> = parts.collect();
    id.len() <= 160
        && first_ok
        && !rest.is_empty()
        && rest.iter().all(|p| {
            !p.is_empty()
                && p.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}

fn activity(id: &str) -> Result<&'static Activity, String> {
    catalog()
        .get(id)
        .ok_or_else(|| "unknown_activity".to_owned())
}

/// Address of an activity's entry page as the webview sees it.
fn entry_url(id: &str) -> String {
    #[cfg(any(windows, target_os = "android"))]
    return format!("http://{SCHEME}.localhost/{id}/index.html");
    #[cfg(not(any(windows, target_os = "android")))]
    return format!("{SCHEME}://localhost/{id}/index.html");
}

/// Origins allowed to frame an activity: the app itself (and the dev server in debug builds).
fn frame_ancestors() -> &'static str {
    #[cfg(all(windows, debug_assertions))]
    return "http://tauri.localhost http://localhost:1420";
    #[cfg(all(windows, not(debug_assertions)))]
    return "http://tauri.localhost";
    #[cfg(all(not(windows), debug_assertions))]
    return "tauri://localhost http://localhost:1420";
    #[cfg(all(not(windows), not(debug_assertions)))]
    return "tauri://localhost";
}

/// CSP sent with every activity response. `sandbox allow-scripts` gives the page an opaque
/// origin even if it is ever loaded without the host iframe's own `sandbox` attribute.
pub fn activity_csp() -> String {
    format!(
        "sandbox allow-scripts; default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:; \
         font-src 'self'; media-src 'self'; connect-src 'none'; frame-src 'none'; \
         child-src 'none'; worker-src 'none'; object-src 'none'; manifest-src 'none'; \
         base-uri 'none'; form-action 'none'; frame-ancestors {}",
        frame_ancestors()
    )
}

/// Content type for an allowed file extension; anything else is not served.
fn mime(path: &str) -> Option<&'static str> {
    let ext = path.rsplit_once('.')?.1.to_ascii_lowercase();
    Some(match ext.as_str() {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "woff2" => "font/woff2",
        "ogg" => "audio/ogg",
        "mp3" => "audio/mpeg",
        _ => return None,
    })
}

/// Splits `/<activity-id>/<file path>` and checks every segment: no `..`, no hidden files, no
/// encoded characters, no backslashes, at most 8 levels.
fn split_path(path: &str) -> Option<(&str, &str)> {
    let rest = path.strip_prefix('/')?;
    let (id, file) = rest.split_once('/')?;
    let segments: Vec<&str> = file.split('/').collect();
    let ok = valid_id(id)
        && !file.is_empty()
        && segments.len() <= 8
        && segments.iter().all(|s| {
            s.bytes().next().is_some_and(|b| b.is_ascii_alphanumeric())
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
                && !s.contains("..")
        });
    ok.then_some((id, file))
}

/// The bytes and content type for a request path, if it names an allowed bundled file.
fn lookup(path: &str) -> Option<(&'static [u8], &'static str)> {
    let (id, file) = split_path(path)?;
    let activity = catalog().get(id)?;
    let content_type = mime(file)?;
    let found = activity.dir.get_file(activity.dir.path().join(file))?;
    Some((found.contents(), content_type))
}

/// Serves one request on the `lullact` scheme. Only GET; everything carries the activity CSP.
pub fn serve(request: &tauri::http::Request<Vec<u8>>) -> Response<Vec<u8>> {
    let found = (request.method() == tauri::http::Method::GET)
        .then(|| lookup(request.uri().path()))
        .flatten();
    let (status, body, content_type) = match found {
        Some((bytes, ct)) => (StatusCode::OK, bytes.to_vec(), ct),
        None => (
            StatusCode::NOT_FOUND,
            b"Not found".to_vec(),
            "text/plain; charset=utf-8",
        ),
    };
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_SECURITY_POLICY, activity_csp())
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .header(header::REFERRER_POLICY, "no-referrer")
        .header(header::CACHE_CONTROL, "no-store")
        .header(
            "Permissions-Policy",
            "camera=(), microphone=(), geolocation=(), usb=(), serial=(), hid=(), \
             bluetooth=(), payment=(), clipboard-read=(), clipboard-write=(), \
             display-capture=(), screen-wake-lock=(), publickey-credentials-get=()",
        )
        .header("Cross-Origin-Opener-Policy", "same-origin")
        .body(body)
        .unwrap_or_else(|_| Response::new(Vec::new()))
}

fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= KEY_MAX
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// An activity that declared `activity.storage`, and a valid key.
fn storage_target(id: &str, key: &str) -> Result<(), String> {
    let a = activity(id)?;
    if !a
        .manifest
        .permissions
        .iter()
        .any(|p| p == "activity.storage")
    {
        return Err("not_permitted".into());
    }
    if !valid_key(key) {
        return Err("invalid_key".into());
    }
    Ok(())
}

fn with_store<T>(
    core: &Core,
    f: impl FnOnce(&mut local_store::Store) -> Result<T, local_store::StoreError>,
) -> Result<T, String> {
    let mut guard = core.store.lock().map_err(|_| "internal")?;
    let store = guard.as_mut().ok_or("store_unavailable")?;
    f(store).map_err(|e| match e {
        local_store::StoreError::QuotaExceeded { .. } => "quota_exceeded".to_owned(),
        _ => "store_unavailable".to_owned(),
    })
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|_| "internal".to_owned())?
}

#[tauri::command]
pub async fn activities_list(
    core: tauri::State<'_, Arc<Core>>,
) -> Result<Vec<ActivityInfo>, String> {
    let core = Arc::clone(&core);
    blocking(move || {
        let saved: BTreeMap<String, String> = with_store(&core, |s| s.snapshots())
            .unwrap_or_default()
            .into_iter()
            .collect();
        Ok(catalog()
            .values()
            .filter(|a| a.listed)
            .map(|a| info(a, saved.get(&a.manifest.id).cloned()))
            .collect())
    })
    .await
}

/// One activity, for the host to launch.
#[tauri::command]
pub async fn activity_info(
    core: tauri::State<'_, Arc<Core>>,
    id: String,
) -> Result<ActivityInfo, String> {
    let core = Arc::clone(&core);
    blocking(move || {
        let a = activity(&id)?;
        let saved = with_store(&core, |s| s.snapshot(&id))
            .ok()
            .flatten()
            .map(|(_, t)| t);
        Ok(info(a, saved))
    })
    .await
}

fn info(a: &Activity, saved_utc: Option<String>) -> ActivityInfo {
    let m = &a.manifest;
    ActivityInfo {
        id: m.id.clone(),
        name: m.name.clone(),
        description: m.description.clone(),
        category: m.category.clone(),
        version: m.version.clone(),
        url: entry_url(&m.id),
        storage: m.permissions.iter().any(|p| p == "activity.storage"),
        saved_utc,
    }
}

#[tauri::command]
pub async fn activity_storage_get(
    core: tauri::State<'_, Arc<Core>>,
    id: String,
    key: String,
) -> Result<Option<String>, String> {
    storage_target(&id, &key)?;
    let core = Arc::clone(&core);
    blocking(move || with_store(&core, |s| s.activity_value(&id, &key))).await
}

#[tauri::command]
pub async fn activity_storage_set(
    core: tauri::State<'_, Arc<Core>>,
    id: String,
    key: String,
    value: String,
) -> Result<(), String> {
    storage_target(&id, &key)?;
    if value.len() > VALUE_MAX {
        return Err("value_too_large".into());
    }
    let core = Arc::clone(&core);
    blocking(move || {
        with_store(&core, |s| {
            s.set_activity_value(&id, &key, &value, STORAGE_QUOTA)
        })
    })
    .await
}

#[tauri::command]
pub async fn activity_storage_remove(
    core: tauri::State<'_, Arc<Core>>,
    id: String,
    key: String,
) -> Result<(), String> {
    storage_target(&id, &key)?;
    let core = Arc::clone(&core);
    blocking(move || with_store(&core, |s| s.remove_activity_value(&id, &key))).await
}

#[tauri::command]
pub async fn activity_snapshot(
    core: tauri::State<'_, Arc<Core>>,
    id: String,
) -> Result<Option<Snapshot>, String> {
    activity(&id)?;
    let core = Arc::clone(&core);
    blocking(move || {
        Ok(
            with_store(&core, |s| s.snapshot(&id))?.map(|(snapshot, saved_utc)| Snapshot {
                snapshot,
                saved_utc,
            }),
        )
    })
    .await
}

/// Saves an activity's state. Must be JSON and at most [`SNAPSHOT_MAX`] bytes; otherwise the
/// previous snapshot is kept.
#[tauri::command]
pub async fn activity_save_snapshot(
    core: tauri::State<'_, Arc<Core>>,
    id: String,
    snapshot: String,
) -> Result<(), String> {
    activity(&id)?;
    if snapshot.len() > SNAPSHOT_MAX {
        return Err("quota_exceeded".into());
    }
    // A null state would replace good progress with nothing; the host never sends one.
    match serde_json::from_str::<serde_json::Value>(&snapshot) {
        Ok(serde_json::Value::Null) | Err(_) => return Err("invalid_snapshot".into()),
        Ok(_) => {}
    }
    let core = Arc::clone(&core);
    blocking(move || {
        with_store(&core, |s| {
            s.save_snapshot(&id, &snapshot, OffsetDateTime::now_utc(), SNAPSHOT_MAX)
        })
    })
    .await
}

#[tauri::command]
pub async fn activity_clear_snapshot(
    core: tauri::State<'_, Arc<Core>>,
    id: String,
) -> Result<(), String> {
    activity(&id)?;
    let core = Arc::clone(&core);
    blocking(move || with_store(&core, |s| s.clear_snapshot(&id))).await
}

#[cfg(test)]
mod tests {
    use super::*;

    const APP: &str = r#"{"schemaVersion":1,"kind":"app","id":"app.lullshift.test","version":"1.0.0",
        "name":"Test","category":"arcade","minHostVersion":"0.1.0","sdkVersion":"0.1.0",
        "permissions":["activity.storage"],"offline":true,"entrypoint":"index.html",
        "runtime":"isolated-web-v1","interruptibility":{"pause":true,"resume":true,"persistState":true}}"#;

    #[test]
    fn manifests_must_be_interruptible_sandboxed_apps() {
        assert!(parse_manifest(APP).is_some());
        for (from, to) in [
            (r#""kind":"app""#, r#""kind":"content""#),
            (r#""runtime":"isolated-web-v1""#, r#""runtime":"native""#),
            (
                r#""entrypoint":"index.html""#,
                r#""entrypoint":"main.html""#,
            ),
            (r#""persistState":true"#, r#""persistState":false"#),
            (r#"["activity.storage"]"#, r#"["activity.network"]"#),
            (r#""id":"app.lullshift.test""#, r#""id":"App.Test""#),
            (r#""category":"arcade""#, r#""category":"admin""#),
        ] {
            assert!(parse_manifest(&APP.replace(from, to)).is_none(), "{to}");
        }
    }

    #[test]
    fn ids_follow_the_manifest_pattern() {
        for ok in ["app.lullshift.test", "a1.b-2.c", "x.y"] {
            assert!(valid_id(ok), "{ok}");
        }
        for bad in ["", "app", "app.", ".app", "App.x", "a..b", "a.b_c", "a/b.c"] {
            assert!(!valid_id(bad), "{bad}");
        }
        assert!(!valid_id(&format!("a.{}", "b".repeat(160))));
    }

    #[test]
    fn paths_are_split_and_traversal_is_refused() {
        assert_eq!(
            split_path("/app.lullshift.test/index.html"),
            Some(("app.lullshift.test", "index.html"))
        );
        assert_eq!(
            split_path("/app.lullshift.test/img/a-1_b.svg"),
            Some(("app.lullshift.test", "img/a-1_b.svg"))
        );
        for bad in [
            "",
            "/",
            "/app.lullshift.test/",
            "/app.lullshift.test",
            "app.lullshift.test/index.html",
            "/app.lullshift.test/../x/index.html",
            "/app.lullshift.test/a/../../index.html",
            "/app.lullshift.test/.hidden",
            "/app.lullshift.test/a%2e%2e/index.html",
            "/app.lullshift.test/a\\index.html",
            "/app.lullshift.test//index.html",
            "/../manifest.json",
            "/app.lullshift.test/a/b/c/d/e/f/g/h/i.js",
        ] {
            assert_eq!(split_path(bad), None, "{bad}");
        }
    }

    #[test]
    fn only_static_web_types_are_served() {
        assert_eq!(mime("index.html"), Some("text/html; charset=utf-8"));
        assert_eq!(mime("a/b.MJS"), Some("text/javascript; charset=utf-8"));
        for bad in ["run.exe", "x.wasm", "a.sh", "noext", "a.php", "x.dll"] {
            assert_eq!(mime(bad), None, "{bad}");
        }
    }

    #[test]
    fn csp_blocks_network_frames_workers_and_forms() {
        let csp = activity_csp();
        assert!(csp.starts_with("sandbox allow-scripts;"), "{csp}");
        for d in [
            "default-src 'none'",
            "connect-src 'none'",
            "frame-src 'none'",
            "worker-src 'none'",
            "object-src 'none'",
            "form-action 'none'",
            "base-uri 'none'",
            "script-src 'self';",
        ] {
            assert!(csp.contains(d), "{d}");
        }
        assert!(!csp.contains("unsafe"), "no unsafe-inline or unsafe-eval");
    }

    #[test]
    fn bundled_activities_are_found_and_served_with_csp() {
        let listed: Vec<&str> = catalog()
            .values()
            .filter(|a| a.listed)
            .map(|a| a.manifest.id.as_str())
            .collect();
        assert!(!listed.is_empty(), "at least one bundled activity");
        let id = listed[0];
        let ok = serve(
            &tauri::http::Request::get(format!("{SCHEME}://localhost/{id}/index.html"))
                .body(Vec::new())
                .expect("request"),
        );
        assert_eq!(ok.status(), StatusCode::OK);
        assert_eq!(
            ok.headers()[header::CONTENT_SECURITY_POLICY],
            activity_csp().as_str()
        );
        for (method, path) in [
            ("GET", format!("/{id}/missing.html")),
            ("GET", format!("/{id}/../{id}/index.html")),
            ("GET", "/app.unknown.activity/index.html".to_owned()),
            ("POST", format!("/{id}/index.html")),
        ] {
            let r = serve(
                &tauri::http::Request::builder()
                    .method(method)
                    .uri(format!("{SCHEME}://localhost{path}"))
                    .body(Vec::new())
                    .expect("request"),
            );
            assert_eq!(r.status(), StatusCode::NOT_FOUND, "{method} {path}");
            assert!(r.headers().contains_key(header::CONTENT_SECURITY_POLICY));
        }
    }

    #[test]
    fn storage_needs_the_permission_and_a_valid_key() {
        let id = catalog()
            .values()
            .find(|a| {
                a.manifest
                    .permissions
                    .iter()
                    .any(|p| p == "activity.storage")
            })
            .map(|a| a.manifest.id.clone())
            .expect("an activity with storage");
        assert!(storage_target(&id, "best-score").is_ok());
        assert_eq!(storage_target(&id, "").unwrap_err(), "invalid_key");
        assert_eq!(storage_target(&id, "a/b").unwrap_err(), "invalid_key");
        assert_eq!(
            storage_target(&id, &"k".repeat(65)).unwrap_err(),
            "invalid_key"
        );
        assert_eq!(
            storage_target("app.unknown.activity", "k").unwrap_err(),
            "unknown_activity"
        );
    }
}
