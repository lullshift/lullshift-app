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
//!
//! Content packs (ADR-016) are listed and run like apps, under their own id: the iframe loads
//! `/<pack id>/index.html`, which serves the *engine's* files, so each pack has its own URL,
//! snapshots, storage and play time. Engines themselves are never listed, served or launched.
//! A pack's data is checked against its engine's data schema ([`marketplace::data_schema`]) before
//! anything is served, and reaches the engine only as `init.content` (`activity_content`).
//!
//! Activities installed from a marketplace (Sprint 08, ADR-015) are served the same way, under
//! the same CSP and path rules, from `<app data>/installed/<id>/<commit>/`, and only while they
//! are enabled and still match their install digest (see [`installed`]). Bundled ids always win:
//! an installed activity can never shadow a bundled one.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

use include_dir::{Dir, include_dir};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::http::{Response, StatusCode, header};
use time::OffsetDateTime;

pub(crate) mod installed;

use crate::Core;
use installed::Served;
use marketplace::VerifyError;
use marketplace::data_schema;
#[cfg(test)]
use marketplace::manifest::plain;
use marketplace::manifest::{Help, Manifest, parse_manifest, valid_id, valid_rel_path};

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
/// Largest content-pack data file (ADR-016).
pub const DATA_MAX: usize = 256 * 1024;
/// Largest engine data schema.
const DATA_SCHEMA_MAX: usize = 256 * 1024;

static BUNDLED: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/../../../activities");
/// Debug builds also carry the sandbox probe used by the denial tests (S03-T05).
#[cfg(debug_assertions)]
static PROBE: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/sandbox-probe");

#[derive(Clone)]
pub(crate) struct Activity {
    pub(crate) manifest: Manifest,
    pub(crate) dir: &'static Dir<'static>,
    /// Shown in the Activities list. The sandbox probe is listed too, but exists only in
    /// debug builds.
    pub(crate) listed: bool,
    pub(crate) role: Role,
}

/// What an activity folder is (ADR-016).
#[derive(Clone)]
pub(crate) enum Role {
    /// A sandboxed web app with its own files.
    App,
    /// An app that runs content packs; hidden. None if its `engine.json` is invalid, so no
    /// pack can name it.
    Engine(Option<EngineDef>),
    /// Code-free data for an engine.
    Pack,
}

#[derive(Clone)]
pub(crate) struct EngineDef {
    /// `id@major`, as packs name it.
    pub(crate) reference: String,
    /// The checked data schema, or why it can't be used (packs naming it say so).
    pub(crate) schema: Result<Value, String>,
}

/// `engine.json` beside an engine's manifest.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EngineFile {
    schema_version: u32,
    id: String,
    major: u32,
    data_schema: String,
}

/// Why a pack can't start. Shown to the user as a calm card; `detail` is plain text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentError {
    /// `unknown_activity`, `engine_missing`, `engine_unusable`, `data_missing`,
    /// `data_too_large`, `data_not_json`, `data_invalid`, `damaged` (an installed pack's files
    /// changed on disk since its check) or `internal`.
    code: &'static str,
    detail: String,
}

impl ContentError {
    #[cfg(test)]
    pub(crate) fn code(&self) -> &'static str {
        self.code
    }

    pub(crate) fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }
}

/// What the UI shows for one activity.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityInfo {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) category: String,
    version: String,
    url: String,
    storage: bool,
    saved_utc: Option<String>,
    pub(crate) help: Option<Help>,
    /// True if the manifest names a preview image; fetch it with `activity_preview`.
    preview: bool,
    /// Total time played, in seconds, measured by the host.
    pub(crate) played_seconds: i64,
    /// Content packs: the engine reference (`id@major`); None for apps.
    engine: Option<String>,
    /// Where the activity came from.
    source: Source,
}

