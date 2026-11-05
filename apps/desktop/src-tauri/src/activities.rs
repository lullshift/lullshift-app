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
/// Largest preview image the host will show (ADR-012 additions, Sprint 05).
pub const PREVIEW_MAX: usize = 256 * 1024;
/// Most time one call may add to an activity's time played.
const PLAY_ADD_MAX: i64 = 3600;

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
    author: Option<String>,
    category: String,
    min_host_version: String,
    sdk_version: String,
    permissions: Vec<String>,
    entrypoint: Option<String>,
    runtime: Option<String>,
    interruptibility: Option<Interruptibility>,
    help: Option<Help>,
    preview: Option<String>,
}

/// How-to-play help (S05-T07). Plain text only; the UI renders it as text.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Help {
    summary: String,
    goal: Option<String>,
    how_to_play: Vec<String>,
    #[serde(default)]
    controls: Vec<HelpControl>,
    #[serde(default)]
    tips: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HelpControl {
    keys: String,
    action: String,
}

impl Help {
    /// The same limits as the manifest schema's `help`.
    fn is_valid(&self) -> bool {
        let list = |items: &[String], min: usize, max_items: usize, max: usize| {
            items.len() >= min && items.len() <= max_items && items.iter().all(|t| plain(t, max))
        };
        plain(&self.summary, 200)
            && self.goal.as_deref().is_none_or(|g| plain(g, 300))
            && list(&self.how_to_play, 1, 8, 300)
            && self.controls.len() <= 12
            && self
                .controls
                .iter()
                .all(|c| plain(&c.keys, 40) && plain(&c.action, 120))
            && list(&self.tips, 0, 6, 200)
    }
}

/// Plain text of 1 to `max` characters with no control, bidirectional-override or invisible
/// formatting characters. Mirrors the manifest schema and `isPlainText` in the SDK.
fn plain(text: &str, max: usize) -> bool {
    !text.is_empty() && text.chars().count() <= max && !text.chars().any(hidden_char)
}

fn hidden_char(c: char) -> bool {
    matches!(c,
        '\u{0}'..='\u{1F}' | '\u{7F}'..='\u{9F}' | '\u{AD}' | '\u{61C}' | '\u{180E}'
        | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2064}'
        | '\u{2066}'..='\u{206F}' | '\u{FEFF}')
}

/// A relative file path whose segments start with a letter, digit or underscore (so no `.`,
/// `..`, hidden names or leading `-`), at most 8 deep, ending in one of `exts`.
fn valid_rel_path(path: &str, exts: &[&str]) -> bool {
    let segments: Vec<&str> = path.split('/').collect();
    let ext_ok = path
        .rsplit_once('.')
        .is_some_and(|(_, e)| exts.contains(&e));
    path.len() <= 256
        && segments.len() <= 8
        && ext_ok
        && segments.iter().all(|s| {
            s.len() <= 64
                && s.bytes()
                    .next()
                    .is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_')
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        })
}

/// `MAJOR.MINOR.PATCH` with an optional pre-release, as in the manifest schema.
fn valid_semver(v: &str) -> bool {
    let (core, pre) = v.split_once('-').map_or((v, None), |(c, p)| (c, Some(p)));
    let nums: Vec<&str> = core.split('.').collect();
    v.len() <= 64
        && nums.len() == 3
        && nums
            .iter()
            .all(|n| (1..=5).contains(&n.len()) && n.bytes().all(|b| b.is_ascii_digit()))
        && pre.is_none_or(|p| {
            (1..=40).contains(&p.len())
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-'))
        })
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
    help: Option<Help>,
    /// True if the manifest names a preview image; fetch it with `activity_preview`.
    preview: bool,
    /// Total time played, in seconds, measured by the host.
    played_seconds: i64,
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
            // Engines (ADR-016) run content packs; on their own they have nothing to show, so
            // they are not listed. Running packs arrives in Sprint 07.
            let engine = dir.get_file(dir.path().join("engine.json")).is_some();
            // A duplicate ID is a packaging bug; keep the first and never serve both.
            out.entry(manifest.id.clone()).or_insert(Activity {
                manifest,
                dir,
                listed: listed && !engine,
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
        && plain(&m.name, 100)
        && (m.description.is_empty() || plain(&m.description, 500))
        && m.author
            .as_deref()
            .is_none_or(|a| a.is_empty() || plain(a, 100))
        && valid_semver(&m.version)
        && valid_semver(&m.min_host_version)
        && valid_semver(&m.sdk_version)
        && m.help.as_ref().is_none_or(Help::is_valid)
        && m.preview
            .as_deref()
            .is_none_or(|p| valid_rel_path(p, &["png", "jpg", "jpeg", "webp", "svg"]))
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
        let played: BTreeMap<String, i64> = with_store(&core, |s| s.play_times())
            .unwrap_or_default()
            .into_iter()
            .collect();
        Ok(catalog()
            .values()
            .filter(|a| a.listed)
            .map(|a| {
                let id = &a.manifest.id;
                info(
                    a,
                    saved.get(id).cloned(),
                    played.get(id).copied().unwrap_or(0),
                )
            })
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
        let played = with_store(&core, |s| s.play_time(&id)).unwrap_or(0);
        Ok(info(a, saved, played))
    })
    .await
}

fn info(a: &Activity, saved_utc: Option<String>, played_seconds: i64) -> ActivityInfo {
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
        help: m.help.clone(),
        preview: m.preview.is_some(),
        played_seconds,
    }
}

