//! Fetching marketplaces and sources into bare cache repositories, reading the index, resolving
//! entries to pinned commits, and listing and reading trees. Nothing is ever checked out: file
//! contents are read from Git objects (`ls-tree`, `cat-file`), so no repository file is written
//! by Git, `.gitattributes` filters and hooks never run, and symlinks are never created.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::{Error, GitFailureKind, IndexError, PackError, RefError};
use crate::git::{Class, Git, SMALL_OUTPUT};
use crate::index::{
    EntrySource, INDEX_MAX_BYTES, INDEX_PATH, IndexEntry, MarketplaceIndex, parse_index,
};
use crate::manifest::{Manifest, parse_manifest};
use crate::net::{check_destination, curl_resolve};
use crate::validate::{Commit, GitRef, GitUrl, RelPath, Scheme};

use std::sync::atomic::{AtomicU64, Ordering};

static STAGING_COUNTER: AtomicU64 = AtomicU64::new(0);

/// The ref the fetched commit is stored under inside a cache repository.
const FETCH_REF: &str = "refs/lullshift/fetched";

/// One line of `git ls-tree -r -l`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TreeEntry {
    pub mode: String,
    pub kind: String,
    pub oid: String,
    pub size: u64,
    pub path: String,
}

/// Where an entry's files are, pinned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSource {
    /// The bare cache repository holding `commit`.
    pub repo: PathBuf,
    /// The pinned commit.
    pub commit: Commit,
    /// The folder inside the tree; None means the repository root.
    pub path: Option<RelPath>,
    /// The repository address, without userinfo.
    pub source_url: String,
    /// True if the files come from a repository other than the marketplace's own.
    pub external: bool,
}

/// What `resolve_entry` needs to know about the marketplace.
#[derive(Debug, Clone, Copy)]
pub struct MarketplaceRef<'a> {
    /// The marketplace's bare cache repository.
    pub repo: &'a Path,
    /// The commit its index was read at.
    pub commit: &'a Commit,
    pub url: &'a GitUrl,
    /// Folder under which caches for external sources are kept (one folder per source).
    pub external_cache_root: &'a Path,
}

impl Git {
    /// Creates the bare cache repository at `repo` if it doesn't exist. No templates are
    /// copied, so it starts without hooks.
    pub fn ensure_repo(&self, repo: &Path) -> Result<(), Error> {
        if repo.join("HEAD").is_file() {
            return Ok(());
        }
        if let Some(parent) = repo.parent() {
            std::fs::create_dir_all(parent).map_err(Error::io("create the cache folder"))?;
        }
        let repo_text = repo.to_str().ok_or(Error::Io {
            what: "use the cache path because it isn't UTF-8",
            kind: std::io::ErrorKind::InvalidInput,
        })?;
        self.run_ok(
            None,
            &[
                "init",
                "--bare",
                "--quiet",
                "--template=",
                "--end-of-options",
                repo_text,
            ],
            Class::Local,
            None,
            SMALL_OUTPUT,
        )?;
        Ok(())
    }

    /// Git's own check of a ref name (S08-T06). Commit ids need none.
    pub fn check_ref_format(&self, r: &GitRef) -> Result<(), Error> {
        let GitRef::Name(name) = r else { return Ok(()) };
        // check-ref-format has no option terminator, but a name can't start with '-' here.
        let out = self.run(
            None,
            &["check-ref-format", "--allow-onelevel", name],
            Class::Local,
            None,
            SMALL_OUTPUT,
        )?;
        if out.success() {
            Ok(())
        } else {
            Err(RefError::Rejected.into())
        }
    }

    /// Fetches `r` (or the remote's default branch) from `url` into the bare cache `repo`,
    /// shallowly, and returns the commit it points to. The ref is only a hint; callers pin the
    /// returned commit.
    ///
    /// A fetch may add at most [`crate::git::FETCH_MAX_BYTES`] of objects; past that the process
    /// is killed and the call fails with `OutputTooLarge` (review M2). A cache that did not exist
    /// is fetched into a staging folder next to it and moved into place only on success, so an
    /// oversized or failed first fetch leaves nothing behind. An existing cache is fetched into in
    /// place (its older commits are pinned by installs and must stay); a failed or killed fetch
    /// only ever leaves temporary pack files, which are removed, and never moves the fetched ref.
    ///
    /// Git never follows redirects. If an https fetch fails the way a moved repository makes it
    /// fail, the server is asked once where it went ([`crate::redirect::probe`]) and the call
    /// fails with [`Error::Moved`] so the user can decide (decision 18.1).
    pub fn fetch(&self, repo: &Path, url: &GitUrl, r: Option<&GitRef>) -> Result<Commit, Error> {
        let result = self.fetch_checked(repo, url, r);
        if let Err(Error::Git(GitFailureKind::NotFound | GitFailureKind::Failed)) = &result
            && url.scheme() == Scheme::Https
            && let Some(moved) = crate::redirect::probe(url)
        {
            return Err(moved.into());
        }
        result
    }

