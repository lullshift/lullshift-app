//! Installed (marketplace) activities, served beside the bundled ones (Sprint 08, ADR-015).
//!
//! The library keeps one in-memory [`Snapshot`] of what may be shown and served:
//!
//! - which bundled activities the user disabled,
//! - which installed activities are enabled, still match their install digest and can run on a
//!   bundled engine,
//! - a [`ManageItem`] for every bundled and installed activity, disabled or broken ones included.
//!
//! An installed activity is never read from the marketplace cache: only from
//! `<app data>/installed/<id>/<commit>/`. Building a snapshot reads and hashes every installed
//! file against the digest recorded at install and keeps only the per-file hashes, never file
//! content (decision 18.5). Every later use (the manifest, a pack's data, its preview, an app's
//! files) reads the file again and hashes it again; bytes that no longer match are never served,
//! the activity is turned off and shown as damaged (`tampered`), and the snapshot is rebuilt. A
//! file that is there but can't be read right now (a scanner holds it, say) only refuses that
//! use; nothing is turned off and the next use tries again (`unreadable`).

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use local_store::{ActivityKind, InstalledActivity};
use marketplace::manifest::{Manifest, parse_manifest};
use marketplace::{VerifiedHashes, VerifyError, install_dir, verified_hashes};
use serde::Serialize;
use serde_json::Value;
use time::OffsetDateTime;

use super::{
    Activity, ContentError, DATA_MAX, FoundFile, Role, catalog, check_images, find_engine,
    listed_image_type, validate_data,
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
    /// A sandboxed web app with its own files in `dir`: code from a third-party marketplace,
    /// installed after the user accepted the risk (owner decision 27.5). Served exactly like a
    /// bundled app (same scheme, CSP and headers), with every file re-hashed on each read.
    App,
    /// A content pack: the bundled engine's files, plus the pack's data file, read, re-hashed
    /// and checked against the engine's schema on every load.
    Pack {
        engine: &'static Activity,
        data_path: String,
    },
}

/// One installed activity that may be shown and served.
pub(crate) struct Served {
    pub manifest: Manifest,
    /// The install folder, and the length and hash of each of its files from the snapshot's
    /// check. No file content is held (pictures included); [`VerifiedHashes::read`] reads and
    /// hashes again on every use.
    dir: PathBuf,
    hashes: VerifiedHashes,
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
    /// A file is there but couldn't be read during this check (another program holds it, for
    /// example). Not proof of a change: the activity isn't turned off, and the next check tries
    /// again (security review INFO-1).
    Unreadable,
}

