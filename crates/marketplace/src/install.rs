//! Writing a prepared pack into the install root, re-verifying it later, and describing updates
//! (ADR-015). Installs live at `<install root>/<activity id>/<commit>/` and are never run from
//! the cache.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::error::{Error, PackError};
use crate::manifest::valid_id;
use crate::pack::{InstallPreview, Kind, MAX_FILES, PreparedPack, digest_of_hashes, pack_digest};
use crate::validate::{Commit, windows_reserved};

/// A pack written to disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub dir: PathBuf,
    pub digest: String,
}

/// Why a stored install no longer matches what was approved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyError {
    /// The folder is missing or contains something that isn't a plain file or folder.
    Unreadable,
    /// The files differ from the digest recorded at install.
    Mismatch,
}

impl std::fmt::Display for VerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Unreadable => "the installed activity can't be read",
            Self::Mismatch => "the installed activity's files have changed since it was installed",
        })
    }
}

impl std::error::Error for VerifyError {}

/// `<install root>/<activity id>/<commit>`. The id and commit are validated, so the result
/// can't leave the install root.
pub fn install_dir(root: &Path, activity_id: &str, commit: &str) -> Result<PathBuf, Error> {
    let first = activity_id.split('.').next().unwrap_or_default();
    // `con.something` is a device name on Windows even with an extension.
    if !valid_id(activity_id) || windows_reserved(first) || Commit::parse(commit).is_none() {
        return Err(PackError::Unreadable.into());
    }
    Ok(root.join(activity_id).join(commit))
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Writes `pack` to `<root>/<id>/<commit>/` through a temporary sibling folder and an atomic
/// rename. If that folder already exists and still matches the digest, it is returned as is;
/// if it exists and differs, nothing is changed and an error is returned.
pub fn install(root: &Path, pack: &PreparedPack) -> Result<Installed, Error> {
    let p = &pack.preview;
    let target = install_dir(root, &p.activity_id, &p.commit)?;
    // Re-derive the digest from the bytes about to be written; never trust the stored field.
    let digest = pack_digest(
        pack.files
            .iter()
            .map(|f| (f.path.as_str(), f.bytes.as_slice())),
    );
    if digest != p.digest || pack.files.len() > MAX_FILES {
        return Err(PackError::Inconsistent.into());
    }
    if target.exists() {
        return match verify_install(&target, &digest) {
            Ok(()) => Ok(Installed {
                dir: target,
                digest,
            }),
            Err(_) => Err(Error::Io {
                what: "install because a different copy is already there",
                kind: std::io::ErrorKind::AlreadyExists,
            }),
        };
    }
    let parent = target.parent().ok_or(PackError::Unreadable)?;
    fs::create_dir_all(parent).map_err(Error::io("create the install folder"))?;
    let tmp = parent.join(format!(
        ".tmp-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let result = write_all(&tmp, pack).and_then(|()| {
        // The digest of what actually landed on disk must match before it becomes visible.
        verify_install(&tmp, &digest).map_err(|_| Error::Pack(PackError::Inconsistent))?;
        fs::rename(&tmp, &target).map_err(Error::io("move the activity into place"))
    });
    if let Err(e) = result {
        let _ = fs::remove_dir_all(&tmp);
        return Err(e);
    }
    Ok(Installed {
        dir: target,
        digest,
    })
}

fn write_all(tmp: &Path, pack: &PreparedPack) -> Result<(), Error> {
    fs::create_dir(tmp).map_err(Error::io("create a temporary folder"))?;
    for f in &pack.files {
        let dest = tmp.join(&f.path);
        if let Some(dir) = dest.parent() {
            fs::create_dir_all(dir).map_err(Error::io("create a folder"))?;
        }
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&dest)
            .map_err(Error::io("write a file"))?;
        file.write_all(&f.bytes)
            .map_err(Error::io("write a file"))?;
        file.sync_all().map_err(Error::io("write a file"))?;
    }
    Ok(())
}