    fn fetch_checked(
        &self,
        repo: &Path,
        url: &GitUrl,
        r: Option<&GitRef>,
    ) -> Result<Commit, Error> {
        // A private host, or a name that resolves to a private address, is fetched only after
        // the user confirmed it (decisions 17 and 18.8). The checked addresses are pinned for
        // https so Git doesn't resolve the name again to something else.
        let addrs = check_destination(url)?;
        let pin = curl_resolve(url, &addrs);
        let pin = pin.as_deref();
        if repo.join("HEAD").is_file() {
            let result = self.fetch_into(repo, url, r, pin);
            if result.is_err() {
                remove_partial_objects(repo);
            }
            return result;
        }
        let name = repo
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let staging = repo.with_file_name(format!(
            "{name}.staging-{}-{}",
            std::process::id(),
            STAGING_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&staging);
        let result = self.fetch_into(&staging, url, r, pin).and_then(|commit| {
            // A folder without HEAD is the leftover of an earlier failed attempt.
            let _ = std::fs::remove_dir_all(repo);
            std::fs::rename(&staging, repo).map_err(Error::io("move the new cache into place"))?;
            Ok(commit)
        });
        if result.is_err() {
            let _ = std::fs::remove_dir_all(&staging);
        }
        result
    }

    fn fetch_into(
        &self,
        repo: &Path,
        url: &GitUrl,
        r: Option<&GitRef>,
        pin: Option<&str>,
    ) -> Result<Commit, Error> {
        self.ensure_repo(repo)?;
        if let Some(r) = r {
            self.check_ref_format(r)?;
        }
        let source = r.map_or("HEAD", GitRef::as_str);
        let refspec = format!("+{source}:{FETCH_REF}");
        let fetched = self.run_fetch(
            repo,
            &[
                "fetch",
                "--quiet",
                "--no-tags",
                "--no-recurse-submodules",
                "--depth=1",
                "--end-of-options",
                url.as_str(),
                &refspec,
            ],
            SMALL_OUTPUT,
            pin,
        )?;
        if !fetched.success() {
            return Err(fetched.failure().into());
        }
        let out = self.run_ok(
            Some(repo),
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                "--end-of-options",
                &format!("{FETCH_REF}^{{commit}}"),
            ],
            Class::Local,
            None,
            SMALL_OUTPUT,
        )?;
        let commit = parse_commit(&out)?;
        if let Some(GitRef::Commit(wanted)) = r
            && *wanted != commit
        {
            return Err(Error::NotACommit);
        }
        Ok(commit)
    }

    /// Reads `.lullshift/marketplace.json` at `commit` from the cache and validates it.
    pub fn read_index(
        &self,
        repo: &Path,
        commit: &Commit,
        builtin: bool,
    ) -> Result<MarketplaceIndex, Error> {
        let listing = self.run(
            Some(repo),
            &[
                "ls-tree",
                "-z",
                "-l",
                "--end-of-options",
                commit.as_str(),
                "--",
                INDEX_PATH,
            ],
            Class::Local,
            None,
            SMALL_OUTPUT,
        )?;
        if !listing.success() {
            return Err(listing.failure().into());
        }
        let entries = parse_ls_tree(&listing.stdout)?;
        let entry = entries.first().ok_or(IndexError::Missing)?;
        if entries.len() != 1
            || entry.mode != "100644"
            || entry.kind != "blob"
            || entry.path != INDEX_PATH
        {
            return Err(IndexError::NotARegularFile.into());
        }
        if entry.size > INDEX_MAX_BYTES as u64 {
            return Err(IndexError::TooLarge.into());
        }
        let mut blobs = self.read_blobs(repo, &[(entry.oid.clone(), entry.size)])?;
        parse_index(&blobs.remove(0), builtin)
    }

