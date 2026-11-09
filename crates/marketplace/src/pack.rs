//! Reading and strictly validating an activity folder from a pinned commit, before anything is
//! written to disk (ADR-015, TECHNICAL_ARCHITECTURE 7.3 steps 4 to 6).

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::data_schema;
use crate::error::{Error, PackError, PathError, show};
use crate::git::Git;
use crate::manifest::{Manifest, parse_manifest, valid_id};
use crate::repo::{ResolvedSource, TreeEntry, hex};
use crate::validate::check_segment;

pub const MAX_FILES: usize = 512;
pub const MAX_FILE_BYTES: u64 = 5 * 1024 * 1024;
pub const MAX_PACK_BYTES: u64 = 20 * 1024 * 1024;
/// Largest engine data file (same as the host's check of bundled packs).
pub const DATA_MAX: u64 = 256 * 1024;
/// Largest preview image.
pub const PREVIEW_MAX: u64 = 256 * 1024;
const MANIFEST_MAX: u64 = 128 * 1024;
const MAX_DEPTH: usize = 8;

/// Permissions this host knows. Anything else is refused.
pub const KNOWN_PERMISSIONS: &[&str] = &["activity.storage"];

/// Top-level manifest fields (`reference/ACTIVITY_MANIFEST_V1.schema.json`).
const MANIFEST_FIELDS: &[&str] = &[
    "schemaVersion",
    "kind",
    "id",
    "version",
    "name",
    "description",
    "author",
    "license",
    "category",
    "tags",
    "minHostVersion",
    "sdkVersion",
    "permissions",
    "offline",
    "entrypoint",
    "runtime",
    "interruptibility",
    "engine",
    "data",
    "images",
    "help",
    "preview",
];

/// File extensions allowed beside the manifest in a code activity.
const APP_EXTENSIONS: &[&str] = &[
    "html", "js", "css", "json", "svg", "png", "jpg", "jpeg", "webp", "md", "txt", "woff2",
];
/// File extensions allowed as extras in a content pack (besides manifest, data and preview).
const PACK_EXTENSIONS: &[&str] = &["md", "txt"];
const PREVIEW_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "svg"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    App,
    Content,
}

/// An engine this host provides: `id@major` and its data schema (already read by the host).
#[derive(Debug, Clone)]
pub struct AvailableEngine {
    pub reference: String,
    pub schema: Value,
}

/// What `prepare_pack` needs to know about the request.
#[derive(Debug, Clone, Copy)]
pub struct PrepareRequest<'a> {
    /// The marketplace's name, shown in the review.
    pub marketplace: &'a str,
    /// True only for the built-in marketplace.
    pub builtin: bool,
    /// The id the index lists; the manifest must say the same.
    pub entry_id: &'a str,
    pub engines: &'a [AvailableEngine],
}

/// One validated file of a pack. `path` is canonical and `/`-separated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackFile {
    pub path: String,
    pub bytes: Vec<u8>,
}

/// What the install review screen shows (S08-T03).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallPreview {
    pub marketplace: String,
    /// The source repository, without userinfo.
    pub source_url: String,
    /// The pinned commit.
    pub commit: String,
    pub activity_id: String,
    pub name: String,
    pub version: String,
    pub kind: Kind,
    /// Content packs: the engine reference (`id@major`).
    pub engine: Option<String>,
    pub permissions: Vec<String>,
    pub size_bytes: u64,
    pub file_count: usize,
    /// True for every marketplace except the built-in one; the review says "unverified".
    pub third_party: bool,
    /// SHA-256 of the canonical file list, recorded at install.
    pub digest: String,
}

/// A pack that passed every check and is ready to be written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedPack {
    pub preview: InstallPreview,
    pub files: Vec<PackFile>,
}

/// A path as Git listed it, checked.
#[derive(Debug, Clone)]
pub(crate) struct Listed {
    pub path: String,
    pub oid: String,
    pub size: u64,
}