/// The activity's preview image as a `data:` URL, or None if it has none (or it is missing,
/// too large or of the wrong type). The UI shows it only in an `<img>`, so the app's CSP stays
/// unchanged and an SVG never runs script.
#[tauri::command]
pub fn activity_preview(id: String) -> Result<Option<String>, String> {
    let a = activity(&id)?;
    Ok(a.manifest.preview.as_deref().and_then(preview_data_url(a)))
}

fn preview_data_url(a: &Activity) -> impl Fn(&str) -> Option<String> + '_ {
    move |p| {
        let content_type = mime(p).filter(|t| t.starts_with("image/"))?;
        let file = a.dir.get_file(a.dir.path().join(p))?;
        let bytes = file.contents();
        (bytes.len() <= PREVIEW_MAX)
            .then(|| format!("data:{content_type};base64,{}", base64(bytes)))
    }
}

/// Adds time the activity spent running in view (S05-T09). Measured by the host, never the
/// activity; each call adds at most an hour.
#[tauri::command]
pub async fn activity_add_play(
    core: tauri::State<'_, Arc<Core>>,
    id: String,
    seconds: i64,
    launch: bool,
) -> Result<(), String> {
    activity(&id)?;
    let core = Arc::clone(&core);
    blocking(move || {
        with_store(&core, |s| {
            s.add_play_time(
                &id,
                seconds,
                launch,
                PLAY_ADD_MAX,
                OffsetDateTime::now_utc(),
            )
        })
    })
    .await
}

/// Standard base64 with padding.
fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        let n = (u32::from(chunk[0]) << 16) | (u32::from(b1) << 8) | u32::from(b2);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ABC[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
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
    fn manifest_text_versions_help_and_preview_are_checked() {
        let with = |extra: &str| APP.replacen('{', &format!("{{{extra},"), 1);
        let help = r#""help":{"summary":"Light them all.","howToPlay":["Press a lantern."],"controls":[{"keys":"Enter","action":"Flip"}]}"#;
        assert!(parse_manifest(&with(help)).is_some());
        assert!(parse_manifest(&with(r#""preview":"assets/preview.svg""#)).is_some());
        for bad in [
            r#""help":{"summary":"Hi"}"#,
            r#""help":{"summary":"Hi","howToPlay":["Go"],"html":"<b>"}"#,
            r#""help":{"summary":"Hi\u0007","howToPlay":["Go"]}"#,
            r#""help":{"summary":"Hi","howToPlay":[]}"#,
            r#""preview":"../x.png""#,
            r#""preview":".hidden.png""#,
            r#""preview":"x.gif""#,
            r#""author":"Lull\u200bshift""#,
        ] {
            assert!(parse_manifest(&with(bad)).is_none(), "{bad}");
        }
        for (from, to) in [
            (r#""name":"Test""#, r#""name":"Te\u202est""#),
            (r#""sdkVersion":"0.1.0""#, r#""sdkVersion":"latest""#),
            (r#""minHostVersion":"0.1.0""#, r#""minHostVersion":"1.0""#),
        ] {
            assert!(parse_manifest(&APP.replace(from, to)).is_none(), "{to}");
        }
    }

    #[test]
    fn plain_text_counts_characters_and_refuses_hidden_ones() {
        assert!(plain("Lanterns lit · 7/9", 24));
        assert!(plain(&"é".repeat(24), 24));
        assert!(!plain(&"é".repeat(25), 24));
        for c in [
            '\n', '\u{202E}', '\u{200B}', '\u{FEFF}', '\u{2066}', '\u{AD}', '\u{7F}',
        ] {
            assert!(!plain(&format!("a{c}b"), 24), "{c:?}");
        }
    }

    #[test]
    fn base64_matches_known_vectors() {
        for (input, out) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(input.as_bytes()), out);
        }
    }

    #[test]
    fn bundled_activities_have_help_and_a_preview_that_loads() {
        for a in catalog().values().filter(|a| {
            a.manifest.id.starts_with("app.lullshift.") && !a.manifest.id.contains(".test.")
        }) {
            let m = &a.manifest;
            assert!(m.help.is_some(), "{} has no help", m.id);
            let url = m.preview.as_deref().and_then(preview_data_url(a));
            assert!(
                url.is_some_and(|u| u.starts_with("data:image/")),
                "{} has no usable preview",
                m.id
            );
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