/// Which marketplace an activity came from.
#[cfg(test)]
impl ActivityInfo {
    pub(crate) fn id(&self) -> &str {
        &self.id
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    marketplace_id: String,
    builtin: bool,
    /// The pinned commit of an installed activity; null for bundled ones.
    commit: Option<String>,
}

impl Source {
    fn bundled() -> Self {
        Self {
            marketplace_id: installed::BUILTIN_ID.to_owned(),
            builtin: true,
            commit: None,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    snapshot: String,
    saved_utc: String,
}

pub(crate) fn catalog() -> &'static BTreeMap<String, Activity> {
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

/// Finds activity folders (those with a valid `manifest.json`) up to three levels down
/// (`activities/content/<engine>/<pack>`).
fn collect(
    dir: &'static Dir<'static>,
    listed: bool,
    depth: u8,
    out: &mut BTreeMap<String, Activity>,
) {
    if let Some(file) = dir.get_file(dir.path().join("manifest.json")) {
        if let Some(manifest) = file.contents_utf8().and_then(parse_manifest) {
            // Engines (ADR-016) run content packs; on their own they have nothing to show, so
            // they are never listed, served or launched.
            let role = if manifest.kind == "content" {
                Role::Pack
            } else if let Some(e) = dir.get_file(dir.path().join("engine.json")) {
                Role::Engine(parse_engine(dir, e.contents()))
            } else {
                Role::App
            };
            // A duplicate ID is a packaging bug; keep the first and never serve both.
            out.entry(manifest.id.clone()).or_insert(Activity {
                manifest,
                dir,
                listed: listed && !matches!(role, Role::Engine(_)),
                role,
            });
        }
        return;
    }
    if depth < 3 {
        for sub in dir.dirs() {
            collect(sub, listed, depth + 1, out);
        }
    }
}

/// An engine's `engine.json` and data schema, checked (ADR-016). None if `engine.json` itself
/// is invalid; a schema problem is kept in the definition.
fn parse_engine(dir: &Dir<'_>, json: &[u8]) -> Option<EngineDef> {
    let e: EngineFile = serde_json::from_slice(json).ok()?;
    let ok = e.schema_version == 1
        && valid_id(&e.id)
        && (1..=9999).contains(&e.major)
        && !e.data_schema.contains('/')
        && valid_rel_path(&e.data_schema, &["json"]);
    if !ok {
        return None;
    }
    let schema = (|| {
        let file = dir
            .get_file(dir.path().join(&e.data_schema))
            .ok_or("its data schema is missing")?;
        if file.contents().len() > DATA_SCHEMA_MAX {
            return Err("its data schema is too large".to_owned());
        }
        let schema: Value = serde_json::from_slice(file.contents())
            .map_err(|_| "its data schema isn't valid JSON".to_owned())?;
        data_schema::check_schema(&schema)?;
        Ok(schema)
    })();
    Some(EngineDef {
        reference: format!("{}@{}", e.id, e.major),
        schema,
    })
}

/// The bundled activities shown in the library (engines excluded), with their kind.
pub(crate) fn bundled_listed() -> Vec<(Manifest, &'static str)> {
    catalog()
        .values()
        .filter(|a| a.listed)
        .map(|a| {
            let kind = if matches!(a.role, Role::Pack) {
                "content"
            } else {
                "app"
            };
            (a.manifest.clone(), kind)
        })
        .collect()
}

/// An app or pack by id. Engines are not activities of their own.
fn runnable(id: &str) -> Option<&'static Activity> {
    catalog()
        .get(id)
        .filter(|a| !matches!(a.role, Role::Engine(_)))
}

#[cfg(test)]
fn activity(id: &str) -> Result<&'static Activity, String> {
    runnable(id).ok_or_else(|| "unknown_activity".to_owned())
}

/// The engine a pack names: exactly one bundled engine must claim `id@major`, and its data
/// schema must be usable.
pub(crate) fn find_engine(
    reference: &str,
) -> Result<(&'static Activity, &'static Value), ContentError> {
    let mut found = catalog().values().filter_map(|a| match &a.role {
        Role::Engine(Some(def)) if def.reference == reference => Some((a, def)),
        _ => None,
    });
    match (found.next(), found.next()) {
        (Some((a, def)), None) => match &def.schema {
            Ok(schema) => Ok((a, schema)),
            Err(why) => Err(ContentError::new(
                "engine_unusable",
                format!("the {reference} engine can't be used: {why}"),
            )),
        },
        (Some(_), Some(_)) => Err(ContentError::new(
            "engine_unusable",
            format!("more than one engine claims {reference}"),
        )),
        (None, _) => Err(ContentError::new(
            "engine_missing",
            format!("no bundled engine provides {reference}"),
        )),
    }
}

type Checked = Result<(&'static Activity, Value), ContentError>;

/// A pack's engine and its checked data. Bundled files don't change while the app runs, so
/// each pack is checked once, on first use.
fn pack_content(a: &Activity) -> Result<(&'static Activity, &'static Value), ContentError> {
    static CHECKED: OnceLock<BTreeMap<String, Checked>> = OnceLock::new();
    let all = CHECKED.get_or_init(|| {
        catalog()
            .values()
            .filter(|p| matches!(p.role, Role::Pack))
            .map(|p| (p.manifest.id.clone(), check_pack(p)))
            .collect()
    });
    match all.get(&a.manifest.id) {
        Some(Ok((engine, data))) => Ok((engine, data)),
        Some(Err(e)) => Err(e.clone()),
        None => Err(ContentError::new("unknown_activity", "not a content pack")),
    }
}

/// Resolves a pack's engine and checks its data file: present, at most [`DATA_MAX`] bytes, JSON,
/// and valid against the engine's data schema.
fn check_pack(p: &Activity) -> Checked {
    let reference = p.manifest.engine.as_deref().unwrap_or_default();
    let (engine, schema) = find_engine(reference)?;
    let data_path = p.manifest.data.as_deref().unwrap_or_default();
    let file = p
        .dir
        .get_file(p.dir.path().join(data_path))
        .ok_or_else(|| ContentError::new("data_missing", format!("{data_path} is missing")))?;
    let data = validate_data(schema, data_path, file.contents())?;
    check_images(&p.manifest, &data, |path| {
        p.dir
            .get_file(p.dir.path().join(path))
            .map(include_dir::File::contents)
    })?;
    Ok((engine, data))
}

/// A pack's pictures (ADR-016 amendment, S12-T11): every image the manifest lists is present
/// and passes [`pack_media::check_image`] (container walk, size from the header before any
/// pixel buffer exists, full decode), and every image the data names is listed. Bundled and
/// installed packs share this check, and the installer runs the same one.
pub(crate) fn check_images<'a>(
    manifest: &Manifest,
    data: &Value,
    read: impl Fn(&str) -> Option<&'a [u8]>,
) -> Result<(), ContentError> {
    // `parse_manifest` already limited the list to safe `images/<name>.png|webp` paths, so
    // they can be shown as they are.
    for image in &manifest.images {
        let bytes = read(image)
            .ok_or_else(|| ContentError::new("image_invalid", format!("{image} is missing")))?;
        pack_media::check_image(image, bytes)
            .map_err(|why| ContentError::new("image_invalid", format!("{image} {why}")))?;
    }
    pack_media::check_declared(&manifest.images, data)
        .map_err(|why| ContentError::new("image_invalid", why))
}

/// A pack's data file against its engine's schema: at most [`DATA_MAX`] bytes, JSON, and valid.
/// Bundled and installed packs share this check.
pub(crate) fn validate_data(
    schema: &Value,
    data_path: &str,
    contents: &[u8],
) -> Result<Value, ContentError> {
    if contents.len() > DATA_MAX {
        return Err(ContentError::new(
            "data_too_large",
            format!("{data_path} is over 256 KiB"),
        ));
    }
    let data: Value = serde_json::from_slice(contents)
        .map_err(|_| ContentError::new("data_not_json", format!("{data_path} isn't valid JSON")))?;
    // The schema, then the host's plain-text rule on every string (security review LOW-2).
    let problems = marketplace::pack_text::pack_data_problems(schema, &data);
    if !problems.is_empty() {
        let shown: Vec<String> = problems
            .iter()
            .take(3)
            .map(|p| format!("{data_path}: {p}"))
            .collect();
        let more = problems.len().saturating_sub(3);
        let tail = if more > 0 {
            format!(" (and {more} more)")
        } else {
            String::new()
        };
        return Err(ContentError::new(
            "data_invalid",
            format!("{}{tail}", shown.join("; ")),
        ));
    }
    Ok(data)
}

/// The bundled engines this host provides, with usable data schemas, for checking packs read
/// from a marketplace.
pub(crate) fn available_engines() -> Vec<marketplace::AvailableEngine> {
    catalog()
        .values()
        .filter_map(|a| match &a.role {
            Role::Engine(Some(def)) => {
                def.schema
                    .as_ref()
                    .ok()
                    .map(|schema| marketplace::AvailableEngine {
                        reference: def.reference.clone(),
                        schema: schema.clone(),
                    })
            }
            _ => None,
        })
        .collect()
}

/// Whether the activity may use `activity.storage`. A pack needs it declared by both the pack
/// (what the user is shown) and its engine (the code that would use it).
fn has_storage(a: &Activity) -> bool {
    let declares = |m: &Manifest| m.permissions.iter().any(|p| p == "activity.storage");
    match a.role {
        Role::Pack => {
            declares(&a.manifest)
                && pack_content(a).is_ok_and(|(engine, _)| declares(&engine.manifest))
        }
        _ => declares(&a.manifest),
    }
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

/// A file to serve and its content type.
pub(crate) type FoundFile = (Cow<'static, [u8]>, &'static str);

/// The bytes and content type for a request path, if it names an allowed file of a bundled
/// activity, or of an installed one in `snap`.
///
/// A pack's path serves its engine's files, and only once the pack's data has passed the
/// engine's schema; the pack's own files are never served. Engines have no path of their own.
/// Bundled ids win over installed ones; a disabled bundled activity is not served.
#[cfg(test)]
fn lookup(snap: Option<&installed::Snapshot>, path: &str) -> Option<FoundFile> {
    lookup_checked(snap, path).ok().flatten()
}

/// [`lookup`], except that an installed file which no longer matches its install check, or
/// can't be read right now, is `Err((activity id, why))`: nothing is served, and the caller
/// marks the activity damaged only for a mismatch.
fn lookup_checked(
    snap: Option<&installed::Snapshot>,
    path: &str,
) -> Result<Option<FoundFile>, (String, VerifyError)> {
    let Some((id, file)) = split_path(path) else {
        return Ok(None);
    };
    let Some(content_type) = mime(file) else {
        return Ok(None);
    };
    if let Some(activity) = runnable(id) {
        if snap.is_some_and(|s| s.disabled.contains(id)) {
            return Ok(None);
        }
        let source = match activity.role {
            Role::Pack => {
                let Ok((engine, _)) = pack_content(activity) else {
                    return Ok(None);
                };
                if file.starts_with(pack_media::IMAGE_DIR) {
                    // The pack's own pictures, only those its manifest lists, and only once
                    // the pack (images included) passed its check. Never an engine file.
                    return Ok(pack_image(&activity.manifest, file, |p| {
                        activity
                            .dir
                            .get_file(activity.dir.path().join(p))
                            .map(|f| Cow::Borrowed(f.contents()))
                    }));
                }
                engine
            }
            _ => activity,
        };
        let found = source.dir.get_file(source.dir.path().join(file));
        return Ok(found.map(|f| (Cow::Borrowed(f.contents()), content_type)));
    }
    let Some(served) = snap.and_then(|s| s.served.get(id)) else {
        return Ok(None);
    };
    // An installed file (a pack's picture included) is read and hashed again now.
    served
        .file(file, content_type)
        .map_err(|e| (id.to_owned(), e))
}

/// The content type of a picture the manifest lists (`image/png` or `image/webp`, from the
/// checked path; the bytes were checked to match it), or None if it isn't listed.
pub(crate) fn listed_image_type(manifest: &Manifest, file: &str) -> Option<&'static str> {
    if !manifest.images.iter().any(|p| p == file) {
        return None;
    }
    match file.rsplit_once('.')?.1 {
        "png" => Some("image/png"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

/// A listed pack picture with its content type. Anything not listed is not served.
fn pack_image(
    manifest: &Manifest,
    file: &str,
    read: impl FnOnce(&str) -> Option<Cow<'static, [u8]>>,
) -> Option<(Cow<'static, [u8]>, &'static str)> {
    let content_type = listed_image_type(manifest, file)?;
    Some((read(file)?, content_type))
}

/// Serves one request on the `lullact` scheme. Only GET; everything carries the activity CSP.
/// Without the app state (before setup finishes) only bundled activities can be served.
pub fn serve(core: Option<&Core>, request: &tauri::http::Request<Vec<u8>>) -> Response<Vec<u8>> {
    let snap = core.and_then(|c| c.library.snapshot(c).ok());
    let found = if request.method() == tauri::http::Method::GET {
        match lookup_checked(snap.as_deref(), request.uri().path()) {
            Ok(found) => found,
            Err((id, why)) => {
                // A file that only couldn't be read is a 404 for this request; a retry may
                // work. Only a changed file turns the activity off (security review INFO-1).
                if why == VerifyError::Mismatch
                    && let Some(core) = core
                {
                    installed::mark_damaged(core, &id);
                }
                None
            }
        }
    } else {
        None
    };
    let (status, body, content_type) = match found {
        Some((bytes, ct)) => (StatusCode::OK, bytes.into_owned(), ct),
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

/// An activity the host can act on: bundled, or installed and currently runnable.
enum Found {
    Bundled(&'static Activity),
    Installed(Arc<Served>),
}

/// Finds an activity by id. With `launch`, a bundled activity the user disabled is refused (it
/// is hidden from the library and must not start); without it (saved state, play time) the
/// activity only has to exist. Installed activities must be enabled, verified and runnable.
fn find(core: &Core, id: &str, launch: bool) -> Result<Found, String> {
    if let Some(a) = runnable(id) {
        if launch {
            let snap = core.library.snapshot(core)?;
            if snap.disabled.contains(id) {
                return Err("unknown_activity".into());
            }
        }
        return Ok(Found::Bundled(a));
    }
    let snap = core.library.snapshot(core)?;
    snap.served
        .get(id)
        .map(|s| Found::Installed(Arc::clone(s)))
        .ok_or_else(|| "unknown_activity".to_owned())
}

/// Whether the activity may use `activity.storage`.
fn found_has_storage(f: &Found) -> bool {
    match f {
        Found::Bundled(a) => has_storage(a),
        Found::Installed(s) => s.has_storage(),
    }
}

/// An activity that declared `activity.storage`, and a valid key.
pub(crate) fn storage_target(core: &Core, id: &str, key: &str) -> Result<(), String> {
    let f = find(core, id, false)?;
    if !found_has_storage(&f) {
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
    blocking(move || list_activities(&core)).await
}

/// Enabled bundled activities, then installed ones that are enabled and verified. Engines,
/// disabled bundled activities and tampered or broken installs are left out.
pub(crate) fn list_activities(core: &Core) -> Result<Vec<ActivityInfo>, String> {
    let snap = core.library.snapshot(core)?;
    let saved: BTreeMap<String, String> = with_store(core, |s| s.snapshots())
        .unwrap_or_default()
        .into_iter()
        .collect();
    let played: BTreeMap<String, i64> = with_store(core, |s| s.play_times())
        .unwrap_or_default()
        .into_iter()
        .collect();
    let saved_of = |id: &String| saved.get(id).cloned();
    let played_of = |id: &String| played.get(id).copied().unwrap_or(0);
    let bundled = catalog()
        .values()
        .filter(|a| a.listed && !snap.disabled.contains(&a.manifest.id))
        .map(|a| {
            let id = &a.manifest.id;
            info(a, saved_of(id), played_of(id))
        });
    let installed = snap
        .served
        .iter()
        .map(|(id, s)| installed_info(s, saved_of(id), played_of(id)));
    Ok(bundled.chain(installed).collect())
}

/// Ids of the listed activities. For tests.
#[cfg(test)]
pub(crate) fn list_ids(core: &Core) -> Result<Vec<String>, String> {
    Ok(list_activities(core)?.into_iter().map(|i| i.id).collect())
}

/// Every bundled and installed activity, disabled and broken ones included, so the user can
/// re-enable, update or remove them.
#[tauri::command]
pub async fn activities_manage_list(
    core: tauri::State<'_, Arc<Core>>,
) -> Result<Vec<installed::ManageItem>, String> {
    let core = Arc::clone(&core);
    blocking(move || Ok(core.library.snapshot(&core)?.manage.clone())).await
}

/// One activity, for the host to launch.
#[tauri::command]
pub async fn activity_info(
    core: tauri::State<'_, Arc<Core>>,
    id: String,
) -> Result<ActivityInfo, String> {
    let core = Arc::clone(&core);
    blocking(move || {
        let found = find(&core, &id, true)?;
        let saved = with_store(&core, |s| s.snapshot(&id))
            .ok()
            .flatten()
            .map(|(_, t)| t);
        let played = with_store(&core, |s| s.play_time(&id)).unwrap_or(0);
        Ok(match found {
            Found::Bundled(a) => info(a, saved, played),
            Found::Installed(s) => installed_info(&s, saved, played),
        })
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
        storage: has_storage(a),
        saved_utc,
        help: m.help.clone(),
        preview: m.preview.is_some(),
        played_seconds,
        engine: m.engine.clone(),
        source: Source::bundled(),
    }
}

fn installed_info(s: &Served, saved_utc: Option<String>, played_seconds: i64) -> ActivityInfo {
    let m = &s.manifest;
    ActivityInfo {
        id: m.id.clone(),
        name: m.name.clone(),
        description: m.description.clone(),
        category: m.category.clone(),
        version: m.version.clone(),
        url: entry_url(&m.id),
        storage: s.has_storage(),
        saved_utc,
        help: m.help.clone(),
        preview: m.preview.is_some(),
        played_seconds,
        engine: m.engine.clone(),
        source: Source {
            marketplace_id: s.marketplace_id.clone(),
            builtin: false,
            commit: Some(s.commit.clone()),
        },
    }
}

/// What the host hands an activity as `init.content`: a pack's data, checked against its
/// engine's data schema, or null for an app. The UI starts a pack only after this succeeds.
#[tauri::command]
pub async fn activity_content(
    core: tauri::State<'_, Arc<Core>>,
    id: String,
) -> Result<Value, ContentError> {
    let core = Arc::clone(&core);
    tauri::async_runtime::spawn_blocking(move || content_for_core(&core, &id))
        .await
        .map_err(|_| ContentError::new("internal", "the check didn't finish"))?
}

/// A bundled pack's checked data, or an installed pack's data, read and hashed again now and
/// checked against its engine's schema (so nothing changed on disk since the install check
/// reaches the engine; a changed file marks the activity damaged).
pub(crate) fn content_for_core(core: &Core, id: &str) -> Result<Value, ContentError> {
    match find(core, id, true) {
        Ok(Found::Bundled(a)) => match a.role {
            Role::Pack => pack_content(a).map(|(_, data)| data.clone()),
            _ => Ok(Value::Null),
        },
        Ok(Found::Installed(s)) => match s.pack_data() {
            Ok(data) => data,
            Err(VerifyError::Mismatch) => {
                installed::mark_damaged(core, id);
                Err(ContentError::new(
                    "damaged",
                    "this activity's files changed on disk; reinstall it",
                ))
            }
            // Locked by another program, for example: refuse this load, keep the activity on,
            // and let the user try again (security review INFO-1).
            Err(VerifyError::Unreadable) => Err(ContentError::new(
                "unreadable",
                "this activity's files can't be read right now; try again",
            )),
        },
        Err(code) if code == "unknown_activity" => Err(ContentError::new("unknown_activity", "")),
        Err(_) => Err(ContentError::new("internal", "the check didn't finish")),
    }
}

#[cfg(test)]
fn content_for(id: &str) -> Result<Value, ContentError> {
    let a = runnable(id).ok_or_else(|| ContentError::new("unknown_activity", ""))?;
    match a.role {
        Role::Pack => pack_content(a).map(|(_, data)| data.clone()),
        _ => Ok(Value::Null),
    }
}

/// The activity's preview image as a `data:` URL, or None if it has none (or it is missing,
/// too large or of the wrong type). The UI shows it only in an `<img>`, so the app's CSP stays
/// unchanged and an SVG never runs script.
#[tauri::command]
pub async fn activity_preview(
    core: tauri::State<'_, Arc<Core>>,
    id: String,
) -> Result<Option<String>, String> {
    let core = Arc::clone(&core);
    blocking(move || preview_for_core(&core, &id)).await
}

/// [`activity_preview`] without Tauri. An installed activity's image is read and hashed again
/// now; one that no longer matches is not shown and marks the activity damaged.
pub(crate) fn preview_for_core(core: &Core, id: &str) -> Result<Option<String>, String> {
    Ok(match find(core, id, false)? {
        Found::Bundled(a) => a.manifest.preview.as_deref().and_then(preview_data_url(a)),
        Found::Installed(s) => {
            let content_type = s
                .manifest
                .preview
                .as_deref()
                .and_then(mime)
                .filter(|t| t.starts_with("image/"));
            let Some(t) = content_type else {
                return Ok(None);
            };
            match s.preview_bytes(PREVIEW_MAX) {
                Ok(bytes) => bytes.map(|b| format!("data:{t};base64,{}", base64(&b))),
                Err(VerifyError::Mismatch) => {
                    installed::mark_damaged(core, id);
                    None
                }
                Err(VerifyError::Unreadable) => None,
            }
        }
    })
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
    let core = Arc::clone(&core);
    blocking(move || {
        find(&core, &id, false)?;
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
    let core = Arc::clone(&core);
    blocking(move || {
        storage_target(&core, &id, &key)?;
        with_store(&core, |s| s.activity_value(&id, &key))
    })
    .await
}

#[tauri::command]
pub async fn activity_storage_set(
    core: tauri::State<'_, Arc<Core>>,
    id: String,
    key: String,
    value: String,
) -> Result<(), String> {
    let core = Arc::clone(&core);
    blocking(move || {
        storage_target(&core, &id, &key)?;
        if value.len() > VALUE_MAX {
            return Err("value_too_large".into());
        }
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
    let core = Arc::clone(&core);
    blocking(move || {
        storage_target(&core, &id, &key)?;
        with_store(&core, |s| s.remove_activity_value(&id, &key))
    })
    .await
}

#[tauri::command]
pub async fn activity_snapshot(
    core: tauri::State<'_, Arc<Core>>,
    id: String,
) -> Result<Option<Snapshot>, String> {
    let core = Arc::clone(&core);
    blocking(move || {
        find(&core, &id, false)?;
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
    let core = Arc::clone(&core);
    blocking(move || {
        find(&core, &id, false)?;
        if snapshot.len() > SNAPSHOT_MAX {
            return Err("quota_exceeded".into());
        }
        // A null state would replace good progress with nothing; the host never sends one.
        match serde_json::from_str::<serde_json::Value>(&snapshot) {
            Ok(serde_json::Value::Null) | Err(_) => return Err("invalid_snapshot".into()),
            Ok(_) => {}
        }
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
    let core = Arc::clone(&core);
    blocking(move || {
        find(&core, &id, false)?;
        with_store(&core, |s| s.clear_snapshot(&id))
    })
    .await
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
            None,
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
                None,
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
            .find(|a| matches!(a.role, Role::App) && has_storage(a))
            .map(|a| a.manifest.id.clone())
            .expect("an activity with storage");
        let dir = tempfile::tempdir().expect("tempdir");
        let core = Core::for_tests(dir.path());
        assert!(storage_target(&core, &id, "best-score").is_ok());
        assert_eq!(storage_target(&core, &id, "").unwrap_err(), "invalid_key");
        assert_eq!(
            storage_target(&core, &id, "a/b").unwrap_err(),
            "invalid_key"
        );
        assert_eq!(
            storage_target(&core, &id, &"k".repeat(65)).unwrap_err(),
            "invalid_key"
        );
        assert_eq!(
            storage_target(&core, "app.unknown.activity", "k").unwrap_err(),
            "unknown_activity"
        );
    }

    // ---- Engines and content packs (ADR-016, S07) ----

    const PACK: &str = r#"{"schemaVersion":1,"kind":"content","id":"app.lullshift.content.test",
        "version":"1.0.0","name":"Test Pack","category":"break","minHostVersion":"0.1.0",
        "sdkVersion":"0.2.0","permissions":[],"offline":true,"engine":"lullshift.cards@1",
        "data":"deck.json"}"#;

    fn packs() -> impl Iterator<Item = &'static Activity> {
        catalog().values().filter(|a| matches!(a.role, Role::Pack))
    }

    /// A pack that exists only in this test, with the given manifest and data file.
    fn fake_pack(manifest: &str, data: &[u8]) -> Activity {
        let data: &'static [u8] = Box::leak(data.to_vec().into_boxed_slice());
        let files: &'static [include_dir::DirEntry<'static>] =
            Box::leak(Box::new([include_dir::DirEntry::File(
                include_dir::File::new("fake/deck.json", data),
            )]));
        Activity {
            manifest: parse_manifest(manifest).expect("pack manifest"),
            dir: Box::leak(Box::new(Dir::new("fake", files))),
            listed: true,
            role: Role::Pack,
        }
    }

    #[test]
    fn pack_manifests_name_an_engine_and_a_data_file() {
        assert!(parse_manifest(PACK).is_some());
        let with = |extra: &str| PACK.replacen('{', &format!("{{{extra},"), 1);
        for bad in [
            r#""entrypoint":"index.html""#,
            r#""runtime":"isolated-web-v1""#,
            r#""interruptibility":{"pause":true,"resume":true,"persistState":true}"#,
        ] {
            assert!(parse_manifest(&with(bad)).is_none(), "{bad}");
        }
        for (from, to) in [
            (r#""engine":"lullshift.cards@1","#, ""),
            (r#""data":"deck.json""#, r#""data":"../deck.json""#),
            (r#""data":"deck.json""#, r#""data":"deck.js""#),
            ("@1\"", "@0\""),
            ("@1\"", "@01\""),
            ("@1\"", "@\""),
            (r#""lullshift.cards@1""#, r#""Lullshift.Cards@1""#),
            (r#""lullshift.cards@1""#, r#""lullshift.cards""#),
        ] {
            assert!(parse_manifest(&PACK.replace(from, to)).is_none(), "{to}");
        }
        // An app may not name an engine or data.
        let app_with = |extra: &str| APP.replacen('{', &format!("{{{extra},"), 1);
        assert!(parse_manifest(&app_with(r#""engine":"lullshift.cards@1""#)).is_none());
        assert!(parse_manifest(&app_with(r#""data":"deck.json""#)).is_none());
    }

    #[test]
    fn every_bundled_pack_resolves_its_engine_and_its_data_passes() {
        let mut count = 0;
        for p in packs() {
            count += 1;
            let id = &p.manifest.id;
            let (engine, data) = pack_content(p).unwrap_or_else(|e| panic!("{id}: {e:?}"));
            assert!(
                data.is_object() || data.is_array(),
                "{id}: data is a document"
            );
            assert_eq!(content_for(id).as_ref(), Ok(data), "{id}");
            assert!(p.listed, "{id} is listed");
            // The pack's path serves its engine's files, never the pack's own.
            let (index, _) = lookup(None, &format!("/{id}/index.html")).expect("engine index");
            let engine_index = engine
                .dir
                .get_file(engine.dir.path().join("index.html"))
                .expect("engine index.html");
            assert_eq!(&*index, engine_index.contents(), "{id}");
            let data_file = p.manifest.data.as_deref().expect("data");
            if engine
                .dir
                .get_file(engine.dir.path().join(data_file))
                .is_none()
            {
                assert!(
                    lookup(None, &format!("/{id}/{data_file}")).is_none(),
                    "{id} data"
                );
            }
            assert!(
                lookup(None, &format!("/{id}/manifest.json")).is_some_and(|(b, _)| {
                    b == engine
                        .dir
                        .get_file(engine.dir.path().join("manifest.json"))
                        .expect("engine manifest")
                        .contents()
                })
            );
            let i = info(p, None, 0);
            assert_eq!(i.id, *id);
            assert_eq!(i.engine.as_deref(), p.manifest.engine.as_deref());
            assert!(i.url.contains(&format!("/{id}/index.html")));
        }
        assert!(count >= 1, "at least one bundled content pack");
    }

    /// S12-T11: a pack's listed pictures are checked and served at its own path with the right
    /// type, nosniff and the unchanged activity CSP; nothing else under `images/` is served.
    #[test]
    fn pack_pictures_are_checked_and_served_only_when_listed() {
        let with_images: Vec<&Activity> =
            packs().filter(|p| !p.manifest.images.is_empty()).collect();
        assert!(!with_images.is_empty(), "a bundled pack has pictures");
        for p in &with_images {
            let id = &p.manifest.id;
            pack_content(p).unwrap_or_else(|e| panic!("{id}: {e:?}"));
            for image in &p.manifest.images {
                let (bytes, ct) =
                    lookup(None, &format!("/{id}/{image}")).unwrap_or_else(|| panic!("{image}"));
                let info = pack_media::check_image(image, &bytes).expect("checked image");
                assert_eq!(ct, info.kind.content_type(), "{image}");
                let r = serve(
                    None,
                    &tauri::http::Request::get(format!("{SCHEME}://localhost/{id}/{image}"))
                        .body(Vec::new())
                        .expect("request"),
                );
                assert_eq!(r.status(), StatusCode::OK);
                assert_eq!(r.headers()[header::CONTENT_TYPE], ct);
                assert_eq!(r.headers()[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
                assert_eq!(
                    r.headers()[header::CONTENT_SECURITY_POLICY],
                    activity_csp().as_str()
                );
            }
            for path in [
                "images/not-listed.png",
                "images/../manifest.json",
                "images/hexagon.svg",
                "images",
                "images/",
            ] {
                assert!(lookup(None, &format!("/{id}/{path}")).is_none(), "{path}");
            }
        }
        // A pack without pictures serves nothing from images/, and an engine has no path.
        let plain = packs()
            .find(|p| p.manifest.images.is_empty())
            .expect("a pack without pictures");
        let first = &with_images[0].manifest.images[0];
        assert!(lookup(None, &format!("/{}/{first}", plain.manifest.id)).is_none());
        // The activity CSP still allows images only from the activity's own origin (and data:).
        assert!(activity_csp().contains("img-src 'self' data:;"));
    }

    #[test]
    fn bad_pack_pictures_stop_the_pack() {
        let p = packs()
            .find(|p| !p.manifest.images.is_empty())
            .expect("a pack with pictures");
        let (_, data) = pack_content(p).expect("checks");
        let real = |path: &str| {
            p.dir
                .get_file(p.dir.path().join(path))
                .map(include_dir::File::contents)
        };
        check_images(&p.manifest, data, real).expect("the real pictures pass");
        let svg: &[u8] =
            br#"<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>"#;
        let err = check_images(&p.manifest, data, |_| Some(svg)).expect_err("svg");
        assert_eq!(err.code(), "image_invalid");
        let err = check_images(&p.manifest, data, |_| None).expect_err("missing");
        assert_eq!(err.code(), "image_invalid");
        // A real PNG under a .webp name is refused too: the type comes from the bytes and
        // must match the name.
        let png = real(&p.manifest.images[0]).expect("bytes");
        let err = check_images(&p.manifest, data, |_| Some(png)).expect_err("mismatch");
        assert_eq!(err.code(), "image_invalid");
        // Data that names a picture the manifest doesn't list.
        let mut sneaky = data.clone();
        sneaky["questions"][0]["image"]["src"] = Value::from("images/elsewhere.png");
        let err = check_images(&p.manifest, &sneaky, real).expect_err("undeclared");
        assert_eq!(err.code(), "image_invalid");
    }

    #[test]
    fn engines_are_hidden_and_have_usable_schemas() {
        let engines: Vec<&Activity> = catalog()
            .values()
            .filter(|a| matches!(a.role, Role::Engine(_)))
            .collect();
        assert!(!engines.is_empty(), "at least one bundled engine");
        for e in engines {
            let id = &e.manifest.id;
            match &e.role {
                Role::Engine(Some(def)) => {
                    assert!(def.schema.is_ok(), "{id}: {:?}", def.schema.as_ref().err());
                }
                _ => panic!("{id}: engine.json is invalid"),
            }
            assert!(!e.listed, "{id} is not listed");
            assert!(activity(id).is_err(), "{id} can't be launched on its own");
            assert!(
                lookup(None, &format!("/{id}/index.html")).is_none(),
                "{id} isn't served"
            );
            assert_eq!(
                content_for(id).unwrap_err().code,
                "unknown_activity",
                "{id}"
            );
        }
    }

    #[test]
    fn unknown_engines_and_majors_are_reported_not_panicked() {
        let deck = br#"{"title":"T","cards":[{"front":"Hello"}]}"#;
        assert!(check_pack(&fake_pack(PACK, deck)).is_ok());
        for (reference, code) in [
            ("lullshift.cards@99", "engine_missing"),
            ("lullshift.nothing@1", "engine_missing"),
        ] {
            let manifest = PACK.replace("lullshift.cards@1", reference);
            let err = check_pack(&fake_pack(&manifest, deck))
                .err()
                .expect("refused");
            assert_eq!(err.code, code, "{reference}");
            assert!(err.detail.contains(reference), "{}", err.detail);
        }
        assert_eq!(
            content_for("app.unknown.pack").unwrap_err().code,
            "unknown_activity"
        );
        assert!(find_engine("").is_err());
    }

    #[test]
    fn bad_pack_data_is_refused_with_a_plain_message() {
        let long = "x".repeat(201);
        let cases: Vec<(Vec<u8>, &str, &str)> = vec![
            (
                format!(r#"{{"title":"T","cards":[{{"front":"ok"}},{{"front":"{long}"}}]}}"#)
                    .into_bytes(),
                "data_invalid",
                "deck.json: cards[1].front is longer than 200 characters",
            ),
            (
                br#"{"title":"T","cards":[],"extra":1}"#.to_vec(),
                "data_invalid",
                "deck.json: cards needs at least 1 item; deck.json: extra isn't a known field",
            ),
            (
                b"{not json".to_vec(),
                "data_not_json",
                "deck.json isn't valid JSON",
            ),
            (
                vec![b' '; DATA_MAX + 1],
                "data_too_large",
                "deck.json is over 256 KiB",
            ),
        ];
        for (data, code, detail) in cases {
            let err = check_pack(&fake_pack(PACK, &data)).err().expect("refused");
            assert_eq!((err.code, err.detail.as_str()), (code, detail));
        }
        let missing = PACK.replace("deck.json", "other.json");
        assert_eq!(
            check_pack(&fake_pack(&missing, b"{}"))
                .err()
                .expect("refused")
                .code,
            "data_missing"
        );
    }

    #[test]
    fn engine_files_are_checked() {
        let schema = br#"{"type":"object","additionalProperties":false}"#;
        let files: &'static [include_dir::DirEntry<'static>] = Box::leak(Box::new([
            include_dir::DirEntry::File(include_dir::File::new("e/data.schema.json", schema)),
            include_dir::DirEntry::File(include_dir::File::new(
                "e/odd.schema.json",
                br#"{"type":"object","format":"uri"}"#,
            )),
        ]));
        let dir = Dir::new("e", files);
        let ok = br#"{"schemaVersion":1,"id":"lullshift.test","major":2,"dataSchema":"data.schema.json"}"#;
        let def = parse_engine(&dir, ok).expect("engine");
        assert_eq!(def.reference, "lullshift.test@2");
        assert!(def.schema.is_ok());
        let odd = parse_engine(
            &dir,
            br#"{"schemaVersion":1,"id":"lullshift.test","major":2,"dataSchema":"odd.schema.json"}"#,
        )
        .expect("engine");
        assert!(
            odd.schema.is_err(),
            "unsupported keywords make the engine unusable"
        );
        for bad in [
            r#"{"schemaVersion":2,"id":"lullshift.test","major":1,"dataSchema":"data.schema.json"}"#,
            r#"{"schemaVersion":1,"id":"Bad","major":1,"dataSchema":"data.schema.json"}"#,
            r#"{"schemaVersion":1,"id":"lullshift.test","major":0,"dataSchema":"data.schema.json"}"#,
            r#"{"schemaVersion":1,"id":"lullshift.test","major":1,"dataSchema":"../x.json"}"#,
            r#"{"schemaVersion":1,"id":"lullshift.test","major":1,"dataSchema":"sub/x.json"}"#,
            r#"{"schemaVersion":1,"id":"lullshift.test","major":1,"dataSchema":"x.json","more":1}"#,
        ] {
            assert!(parse_engine(&dir, bad.as_bytes()).is_none(), "{bad}");
        }
        let gone = parse_engine(
            &dir,
            br#"{"schemaVersion":1,"id":"lullshift.test","major":1,"dataSchema":"gone.json"}"#,
        )
        .expect("engine");
        assert!(gone.schema.is_err());
    }

    #[test]
    fn pack_storage_needs_both_the_pack_and_its_engine() {
        for p in packs() {
            let pack_says = p
                .manifest
                .permissions
                .iter()
                .any(|x| x == "activity.storage");
            if !pack_says {
                assert!(!has_storage(p), "{}", p.manifest.id);
            }
        }
    }

    /// The host validator agrees with Ajv on every engine fixture (S07-T02): valid fixtures pass
    /// and invalid ones fail. Fixtures live outside `activities/`, so they're read from disk.
    #[test]
    fn engine_fixtures_agree_with_the_host_validator() {
        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../tests/fixtures/engines");
        let mut disagree = Vec::new();
        let mut checked = 0;
        for e in catalog().values() {
            let Role::Engine(Some(def)) = &e.role else {
                continue;
            };
            let schema = match &def.schema {
                Ok(schema) => schema,
                Err(err) => {
                    disagree.push(format!("{}: schema refused: {err}", def.reference));
                    continue;
                }
            };
            for (kind, should_pass) in [("valid", true), ("invalid", false)] {
                let dir = base.join(&def.reference).join(kind);
                let Ok(entries) = std::fs::read_dir(&dir) else {
                    disagree.push(format!("{}: missing fixture folder", dir.display()));
                    continue;
                };
                let mut found = 0;
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|x| x.to_str()) != Some("json") {
                        continue;
                    }
                    found += 1;
                    checked += 1;
                    let text = std::fs::read(&path).expect("fixture");
                    let problems = match serde_json::from_slice::<Value>(&text) {
                        Ok(data) => marketplace::pack_text::pack_data_problems(schema, &data),
                        Err(_) => vec!["not JSON".to_owned()],
                    };
                    if problems.is_empty() != should_pass {
                        disagree.push(format!("{} ({kind}): {problems:?}", path.display()));
                    }
                }
                if found == 0 {
                    disagree.push(format!("{}: no .json fixtures", dir.display()));
                }
            }
        }
        assert!(checked > 0, "no engine fixtures were checked");
        assert!(disagree.is_empty(), "{disagree:#?}");
    }

    /// The same corpus runs through Ajv in `tests/activities/check-engine-compat.mjs`.
    #[test]
    fn schema_parity_corpus_agrees_with_the_host_validator() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../tests/fixtures/schema-parity/cases.json");
        let cases: Vec<Value> =
            serde_json::from_slice(&std::fs::read(&path).expect("corpus")).expect("corpus JSON");
        assert!(cases.len() >= 20, "the corpus shrank");
        let mut wrong = Vec::new();
        for case in &cases {
            let name = case["name"].as_str().unwrap_or("?");
            let want = case["valid"].as_bool().expect("valid");
            if let Err(err) = data_schema::check_schema(&case["schema"]) {
                wrong.push(format!("{name}: schema refused: {err}"));
                continue;
            }
            let problems = data_schema::validate(&case["schema"], &case["data"]);
            if problems.is_empty() != want {
                wrong.push(format!("{name}: expected valid={want}, got {problems:?}"));
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }
}