/// Checks a recursive tree listing before any blob is read: only regular non-executable files,
/// canonical paths, no collisions, and the count and size limits.
pub(crate) fn check_listing(entries: &[TreeEntry]) -> Result<Vec<Listed>, PackError> {
    if entries.len() > MAX_FILES {
        return Err(PackError::TooManyFiles);
    }
    let mut seen: BTreeMap<String, ()> = BTreeMap::new();
    let mut total: u64 = 0;
    let mut out = Vec::with_capacity(entries.len());
    for e in entries {
        let shown = show(&e.path);
        match (e.mode.as_str(), e.kind.as_str()) {
            ("100644", "blob") => {}
            ("120000", _) => return Err(PackError::Symlink(shown)),
            ("160000", _) | (_, "commit") => return Err(PackError::Submodule(shown)),
            ("100755", _) => return Err(PackError::Executable(shown)),
            _ => return Err(PackError::UnsupportedMode(shown)),
        }
        let segments: Vec<&str> = e.path.split('/').collect();
        if segments.len() > MAX_DEPTH {
            return Err(PackError::BadPath(shown, PathError::TooDeep));
        }
        for seg in &segments {
            check_segment(seg).map_err(|why| PackError::BadPath(shown.clone(), why))?;
        }
        // Names are ASCII after the checks above, so a lowercase key also catches names that a
        // case-insensitive file system (Windows, macOS) would merge. Unicode normalization
        // collisions can't occur: any non-ASCII name was already refused.
        if seen.insert(e.path.to_ascii_lowercase(), ()).is_some() {
            return Err(PackError::BadPath(shown, PathError::CaseCollision));
        }
        if e.size > MAX_FILE_BYTES {
            return Err(PackError::FileTooLarge(shown));
        }
        total += e.size;
        if total > MAX_PACK_BYTES {
            return Err(PackError::PackTooLarge);
        }
        out.push(Listed {
            path: e.path.clone(),
            oid: e.oid.clone(),
            size: e.size,
        });
    }
    // A file and a folder with the same name can't both be written.
    let keys: Vec<&String> = seen.keys().collect();
    for k in &keys {
        let dir = format!("{k}/");
        if keys.iter().any(|other| other.starts_with(&dir)) {
            return Err(PackError::BadPath(show(k), PathError::CaseCollision));
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// SHA-256 over the canonical file list: files sorted by path, each as path, length and the
/// SHA-256 of its bytes. Any change to any file, a missing file or an extra file changes it.
pub fn pack_digest<'a>(files: impl IntoIterator<Item = (&'a str, &'a [u8])>) -> String {
    let hashed: Vec<(&str, u64, [u8; 32])> = files
        .into_iter()
        .map(|(p, b)| (p, b.len() as u64, Sha256::digest(b).into()))
        .collect();
    digest_of_hashes(hashed.iter().map(|(p, l, h)| (*p, *l, h)))
}

/// [`pack_digest`] from each file's length and SHA-256, so a whole install can be checked
/// without keeping its bytes.
pub(crate) fn digest_of_hashes<'a>(
    files: impl IntoIterator<Item = (&'a str, u64, &'a [u8; 32])>,
) -> String {
    let mut list: Vec<(&str, u64, &[u8; 32])> = files.into_iter().collect();
    list.sort_by(|a, b| a.0.cmp(b.0));
    let mut h = Sha256::new();
    h.update(b"lullshift-pack-digest-v1\n");
    for (path, len, file_hash) in list {
        h.update(path.as_bytes());
        h.update([0]);
        h.update(len.to_string().as_bytes());
        h.update([0]);
        h.update(hex(file_hash).as_bytes());
        h.update(b"\n");
    }
    hex(&h.finalize())
}

/// Checks the fields `parse_manifest` leaves open, per the manifest schema: no unknown fields,
/// `offline: true`, known permissions, unique tags and permissions. Returns the typed manifest.
pub(crate) fn strict_manifest(bytes: &[u8]) -> Result<(Manifest, Value), PackError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| PackError::ManifestInvalid("it isn't UTF-8 text"))?;
    let value: Value = serde_json::from_str(text)
        .map_err(|_| PackError::ManifestInvalid("it isn't valid JSON"))?;
    let obj = value
        .as_object()
        .ok_or(PackError::ManifestInvalid("it isn't a JSON object"))?;
    if obj.keys().any(|k| !MANIFEST_FIELDS.contains(&k.as_str())) {
        return Err(PackError::ManifestInvalid(
            "it has a field this version doesn't know",
        ));
    }
    if obj.get("offline") != Some(&Value::Bool(true)) {
        return Err(PackError::ManifestInvalid("offline must be true"));
    }
    // Unknown permissions get their own error so the review can say which one.
    let perms = obj
        .get("permissions")
        .and_then(Value::as_array)
        .ok_or(PackError::ManifestInvalid("permissions must be a list"))?;
    let mut names: Vec<&str> = Vec::new();
    for p in perms {
        let name = p
            .as_str()
            .ok_or(PackError::ManifestInvalid("permissions must be text"))?;
        if !KNOWN_PERMISSIONS.contains(&name) {
            return Err(PackError::UnknownPermission(show(name)));
        }
        if names.contains(&name) {
            return Err(PackError::ManifestInvalid("permissions must be unique"));
        }
        names.push(name);
    }
    if let Some(tags) = obj.get("tags") {
        let ok = tags.as_array().is_some_and(|t| {
            let names: Vec<&str> = t.iter().filter_map(Value::as_str).collect();
            names.len() == t.len()
                && names.len() <= 10
                && names.iter().enumerate().all(|(i, n)| {
                    (1..=30).contains(&n.len())
                        && n.bytes()
                            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
                        && !names[..i].contains(n)
                })
        });
        if !ok {
            return Err(PackError::ManifestInvalid("tags aren't valid"));
        }
    }
    if let Some(license) = obj.get("license")
        && !license.as_str().is_some_and(|l| l.len() <= 100)
    {
        return Err(PackError::ManifestInvalid("license isn't valid"));
    }
    if obj
        .get("id")
        .and_then(Value::as_str)
        .is_some_and(crate::manifest::is_reserved_generated_id)
    {
        return Err(PackError::ReservedId);
    }
    let manifest = parse_manifest(text).ok_or(PackError::ManifestInvalid(
        "it doesn't follow the activity manifest rules",
    ))?;
    Ok((manifest, value))
}