    /// Turns an index entry into a pinned source: same-repo entries use the marketplace's commit;
    /// external entries are fetched into their own cache and their `ref` (or the default branch)
    /// is resolved to a commit.
    pub fn resolve_entry(
        &self,
        market: &MarketplaceRef<'_>,
        entry: &IndexEntry,
    ) -> Result<ResolvedSource, Error> {
        match &entry.source {
            EntrySource::Local { path } => Ok(ResolvedSource {
                repo: market.repo.to_path_buf(),
                commit: market.commit.clone(),
                path: Some(path.clone()),
                source_url: market.url.display().to_owned(),
                external: false,
            }),
            EntrySource::Git { url, git_ref, path } => {
                let key = hex(&Sha256::digest(url.as_str().as_bytes()));
                let repo = market.external_cache_root.join(&key[..32]).join("repo.git");
                let commit = self.fetch(&repo, url, git_ref.as_ref())?;
                Ok(ResolvedSource {
                    repo,
                    commit,
                    path: path.clone(),
                    source_url: url.display().to_owned(),
                    external: true,
                })
            }
        }
    }

    /// Reads `<path>/manifest.json` at `commit` for each folder in `paths` (for listing only:
    /// nothing here is installed). One answer per path, in order: `None` if the file is missing,
    /// isn't a regular file, is over 128 KiB or isn't a manifest this host accepts. Uses two Git
    /// calls per 50 paths rather than two per path.
    pub fn read_manifests(
        &self,
        repo: &Path,
        commit: &Commit,
        paths: &[&RelPath],
    ) -> Result<Vec<Option<Manifest>>, Error> {
        const CHUNK: usize = 50;
        const MANIFEST_LIST_MAX: u64 = 128 * 1024;
        let mut out = Vec::with_capacity(paths.len());
        for chunk in paths.chunks(CHUNK) {
            let files: Vec<String> = chunk
                .iter()
                .map(|p| format!("{}/manifest.json", p.as_str()))
                .collect();
            let mut args: Vec<&str> = vec![
                "ls-tree",
                "-z",
                "-l",
                "--end-of-options",
                commit.as_str(),
                "--",
            ];
            args.extend(files.iter().map(String::as_str));
            let listing = self.run(Some(repo), &args, Class::Local, None, SMALL_OUTPUT)?;
            if !listing.success() {
                return Err(listing.failure().into());
            }
            let entries = parse_ls_tree(&listing.stdout)?;
            let found: Vec<Option<&TreeEntry>> = files
                .iter()
                .map(|f| {
                    entries.iter().find(|e| {
                        e.path == *f
                            && e.mode == "100644"
                            && e.kind == "blob"
                            && e.size <= MANIFEST_LIST_MAX
                    })
                })
                .collect();
            let wanted: Vec<(String, u64)> = found
                .iter()
                .flatten()
                .map(|e| (e.oid.clone(), e.size))
                .collect();
            let mut blobs = self.read_blobs(repo, &wanted)?.into_iter();
            for entry in found {
                out.push(entry.and_then(|_| {
                    let bytes = blobs.next()?;
                    parse_manifest(std::str::from_utf8(&bytes).ok()?)
                }));
            }
        }
        Ok(out)
    }

    /// Every file under `path` at `commit` (recursively), as Git lists them. The folder must
    /// exist and be a tree.
    pub(crate) fn list_tree(
        &self,
        repo: &Path,
        commit: &Commit,
        path: Option<&RelPath>,
    ) -> Result<Vec<TreeEntry>, Error> {
        let spec = match path {
            Some(p) => format!("{commit}:{p}", p = p.as_str()),
            None => format!("{commit}^{{tree}}"),
        };
        let kind = self.run(
            Some(repo),
            &["cat-file", "-t", "--end-of-options", &spec],
            Class::Local,
            None,
            SMALL_OUTPUT,
        )?;
        if !kind.success() || kind.stdout.trim_ascii() != b"tree" {
            return Err(PackError::NotFound.into());
        }
        let out = match self.run(
            Some(repo),
            &["ls-tree", "-r", "-z", "-l", "--end-of-options", &spec],
            Class::Local,
            None,
            SMALL_OUTPUT,
        ) {
            // A tree with absurdly many entries is "too many files", not a Git fault.
            Err(Error::Git(GitFailureKind::OutputTooLarge)) => {
                return Err(PackError::TooManyFiles.into());
            }
            other => other?,
        };
        if !out.success() {
            return Err(out.failure().into());
        }
        parse_ls_tree(&out.stdout)
    }

