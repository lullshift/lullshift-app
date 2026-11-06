//! Installed (marketplace) activities, served beside the bundled ones (Sprint 08, ADR-015).
//!
//! The library keeps one in-memory [`Snapshot`] of what may be shown and served:
//!
//! - which bundled activities the user disabled,
//! - which installed activities are enabled, still match their install digest and can run on a
//!   bundled engine, with a pack's checked data held in memory,
//! - a [`ManageItem`] for every bundled and installed activity, disabled or broken ones included.
//!
//! An installed activity is never read from the marketplace cache: only from
//! `<app data>/installed/<id>/<commit>/`, and only after `verify_install` agreed with the digest
//! recorded at install. The check is cached per `(id, commit, digest)` for the session; the cache
//! entries of an activity are dropped when it is installed, updated or uninstalled
//! ([`Library::invalidate`]). A digest mismatch is never cached, disables the activity in the
//! store and is reported as `tampered`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use local_store::{ActivityKind, InstalledActivity};
use marketplace::manifest::{Manifest, parse_manifest};
use marketplace::{VerifyError, install_dir, third_party_code_allowed, verified_files};
use serde::Serialize;
use serde_json::Value;
use time::OffsetDateTime;

use super::{
    Activity, DATA_MAX, Role, catalog, check_images, find_engine, pack_image, validate_data,
};
use crate::Core;

/// The id of the built-in marketplace.
pub(crate) const BUILTIN_ID: &str = local_store::BUILTIN_MARKETPLACE_ID;
/// Largest manifest read from an install folder.
const MANIFEST_MAX: usize = 128 * 1024;
/// Largest single file served from an install folder (same as the install limit).
const FILE_MAX: usize = 5 * 1024 * 1024;

/// What an installed activity is when it runs.
pub(crate) enum ServedRole {
    /// A sandboxed web app with its own files in `dir`. (Unreachable in this release: code from
    /// third-party marketplaces cannot be installed.)
    App,
    /// A content pack: the bundled engine's files, plus the pack's data, checked against the
    /// engine's schema.
    Pack {
        engine: &'static Activity,
        data: Value,
    },
}

/// One installed activity that may be shown and served.
pub(crate) struct Served {
    pub manifest: Manifest,
    /// Bytes that were hashed by the install check, and the only bytes ever served from this
    /// install (a file swapped on disk afterwards is never read; review L4). An app keeps all of
    /// its files (at most 20 MiB, and apps from third parties cannot be installed in this
    /// release); a pack keeps only its preview image and the pictures its manifest lists (S12-T11).
    /// Held for the session with the check.
    pub verified: BTreeMap<String, Vec<u8>>,
    pub commit: String,
    pub marketplace_id: String,
    pub role: ServedRole,
}

/// Why an installed activity can't run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Problem {
    /// The folder no longer matches the digest recorded at install (or the record is not
    /// something this release can run).
    Tampered,
    /// The bundled engine it names is gone or no longer accepts its data.
    MissingEngine,
}

impl Problem {
    fn as_str(self) -> &'static str {
        match self {
            Self::Tampered => "tampered",
            Self::MissingEngine => "missingEngine",
        }
    }
}

/// A row of the "manage activities" list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ManageItem {
    pub activity_id: String,
    pub name: String,
    /// `app` or `content`.
    pub kind: &'static str,
    pub builtin: bool,
    pub marketplace_id: String,
    pub enabled: bool,
    pub commit: Option<String>,
    pub version: String,
    /// `tampered`, `missingEngine` or null.
    pub problem: Option<&'static str>,
}

/// Everything the host needs to answer list, info, content and serve requests.
pub(crate) struct Snapshot {
    /// Bundled activities the user turned off.
    pub disabled: HashSet<String>,
    /// Installed activities that are enabled, verified and runnable, by id.
    pub served: BTreeMap<String, Arc<Served>>,
    pub manage: Vec<ManageItem>,
}

type Key = (String, String, String);

/// A checked install, remembered for the session.
#[derive(Clone)]
struct Evaluated {
    name: String,
    version: String,
    outcome: Result<Arc<Served>, Problem>,
}