impl Problem {
    fn as_str(self) -> &'static str {
        match self {
            Self::Tampered => "tampered",
            Self::MissingEngine => "missingEngine",
            Self::Unreadable => "unreadable",
        }
    }

    /// Only a mismatch is tampering; a read that failed is retried.
    pub(crate) fn from_verify(e: VerifyError) -> Self {
        match e {
            VerifyError::Mismatch => Self::Tampered,
            VerifyError::Unreadable => Self::Unreadable,
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
    /// `tampered`, `missingEngine`, `unreadable` (couldn't be read just now; retried on the next
    /// check) or null.
    pub problem: Option<&'static str>,
    /// `notFoundAtNewAddress` when the marketplace moved and this install's pinned commit wasn't
    /// at the new address (decision 18.4); it still runs as installed. Null otherwise.
    pub source_note: Option<&'static str>,
}

/// Everything the host needs to answer list, info, content and serve requests.
pub(crate) struct Snapshot {
    /// Bundled activities the user turned off.
    pub disabled: HashSet<String>,
    /// Installed activities that are enabled, verified and runnable, by id.
    pub served: BTreeMap<String, Arc<Served>>,
    pub manage: Vec<ManageItem>,
    /// Some install couldn't be read during this build. Such a snapshot is used once and not
    /// kept, so the next use checks again instead of hiding the activity for the session.
    pub retry: bool,
}

/// A checked install.
struct Evaluated {
    name: String,
    version: String,
    outcome: Result<Arc<Served>, Problem>,
}

/// The session state: the current snapshot. Nothing else is remembered between builds.
#[derive(Default)]
pub(crate) struct Library {
    /// Bumped by every invalidation, so a snapshot built from older data is never kept.
    generation: AtomicU64,
    snapshot: RwLock<Option<Arc<Snapshot>>>,
}

impl Library {
    /// The current snapshot, built on first use.
    pub(crate) fn snapshot(&self, core: &Core) -> Result<Arc<Snapshot>, String> {
        if let Some(s) = self.peek() {
            return Ok(s);
        }
        let started = self.generation.load(Ordering::SeqCst);
        let built = Arc::new(build(core)?);
        if !built.retry
            && self.generation.load(Ordering::SeqCst) == started
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

    /// Drops the snapshot, so the next use rebuilds it and checks every install again (install,
    /// update, uninstall, enable, disable, and a file found damaged while serving).
    pub(crate) fn invalidate(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut slot) = self.snapshot.write() {
            *slot = None;
        }
    }
}

/// A file of an installed activity no longer matches what was checked: it is turned off in the
/// store and the snapshot is dropped, so the next build shows it as damaged (`tampered`) until
/// it is reinstalled. Only for [`VerifyError::Mismatch`]: a file that couldn't be read
/// ([`VerifyError::Unreadable`]) just refuses that one use (security review INFO-1).
pub(crate) fn mark_damaged(core: &Core, activity_id: &str) {
    if let Ok(guard) = core.store.lock()
        && let Some(store) = guard.as_ref()
    {
        let _ = store.set_installed_enabled(activity_id, false, OffsetDateTime::now_utc());
    }
    core.library.invalidate();
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

/// The manifest, read and hashed again against the check.
fn verified_manifest(dir: &Path, hashes: &VerifiedHashes) -> Result<Option<Manifest>, Problem> {
    let Some(bytes) = hashes
        .read(dir, "manifest.json", MANIFEST_MAX)
        .map_err(Problem::from_verify)?
    else {
        return Ok(None);
    };
    Ok(std::str::from_utf8(&bytes).ok().and_then(parse_manifest))
}

fn read_manifest(dir: &Path) -> Option<Manifest> {
    let bytes = read_regular(dir, "manifest.json", MANIFEST_MAX)?;
    parse_manifest(std::str::from_utf8(&bytes).ok()?)
}

/// Whether a record may run as far as consent goes: packs and built-in code need none;
/// third-party code needs the consent recorded for its own pinned commit.
pub(crate) fn has_code_consent(rec: &InstalledActivity) -> bool {
    let third_party_code = rec.kind == ActivityKind::App && rec.marketplace_id != BUILTIN_ID;
    !third_party_code
        || rec
            .code_consent
            .as_ref()
            .is_some_and(|c| c.commit == rec.commit_sha)
}

/// Checks one install: digest, manifest, and for a pack its engine and data. Reads every file.
fn evaluate(core: &Core, rec: &InstalledActivity) -> Evaluated {
    let fallback = |outcome| Evaluated {
        name: rec.activity_id.clone(),
        version: rec.manifest_version.clone(),
        outcome,
    };
    // Third-party code runs only if the user accepted the risk for exactly this commit (owner
    // decision 27.5). The install and update commands always record that; a record without it
    // was not made by them, so it is not something to run.
    if !has_code_consent(rec) {
        return fallback(Err(Problem::Tampered));
    }
    let Ok(dir) = install_dir(&install_root(core), &rec.activity_id, &rec.commit_sha) else {
        return fallback(Err(Problem::Tampered));
    };
    let hashes = match verified_hashes(&dir, &rec.digest) {
        Ok(hashes) => hashes,
        Err(e) => {
            let name = read_manifest(&dir).map_or_else(|| rec.activity_id.clone(), |m| m.name);
            return Evaluated {
                name,
                version: rec.manifest_version.clone(),
                outcome: Err(Problem::from_verify(e)),
            };
        }
    };
    let manifest = match verified_manifest(&dir, &hashes) {
        Ok(found) => found.filter(|m| {
            m.id == rec.activity_id && m.kind == rec.kind.as_str() && m.engine == rec.engine
        }),
        Err(problem) => return fallback(Err(problem)),
    };
    let Some(manifest) = manifest else {
        return fallback(Err(Problem::Tampered));
    };
    let name = manifest.name.clone();
    let version = manifest.version.clone();
    let role = match rec.kind {
        ActivityKind::App => Ok(ServedRole::App),
        ActivityKind::Content => pack_role(&manifest, &dir, &hashes),
    };
    let outcome = role.map(|role| {
        Arc::new(Served {
            manifest,
            dir,
            hashes,
            commit: rec.commit_sha.clone(),
            marketplace_id: rec.marketplace_id.clone(),
            role,
        })
    });
    Evaluated {
        name,
        version,
        outcome,
    }
}

/// A pack's engine (must be bundled and usable) and its data, checked against the engine's
/// schema exactly as for a bundled pack.
fn pack_role(
    manifest: &Manifest,
    dir: &Path,
    hashes: &VerifiedHashes,
) -> Result<ServedRole, Problem> {
    let reference = manifest.engine.as_deref().unwrap_or_default();
    let (engine, schema) = find_engine(reference).map_err(|_| Problem::MissingEngine)?;
    let data_path = manifest.data.as_deref().unwrap_or_default();
    let bytes = hashes
        .read(dir, data_path, DATA_MAX + 1)
        .map_err(Problem::from_verify)?
        .ok_or(Problem::MissingEngine)?;
    let data = validate_data(schema, data_path, &bytes).map_err(|_| Problem::MissingEngine)?;
    // Pictures were checked at install; check them again here, so a pack installed by an older
    // release or under older rules can't serve one that fails now. They are read and hashed
    // like every other file, and read and hashed again each time one is served.
    let mut pictures: Vec<(String, Vec<u8>)> = Vec::with_capacity(manifest.images.len());
    for path in &manifest.images {
        if let Some(b) = hashes
            .read(dir, path, FILE_MAX)
            .map_err(Problem::from_verify)?
        {
            pictures.push((path.clone(), b));
        }
    }
    check_images(manifest, &data, |path| {
        pictures
            .iter()
            .find(|(p, _)| p == path)
            .map(|(_, b)| b.as_slice())
    })
    .map_err(|_| Problem::MissingEngine)?;
    Ok(ServedRole::Pack {
        engine,
        data_path: data_path.to_owned(),
    })
}

fn build(core: &Core) -> Result<Snapshot, String> {
    let (disabled, records, notes) = {
        let guard = core.store.lock().map_err(|_| "internal")?;
        let store = guard.as_ref().ok_or("store_unavailable")?;
        let disabled = store.disabled_builtins().map_err(|_| "store_unavailable")?;
        let records = store
            .installed_activities()
            .map_err(|_| "store_unavailable")?;
        let notes = store
            .install_source_notes()
            .map_err(|_| "store_unavailable")?;
        (disabled, records, notes)
    };
    let note_of = |id: &str| {
        notes
            .iter()
            .find(|(a, n)| a == id && n == "notFoundAtNewAddress")
            .map(|_| "notFoundAtNewAddress")
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
            source_note: None,
        })
        .collect();
    for rec in &records {
        let shadowed = catalog_has(&rec.activity_id);
        let evaluated = evaluate(core, rec);
        let problem = evaluated.outcome.as_ref().err().copied();
        let mut enabled = rec.enabled && !shadowed;
        // A copy that changed on disk stops running now and stays off until it verifies again.
        // One that only couldn't be read keeps its enabled choice; it isn't served this time.
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
            source_note: note_of(&rec.activity_id),
        });
    }
    let retry = manage.iter().any(|m| m.problem == Some("unreadable"));
    Ok(Snapshot {
        disabled,
        served,
        manage,
        retry,
    })
}