/// Recomputes the digest of an installed folder and compares it with `expected`. Anything that
/// isn't a plain file or folder (links, devices) makes it unreadable; extra, missing or changed
/// files make it a mismatch. Serve an installed activity only when this is `Ok`.
pub fn verify_install(dir: &Path, expected: &str) -> Result<(), VerifyError> {
    verified_hashes(dir, expected).map(|_| ())
}

/// The length and SHA-256 of one installed file, taken while the whole folder matched the digest
/// recorded at install.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FileHash {
    len: u64,
    sha256: [u8; 32],
}

/// Every file of a verified install by relative path, with its length and hash. Holds no file
/// content (decision 18.5): each file is read again, and hashed again, every time it is used,
/// and only bytes that still match are returned.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VerifiedHashes(BTreeMap<String, FileHash>);

impl VerifiedHashes {
    /// True if `rel` is one of the install's files.
    pub fn contains(&self, rel: &str) -> bool {
        self.0.contains_key(rel)
    }

    /// Reads `rel` from `dir` now and returns its bytes only if they still have the length and
    /// hash recorded at verification. `Ok(None)`: not a file of this install, or larger than
    /// `max`. `Err(Mismatch)`: it changed (or became a link or folder); `Err(Unreadable)`: it
    /// can't be read. The bytes returned are the bytes hashed, never a second read.
    pub fn read(&self, dir: &Path, rel: &str, max: usize) -> Result<Option<Vec<u8>>, VerifyError> {
        let Some(want) = self.0.get(rel) else {
            return Ok(None);
        };
        if usize::try_from(want.len).map_or(true, |l| l > max) {
            return Ok(None);
        }
        let path = dir.join(rel);
        // symlink_metadata: a link put in its place is never followed.
        let meta = fs::symlink_metadata(&path).map_err(|_| VerifyError::Unreadable)?;
        if !meta.is_file() || meta.len() != want.len {
            return Err(VerifyError::Mismatch);
        }
        let mut bytes = Vec::with_capacity(usize::try_from(want.len).unwrap_or(0));
        fs::File::open(&path)
            .and_then(|f| f.take(want.len.saturating_add(1)).read_to_end(&mut bytes))
            .map_err(|_| VerifyError::Unreadable)?;
        let got: [u8; 32] = Sha256::digest(&bytes).into();
        if bytes.len() as u64 != want.len || got != want.sha256 {
            return Err(VerifyError::Mismatch);
        }
        Ok(Some(bytes))
    }
}

/// Reads and hashes every file of an installed folder, one at a time, and returns their hashes
/// if the folder matches `expected` (the digest recorded at install). No file content is kept.
pub fn verified_hashes(dir: &Path, expected: &str) -> Result<VerifiedHashes, VerifyError> {
    let mut files: BTreeMap<String, FileHash> = BTreeMap::new();
    let mut total = 0_u64;
    collect(dir, "", &mut files, &mut total, 0)?;
    let digest = digest_of_hashes(files.iter().map(|(p, h)| (p.as_str(), h.len, &h.sha256)));
    if digest == expected {
        Ok(VerifiedHashes(files))
    } else {
        Err(VerifyError::Mismatch)
    }
}

fn collect(
    dir: &Path,
    prefix: &str,
    out: &mut BTreeMap<String, FileHash>,
    total: &mut u64,
    depth: usize,
) -> Result<(), VerifyError> {
    if depth > 8 {
        return Err(VerifyError::Mismatch);
    }
    let read = fs::read_dir(dir).map_err(|_| VerifyError::Unreadable)?;
    for entry in read {
        let entry = entry.map_err(|_| VerifyError::Unreadable)?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| VerifyError::Mismatch)?;
        // symlink_metadata: a link is never followed.
        let meta = fs::symlink_metadata(entry.path()).map_err(|_| VerifyError::Unreadable)?;
        let rel = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        if meta.is_dir() {
            collect(&entry.path(), &rel, out, total, depth + 1)?;
        } else if meta.is_file() {
            if out.len() >= MAX_FILES
                || meta.len() > crate::pack::MAX_FILE_BYTES
                || *total + meta.len() > crate::pack::MAX_PACK_BYTES
            {
                return Err(VerifyError::Mismatch);
            }
            let bytes = fs::read(entry.path()).map_err(|_| VerifyError::Unreadable)?;
            *total += bytes.len() as u64;
            out.insert(
                rel,
                FileHash {
                    len: bytes.len() as u64,
                    sha256: Sha256::digest(&bytes).into(),
                },
            );
        } else {
            return Err(VerifyError::Unreadable);
        }
    }
    Ok(())
}