/// The session state: the current snapshot and the verification cache.
#[derive(Default)]
pub(crate) struct Library {
    /// Bumped by every invalidation, so a snapshot built from older data is never kept.
    generation: AtomicU64,
    snapshot: RwLock<Option<Arc<Snapshot>>>,
    /// `(id, commit, digest)` -> result of the digest and pack-data checks. Mismatches are
    /// never stored here.
    checked: Mutex<HashMap<Key, Evaluated>>,
}

impl Library {
    /// The current snapshot, built on first use.
    pub(crate) fn snapshot(&self, core: &Core) -> Result<Arc<Snapshot>, String> {
        if let Some(s) = self.peek() {
            return Ok(s);
        }
        let started = self.generation.load(Ordering::SeqCst);
        let built = Arc::new(build(core, self)?);
        if self.generation.load(Ordering::SeqCst) == started
            && let Ok(mut slot) = self.snapshot.write()
        {
            *slot = Some(Arc::clone(&built));
        }
        Ok(built)
    }

    /// The current snapshot if there is one; never builds.
    pub(crate) fn peek(&self) -> Option<Arc<Snapshot>> {
        self.snapshot.read().ok().and_then(|s| s.clone())
    }

    /// Drops the snapshot and, for `activity_id`, its cached verification (install, update and
    /// uninstall call this); with `None` only the snapshot goes (enable, disable).
    pub(crate) fn invalidate(&self, activity_id: Option<&str>) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        if let Some(id) = activity_id
            && let Ok(mut cache) = self.checked.lock()
        {
            cache.retain(|(cached, _, _), _| cached != id);
        }
        if let Ok(mut slot) = self.snapshot.write() {
            *slot = None;
        }
    }

    /// Forgets every verification, as a fresh session would. For tests.
    #[cfg(test)]
    pub(crate) fn restart(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut cache) = self.checked.lock() {
            cache.clear();
        }
        if let Ok(mut slot) = self.snapshot.write() {
            *slot = None;
        }
    }

    /// How many verifications are cached. For tests.
    #[cfg(test)]
    pub(crate) fn cached_checks(&self) -> usize {
        self.checked.lock().map_or(0, |c| c.len())
    }
}

/// The install root, `<app data>/installed`.
pub(crate) fn install_root(core: &Core) -> PathBuf {
    core.app_dir.join("installed")
}

/// Reads a regular file (never through a link) of at most `max` bytes.
pub(crate) fn read_regular(dir: &Path, rel: &str, max: usize) -> Option<Vec<u8>> {
    let path = dir.join(rel);
    let meta = std::fs::symlink_metadata(&path).ok()?;
    if !meta.is_file() || usize::try_from(meta.len()).ok()? > max {
        return None;
    }
    std::fs::read(path).ok()
}

fn catalog_has(id: &str) -> bool {
    catalog().contains_key(id)
}

/// The manifest among the bytes the install check hashed.
fn verified_manifest(files: &[(String, Vec<u8>)]) -> Option<Manifest> {
    let (_, bytes) = files
        .iter()
        .find(|(p, b)| p == "manifest.json" && b.len() <= MANIFEST_MAX)?;
    parse_manifest(std::str::from_utf8(bytes).ok()?)
}

fn read_manifest(dir: &Path) -> Option<Manifest> {
    let bytes = read_regular(dir, "manifest.json", MANIFEST_MAX)?;
    parse_manifest(std::str::from_utf8(&bytes).ok()?)
}