    /// Reads blobs by object id, in order, checking that each is a blob of the listed size.
    pub(crate) fn read_blobs(
        &self,
        repo: &Path,
        wanted: &[(String, u64)],
    ) -> Result<Vec<Vec<u8>>, Error> {
        if wanted.is_empty() {
            return Ok(Vec::new());
        }
        let total: u64 = wanted.iter().map(|(_, s)| *s).sum();
        let cap = usize::try_from(total)
            .unwrap_or(usize::MAX)
            .saturating_add(128 * wanted.len() + 1024);
        let mut input = Vec::new();
        for (oid, _) in wanted {
            input.extend_from_slice(oid.as_bytes());
            input.push(b'\n');
        }
        let out = self.run_ok(
            Some(repo),
            &["cat-file", "--batch"],
            Class::Local,
            Some(input),
            cap,
        )?;
        let mut rest = out.as_slice();
        let mut blobs = Vec::with_capacity(wanted.len());
        for (oid, size) in wanted {
            let nl = rest
                .iter()
                .position(|b| *b == b'\n')
                .ok_or(PackError::Inconsistent)?;
            let header = std::str::from_utf8(&rest[..nl]).map_err(|_| PackError::Inconsistent)?;
            let expected = format!("{oid} blob {size}");
            if header != expected {
                return Err(PackError::Inconsistent.into());
            }
            let start = nl + 1;
            let end = start
                .checked_add(usize::try_from(*size).map_err(|_| PackError::Inconsistent)?)
                .ok_or(PackError::Inconsistent)?;
            if rest.len() < end + 1 || rest[end] != b'\n' {
                return Err(PackError::Inconsistent.into());
            }
            blobs.push(rest[start..end].to_vec());
            rest = &rest[end + 1..];
        }
        Ok(blobs)
    }
}

/// Removes what a killed or failed fetch leaves in an existing cache: temporary pack files and
/// quarantine folders. Finished objects and refs are untouched.
fn remove_partial_objects(repo: &Path) {
    let objects = repo.join("objects");
    if let Ok(read) = std::fs::read_dir(objects.join("pack")) {
        for e in read.flatten() {
            if e.file_name().to_string_lossy().starts_with("tmp_") {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    if let Ok(read) = std::fs::read_dir(&objects) {
        for e in read.flatten() {
            if e.file_name().to_string_lossy().starts_with("incoming-") {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }
}

fn parse_commit(out: &[u8]) -> Result<Commit, Error> {
    let text = std::str::from_utf8(out).map_err(|_| Error::NotACommit)?;
    Commit::parse(text.trim()).ok_or(Error::NotACommit)
}

/// Parses `ls-tree -z -l` output: `<mode> SP <type> SP <oid> SP+ <size> TAB <path> NUL`.
pub(crate) fn parse_ls_tree(out: &[u8]) -> Result<Vec<TreeEntry>, Error> {
    let mut entries = Vec::new();
    for record in out.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let tab = record
            .iter()
            .position(|b| *b == b'\t')
            .ok_or(PackError::Inconsistent)?;
        let meta = std::str::from_utf8(&record[..tab]).map_err(|_| PackError::Inconsistent)?;
        let path = std::str::from_utf8(&record[tab + 1..]).map_err(|_| {
            PackError::BadPath(
                "(not UTF-8)".to_owned(),
                crate::error::PathError::ForbiddenCharacter,
            )
        })?;
        let mut parts = meta.split_ascii_whitespace();
        let (Some(mode), Some(kind), Some(oid), Some(size), None) = (
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
        ) else {
            return Err(PackError::Inconsistent.into());
        };
        // Submodules list "-" for the size.
        let size = if size == "-" {
            0
        } else {
            size.parse::<u64>().map_err(|_| PackError::Inconsistent)?
        };
        if oid.len() != 40 || !oid.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(PackError::Inconsistent.into());
        }
        entries.push(TreeEntry {
            mode: mode.to_owned(),
            kind: kind.to_owned(),
            oid: oid.to_owned(),
            size,
            path: path.to_owned(),
        });
    }
    Ok(entries)
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}