/// Deletes `<root>/<id>/<commit>` (and the activity folder if it is then empty). Used for
/// uninstall and for removing the old version after an update swap.
pub fn remove_install(root: &Path, activity_id: &str, commit: &str) -> Result<(), Error> {
    let dir = install_dir(root, activity_id, commit)?;
    match fs::remove_dir_all(&dir) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(Error::Io {
                what: "remove the installed activity",
                kind: e.kind(),
            });
        }
    }
    if let Some(parent) = dir.parent() {
        // Fails (and is ignored) if another commit's folder is still there.
        let _ = fs::remove_dir(parent);
    }
    Ok(())
}

/// What the app knows about the installed version, for [`diff_update`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledSummary {
    pub version: String,
    pub commit: String,
    /// The repository address the install came from (display form, no userinfo).
    pub source_url: String,
    pub kind: Kind,
    pub engine: Option<String>,
    pub permissions: Vec<String>,
}

impl From<&InstallPreview> for InstalledSummary {
    fn from(p: &InstallPreview) -> Self {
        Self {
            version: p.version.clone(),
            commit: p.commit.clone(),
            source_url: p.source_url.clone(),
            kind: p.kind,
            engine: p.engine.clone(),
            permissions: p.permissions.clone(),
        }
    }
}

/// What an update changes, for the confirmation screen (A03: a new permission needs approval).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDiff {
    pub old_version: String,
    pub new_version: String,
    pub old_commit: String,
    pub new_commit: String,
    pub added_permissions: Vec<String>,
    pub removed_permissions: Vec<String>,
    /// `(old, new)` if the kind changed.
    pub kind_change: Option<(Kind, Kind)>,
    /// `(old, new)` if the engine reference changed.
    pub engine_change: Option<(Option<String>, Option<String>)>,
    /// `(old, new)` display addresses if the update comes from a different repository than the
    /// install did (review M1).
    pub source_changed: Option<(String, String)>,
    /// True if the new commit is the one already installed.
    pub unchanged: bool,
    /// True if the user must look at this update twice: a new permission, or a different kind,
    /// engine or source. The UI makes the confirmation stronger when this is set.
    pub needs_attention: bool,
}