/// Checks one install: digest, manifest, and for a pack its engine and data.
fn evaluate(core: &Core, lib: &Library, rec: &InstalledActivity) -> Evaluated {
    let fallback = |outcome| Evaluated {
        name: rec.activity_id.clone(),
        version: rec.manifest_version.clone(),
        outcome,
    };
    // A code activity from a third-party marketplace can't be installed in this release, so a
    // record for one is not something to run.
    if rec.kind == ActivityKind::App
        && rec.marketplace_id != BUILTIN_ID
        && !third_party_code_allowed()
    {
        return fallback(Err(Problem::Tampered));
    }
    let Ok(dir) = install_dir(&install_root(core), &rec.activity_id, &rec.commit_sha) else {
        return fallback(Err(Problem::Tampered));
    };
    let key = (
        rec.activity_id.clone(),
        rec.commit_sha.clone(),
        rec.digest.clone(),
    );
    if let Some(hit) = lib.checked.lock().ok().and_then(|c| c.get(&key).cloned()) {
        return hit;
    }
    let files = match verified_files(&dir, &rec.digest) {
        Ok(files) => files,
        Err(VerifyError::Unreadable | VerifyError::Mismatch) => {
            let name = read_manifest(&dir).map_or_else(|| rec.activity_id.clone(), |m| m.name);
            return Evaluated {
                name,
                version: rec.manifest_version.clone(),
                outcome: Err(Problem::Tampered),
            };
        }
    };
    let Some(manifest) = verified_manifest(&files).filter(|m| {
        m.id == rec.activity_id && m.kind == rec.kind.as_str() && m.engine == rec.engine
    }) else {
        return fallback(Err(Problem::Tampered));
    };
    let name = manifest.name.clone();
    let version = manifest.version.clone();
    let role = match rec.kind {
        ActivityKind::App => Ok(ServedRole::App),
        ActivityKind::Content => pack_role(&manifest, &files),
    };
    let outcome = role.map(|role| {
        let verified: BTreeMap<String, Vec<u8>> = match role {
            ServedRole::App => files.into_iter().collect(),
            ServedRole::Pack { .. } => files
                .into_iter()
                .filter(|(p, _)| {
                    manifest.preview.as_deref() == Some(p.as_str()) || manifest.images.contains(p)
                })
                .collect(),
        };
        Arc::new(Served {
            manifest,
            verified,
            commit: rec.commit_sha.clone(),
            marketplace_id: rec.marketplace_id.clone(),
            role,
        })
    });
    let evaluated = Evaluated {
        name,
        version,
        outcome,
    };
    if let Ok(mut cache) = lib.checked.lock() {
        cache.insert(key, evaluated.clone());
    }
    evaluated
}

/// A pack's engine (must be bundled and usable) and its data, checked against the engine's
/// schema exactly as for a bundled pack.
fn pack_role(manifest: &Manifest, files: &[(String, Vec<u8>)]) -> Result<ServedRole, Problem> {
    let reference = manifest.engine.as_deref().unwrap_or_default();
    let (engine, schema) = find_engine(reference).map_err(|_| Problem::MissingEngine)?;
    let data_path = manifest.data.as_deref().unwrap_or_default();
    let bytes = files
        .iter()
        .find(|(p, b)| p == data_path && b.len() <= DATA_MAX + 1)
        .map(|(_, b)| b.as_slice())
        .ok_or(Problem::MissingEngine)?;
    let data = validate_data(schema, data_path, bytes).map_err(|_| Problem::MissingEngine)?;
    // Pictures were checked at install; check them again on the bytes that were hashed, so a
    // pack installed by an older release or under older rules can't serve one that fails now.
    check_images(manifest, &data, |path| {
        files
            .iter()
            .find(|(p, _)| p == path)
            .map(|(_, b)| b.as_slice())
    })
    .map_err(|_| Problem::MissingEngine)?;
    Ok(ServedRole::Pack { engine, data })
}