/// Verifies one installed activity now, for "enable": `Ok` only if it can run.
pub(crate) fn verify_for_enable(core: &Core, rec: &InstalledActivity) -> Result<(), Problem> {
    if catalog_has(&rec.activity_id) {
        return Err(Problem::Tampered);
    }
    evaluate(core, rec).outcome.map(|_| ())
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
    /// serves its engine's files, plus the pictures its manifest lists under `images/` (never an
    /// engine file there); an app serves its own install files. Installed files (pictures
    /// included) are read and hashed again now. `Err` means one no longer matches its install
    /// check: serve nothing and [`mark_damaged`].
    pub(crate) fn file(
        &self,
        file: &str,
        content_type: &'static str,
    ) -> Result<Option<FoundFile>, VerifyError> {
        match &self.role {
            ServedRole::Pack { .. } if file.starts_with(pack_media::IMAGE_DIR) => {
                let Some(image_type) = listed_image_type(&self.manifest, file) else {
                    return Ok(None);
                };
                Ok(self
                    .hashes
                    .read(&self.dir, file, FILE_MAX)?
                    .map(|b| (std::borrow::Cow::Owned(b), image_type)))
            }
            ServedRole::Pack { engine, .. } => Ok(engine
                .dir
                .get_file(engine.dir.path().join(file))
                .map(|f| (std::borrow::Cow::Borrowed(f.contents()), content_type))),
            ServedRole::App => Ok(self
                .hashes
                .read(&self.dir, file, FILE_MAX)?
                .map(|b| (std::borrow::Cow::Owned(b), content_type))),
        }
    }

    /// The preview image bytes, if the manifest names one (always the activity's own file),
    /// read and hashed again now.
    pub(crate) fn preview_bytes(&self, max: usize) -> Result<Option<Vec<u8>>, VerifyError> {
        match self.manifest.preview.as_deref() {
            Some(rel) => self.hashes.read(&self.dir, rel, max),
            None => Ok(None),
        }
    }

    /// What the activity gets as `init.content`: null for an app; for a pack, its data file read
    /// and hashed again now, then checked against its engine's schema. The outer `Err` means the
    /// file no longer matches the install check.
    pub(crate) fn pack_data(&self) -> Result<Result<Value, ContentError>, VerifyError> {
        let ServedRole::Pack { data_path, .. } = &self.role else {
            return Ok(Ok(Value::Null));
        };
        let Some(bytes) = self.hashes.read(&self.dir, data_path, DATA_MAX + 1)? else {
            return Ok(Err(ContentError::new(
                "data_missing",
                format!("{data_path} is missing"),
            )));
        };
        let reference = self.manifest.engine.as_deref().unwrap_or_default();
        Ok(find_engine(reference).and_then(|(_, schema)| validate_data(schema, data_path, &bytes)))
    }
}