impl Git {
    /// Reads the activity folder named by `source`, validates everything, and returns it ready
    /// to install. Writes nothing. Errors are specific and fail closed.
    pub fn prepare_pack(
        &self,
        source: &ResolvedSource,
        req: &PrepareRequest<'_>,
    ) -> Result<PreparedPack, Error> {
        let entries = self.list_tree(&source.repo, &source.commit, source.path.as_ref())?;
        let listing = check_listing(&entries)?;

        let manifest_entry = listing
            .iter()
            .find(|f| f.path == "manifest.json")
            .ok_or(PackError::MissingManifest)?;
        if manifest_entry.size > MANIFEST_MAX {
            return Err(PackError::ManifestInvalid("it is too large").into());
        }
        let manifest_bytes = self
            .read_blobs(
                &source.repo,
                &[(manifest_entry.oid.clone(), manifest_entry.size)],
            )?
            .remove(0);
        let (manifest, _) = strict_manifest(&manifest_bytes)?;

        if !valid_id(req.entry_id) || manifest.id != req.entry_id {
            return Err(PackError::IdMismatch {
                expected: show(req.entry_id),
                found: show(&manifest.id),
            }
            .into());
        }
        let third_party = !req.builtin;
        let kind = if manifest.kind == "app" {
            Kind::App
        } else {
            Kind::Content
        };
        // Third-party code is reviewed like everything else. Installing it needs the user's
        // consent, which the host checks (`gate::needs_code_consent`, owner decision 27.5).

        // An app's entrypoint (always `index.html`, checked by `parse_manifest`) must exist.
        if kind == Kind::App && !listing.iter().any(|f| f.path == "index.html") {
            return Err(PackError::DataMissing(show("index.html")).into());
        }

        // Engine (content packs): must be provided by this host, with a usable schema.
        let engine = match kind {
            Kind::Content => {
                let reference = manifest.engine.as_deref().unwrap_or_default();
                let found = req
                    .engines
                    .iter()
                    .find(|e| e.reference == reference)
                    .ok_or_else(|| PackError::EngineMissing(show(reference)))?;
                data_schema::check_schema(&found.schema)
                    .map_err(|_| PackError::EngineUnusable(show(reference)))?;
                Some(found)
            }
            Kind::App => None,
        };

        // The files the manifest names must exist (a clearer message than a file-type error).
        if let Some(data) = manifest.data.as_deref()
            && !listing.iter().any(|f| f.path == data)
        {
            return Err(PackError::DataMissing(show(data)).into());
        }

        // File types, before any other blob is read.
        for f in &listing {
            let ext = f.path.rsplit_once('.').map(|(_, e)| e);
            let ext_in = |list: &[&str]| ext.is_some_and(|e| list.contains(&e));
            let allowed = match kind {
                Kind::App => ext_in(APP_EXTENSIONS),
                Kind::Content => {
                    f.path == "manifest.json"
                        || Some(f.path.as_str()) == manifest.data.as_deref()
                        || Some(f.path.as_str()) == manifest.preview.as_deref()
                        || manifest.images.contains(&f.path)
                        || ext_in(PACK_EXTENSIONS)
                }
            };
            if !allowed {
                return Err(PackError::DisallowedFile(show(&f.path)).into());
            }
        }
        if let Some(preview) = manifest.preview.as_deref() {
            let f = listing
                .iter()
                .find(|f| f.path == preview)
                .ok_or_else(|| PackError::DataMissing(show(preview)))?;
            let ext = preview.rsplit_once('.').map(|(_, e)| e);
            if f.size > PREVIEW_MAX || !ext.is_some_and(|e| PREVIEW_EXTENSIONS.contains(&e)) {
                return Err(PackError::FileTooLarge(show(preview)).into());
            }
        }
        // Pack images (S12-T11): each declared one exists and is small enough before any blob
        // is read; the full checks run on the bytes below.
        for image in &manifest.images {
            let f = listing
                .iter()
                .find(|f| &f.path == image)
                .ok_or_else(|| PackError::DataMissing(show(image)))?;
            if f.size > pack_media::MAX_IMAGE_BYTES as u64 {
                return Err(PackError::ImageInvalid(format!(
                    "{} {}",
                    show(image),
                    pack_media::ImageError::TooLarge
                ))
                .into());
            }
        }

        let wanted: Vec<(String, u64)> = listing.iter().map(|f| (f.oid.clone(), f.size)).collect();
        let blobs = self.read_blobs(&source.repo, &wanted)?;
        let files: Vec<PackFile> = listing
            .iter()
            .zip(blobs)
            .map(|(f, bytes)| PackFile {
                path: f.path.clone(),
                bytes,
            })
            .collect();

        // Every image is fully checked (container walk, size from the header, full decode)
        // before the pack can be installed. The same check runs when the app loads it.
        for image in &manifest.images {
            if let Some(file) = files.iter().find(|f| &f.path == image) {
                pack_media::check_image(image, &file.bytes).map_err(|why| {
                    PackError::ImageInvalid(format!("{} {}", show(image), show(&why.to_string())))
                })?;
            }
        }

        if let (Some(engine), Some(data_path)) = (engine, manifest.data.as_deref()) {
            let file = files
                .iter()
                .find(|f| f.path == data_path)
                .ok_or_else(|| PackError::DataMissing(show(data_path)))?;
            if file.bytes.len() as u64 > DATA_MAX {
                return Err(PackError::DataTooLarge(show(data_path)).into());
            }
            let data: Value = serde_json::from_slice(&file.bytes)
                .map_err(|_| PackError::DataNotJson(show(data_path)))?;
            // The schema, then the host's plain-text rule on every string (security review
            // LOW-2: the long-mark-run half can't be a schema pattern the app compiles).
            let problems = crate::pack_text::pack_data_problems(&engine.schema, &data);
            if !problems.is_empty() {
                let shown = problems
                    .iter()
                    .take(3)
                    .map(|p| format!("{}: {}", show(data_path), show(p)))
                    .collect();
                return Err(PackError::DataInvalid(shown).into());
            }
            pack_media::check_declared(&manifest.images, &data)
                .map_err(|why| PackError::ImageInvalid(show(&why)))?;
        }

        let size_bytes = files.iter().map(|f| f.bytes.len() as u64).sum();
        let digest = pack_digest(files.iter().map(|f| (f.path.as_str(), f.bytes.as_slice())));
        Ok(PreparedPack {
            preview: InstallPreview {
                marketplace: req.marketplace.to_owned(),
                source_url: source.source_url.clone(),
                commit: source.commit.to_string(),
                activity_id: manifest.id.clone(),
                name: manifest.name.clone(),
                version: manifest.version.clone(),
                kind,
                engine: manifest.engine.clone(),
                permissions: manifest.permissions.clone(),
                size_bytes,
                file_count: files.len(),
                third_party,
                digest,
            },
            files,
        })
    }
}