/// Compares an installed version with an update that was prepared (and validated) from a newer
/// commit.
pub fn diff_update(old: &InstalledSummary, new: &InstallPreview) -> UpdateDiff {
    let added: Vec<String> = new
        .permissions
        .iter()
        .filter(|p| !old.permissions.contains(p))
        .cloned()
        .collect();
    let removed: Vec<String> = old
        .permissions
        .iter()
        .filter(|p| !new.permissions.contains(p))
        .cloned()
        .collect();
    let kind_change = (old.kind != new.kind).then_some((old.kind, new.kind));
    let engine_change =
        (old.engine != new.engine).then(|| (old.engine.clone(), new.engine.clone()));
    let source_changed = (old.source_url != new.source_url)
        .then(|| (old.source_url.clone(), new.source_url.clone()));
    let needs_attention = !added.is_empty()
        || kind_change.is_some()
        || engine_change.is_some()
        || source_changed.is_some();
    UpdateDiff {
        old_version: old.version.clone(),
        new_version: new.version.clone(),
        old_commit: old.commit.clone(),
        new_commit: new.commit.clone(),
        added_permissions: added,
        removed_permissions: removed,
        kind_change,
        engine_change,
        source_changed,
        unchanged: old.commit == new.commit,
        needs_attention,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preview(
        version: &str,
        commit: &str,
        kind: Kind,
        engine: Option<&str>,
        perms: &[&str],
    ) -> InstallPreview {
        InstallPreview {
            marketplace: "m".into(),
            source_url: "https://example.com/o/r".into(),
            commit: commit.into(),
            activity_id: "a.b".into(),
            name: "n".into(),
            version: version.into(),
            kind,
            engine: engine.map(str::to_owned),
            permissions: perms.iter().map(|s| (*s).to_owned()).collect(),
            size_bytes: 1,
            file_count: 1,
            third_party: true,
            digest: String::new(),
        }
    }

    #[test]
    fn diff_reports_every_change() {
        let c1 = "1".repeat(40);
        let c2 = "2".repeat(40);
        let old = InstalledSummary::from(&preview(
            "1.0.0",
            &c1,
            Kind::Content,
            Some("x.y@1"),
            &["activity.storage"],
        ));
        let new = preview("1.1.0", &c2, Kind::Content, Some("x.y@2"), &[]);
        let d = diff_update(&old, &new);
        assert_eq!(
            (d.old_version.as_str(), d.new_version.as_str()),
            ("1.0.0", "1.1.0")
        );
        assert_eq!(
            (d.old_commit.as_str(), d.new_commit.as_str()),
            (c1.as_str(), c2.as_str())
        );
        assert_eq!(d.removed_permissions, ["activity.storage"]);
        assert!(d.added_permissions.is_empty());
        assert_eq!(
            d.engine_change,
            Some((Some("x.y@1".into()), Some("x.y@2".into())))
        );
        assert!(d.kind_change.is_none() && d.needs_attention && !d.unchanged);

        let old = InstalledSummary::from(&preview("1.0.0", &c1, Kind::Content, Some("x.y@1"), &[]));
        let new = preview(
            "1.0.0",
            &c1,
            Kind::Content,
            Some("x.y@1"),
            &["activity.storage"],
        );
        let d = diff_update(&old, &new);
        assert_eq!(d.added_permissions, ["activity.storage"]);
        assert!(d.needs_attention && d.unchanged);

        let new = preview("1.0.0", &c2, Kind::App, None, &[]);
        let d = diff_update(&old, &new);
        assert_eq!(d.kind_change, Some((Kind::Content, Kind::App)));
        assert!(d.needs_attention);

        let same = preview("1.0.1", &c2, Kind::Content, Some("x.y@1"), &[]);
        assert!(!diff_update(&old, &same).needs_attention);
    }

    #[test]
    fn a_different_source_is_reported_and_needs_attention() {
        let c1 = "1".repeat(40);
        let c2 = "2".repeat(40);
        let old = InstalledSummary::from(&preview("1.0.0", &c1, Kind::Content, None, &[]));
        let mut new = preview("1.0.1", &c2, Kind::Content, None, &[]);
        assert!(diff_update(&old, &new).source_changed.is_none());
        assert!(!diff_update(&old, &new).needs_attention);

        new.source_url = "https://evil.example/o/r".into();
        let d = diff_update(&old, &new);
        assert_eq!(
            d.source_changed,
            Some((
                "https://example.com/o/r".to_owned(),
                "https://evil.example/o/r".to_owned()
            ))
        );
        assert!(d.needs_attention, "a new source alone needs a closer look");
    }
    #[test]
    fn install_dir_refuses_unsafe_names() {
        let root = Path::new("/r");
        let c = "a".repeat(40);
        assert!(install_dir(root, "app.x.y", &c).is_ok());
        for id in [
            "..",
            "a/b.c",
            "a.b/../c",
            "CON.x",
            "nul.thing",
            "",
            "A.b",
            "a..",
        ] {
            assert!(install_dir(root, id, &c).is_err(), "{id}");
        }
        for commit in ["", "..", "main", &"a".repeat(39)] {
            assert!(install_dir(root, "app.x.y", commit).is_err(), "{commit}");
        }
    }
}