fn build(core: &Core, lib: &Library) -> Result<Snapshot, String> {
    let (disabled, records) = {
        let guard = core.store.lock().map_err(|_| "internal")?;
        let store = guard.as_ref().ok_or("store_unavailable")?;
        let disabled = store.disabled_builtins().map_err(|_| "store_unavailable")?;
        let records = store
            .installed_activities()
            .map_err(|_| "store_unavailable")?;
        (disabled, records)
    };
    let disabled: HashSet<String> = disabled.into_iter().collect();
    let mut served = BTreeMap::new();
    let mut manage: Vec<ManageItem> = catalog()
        .values()
        .filter(|a| a.listed)
        .map(|a| ManageItem {
            activity_id: a.manifest.id.clone(),
            name: a.manifest.name.clone(),
            kind: if matches!(a.role, Role::Pack) {
                "content"
            } else {
                "app"
            },
            builtin: true,
            marketplace_id: BUILTIN_ID.to_owned(),
            enabled: !disabled.contains(&a.manifest.id),
            commit: None,
            version: a.manifest.version.clone(),
            problem: None,
        })
        .collect();
    for rec in &records {
        let shadowed = catalog_has(&rec.activity_id);
        let evaluated = evaluate(core, lib, rec);
        let problem = evaluated.outcome.as_ref().err().copied();
        let mut enabled = rec.enabled && !shadowed;
        // A copy that changed on disk stops running now and stays off until it verifies again.
        if problem == Some(Problem::Tampered)
            && rec.enabled
            && let Ok(guard) = core.store.lock()
            && let Some(store) = guard.as_ref()
        {
            let _ = store.set_installed_enabled(&rec.activity_id, false, OffsetDateTime::now_utc());
            enabled = false;
        }
        if enabled && let Ok(s) = &evaluated.outcome {
            served.insert(rec.activity_id.clone(), Arc::clone(s));
        }
        manage.push(ManageItem {
            activity_id: rec.activity_id.clone(),
            name: evaluated.name,
            kind: rec.kind.as_str(),
            builtin: false,
            marketplace_id: rec.marketplace_id.clone(),
            enabled,
            commit: Some(rec.commit_sha.clone()),
            version: evaluated.version,
            problem: problem.map(Problem::as_str),
        });
    }
    Ok(Snapshot {
        disabled,
        served,
        manage,
    })
}

/// Verifies one installed activity now, for "enable": `Ok` only if it can run.
pub(crate) fn verify_for_enable(core: &Core, rec: &InstalledActivity) -> Result<(), Problem> {
    if catalog_has(&rec.activity_id) {
        return Err(Problem::Tampered);
    }
    evaluate(core, &core.library, rec).outcome.map(|_| ())
}

/// Whether a manifest declares the storage permission.
pub(crate) fn declares_storage(m: &Manifest) -> bool {
    m.permissions.iter().any(|p| p == "activity.storage")
}

impl Served {
    /// Whether the activity may use `activity.storage`: a pack needs it from both the pack and
    /// its engine.
    pub(crate) fn has_storage(&self) -> bool {
        match &self.role {
            ServedRole::App => declares_storage(&self.manifest),
            ServedRole::Pack { engine, .. } => {
                declares_storage(&self.manifest) && declares_storage(&engine.manifest)
            }
        }
    }

    /// The bytes and content type of a file this activity serves at `/<id>/<file>`. A pack
    /// serves its engine's files, plus the pictures its manifest lists under `images/` (the
    /// checked, hashed bytes; never an engine file there); an app serves regular files from its
    /// install folder.
    pub(crate) fn file(
        &self,
        file: &str,
        content_type: &'static str,
    ) -> Option<(std::borrow::Cow<'static, [u8]>, &'static str)> {
        match &self.role {
            ServedRole::Pack { .. } if file.starts_with(pack_media::IMAGE_DIR) => {
                pack_image(&self.manifest, file, |p| {
                    self.verified
                        .get(p)
                        .map(|b| std::borrow::Cow::Owned(b.clone()))
                })
            }
            ServedRole::Pack { engine, .. } => engine
                .dir
                .get_file(engine.dir.path().join(file))
                .map(|f| (std::borrow::Cow::Borrowed(f.contents()), content_type)),
            ServedRole::App => self
                .verified
                .get(file)
                .filter(|b| b.len() <= FILE_MAX)
                .map(|b| (std::borrow::Cow::Owned(b.clone()), content_type)),
        }
    }

    /// The preview image bytes, if the manifest names one (always the activity's own file).
    pub(crate) fn preview_bytes(&self, max: usize) -> Option<Vec<u8>> {
        self.verified
            .get(self.manifest.preview.as_deref()?)
            .filter(|b| b.len() <= max)
            .cloned()
    }
}
