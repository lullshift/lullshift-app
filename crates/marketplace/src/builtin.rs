//! The built-in Git: gitoxide (`gix`), used only when no `git` is on `PATH` (decision 28,
//! ADR-023). Public https marketplaces only.
//!
//! It does the same few things the hardened runner does, on the same cache layout (a normal
//! bare repository, so installed Git can read a cache the built-in one wrote, and the other way
//! round):
//!
//! - create a bare cache with no hooks and no templates;
//! - fetch one ref or one commit, depth 1, no tags, over [`http::PinnedHttp`] (pinned
//!   addresses, no proxy, no redirects, no credentials, size cap, deadline);
//! - check the fetched objects ([`fsck`]) before the fetched ref moves;
//! - list trees and read blobs, producing exactly what `ls-tree -z -l` and `cat-file --batch`
//!   give the rest of the crate.
//!
//! Repositories are opened isolated: no system, global or user Git configuration, no
//! environment variables, no credential helpers, no prompts. Nothing is ever checked out, so no
//! working-tree file is written, and no hook, filter, LFS or submodule code exists on this path.

pub(crate) mod fsck;
pub(crate) mod http;

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use gix::ObjectId;
use gix::objs::tree::EntryKind;

use crate::error::{Error, GitFailureKind, PackError, RefError};
use crate::repo::TreeEntry;
use crate::validate::{Commit, GitRef, GitUrl, PATH_PARTS_MAX, RelPath};

/// The temporary ref a fetch writes; the real one moves only after the object check.
const INCOMING_REF: &str = "refs/lullshift/incoming";
/// Largest single object gix may inflate (`gitoxide.objects.allocLimit`), against delta bombs.
/// Lullshift reads files of at most `pack::MAX_FILE_BYTES` (5 MiB), inside packs of at most
/// `pack::MAX_PACK_BYTES` (20 MiB), plus small trees, so 64 MiB covers any object it needs
/// while a delta that expands past it fails cleanly instead of using the 200 MiB fetch cap.
const ALLOC_LIMIT: u64 = 64 * 1024 * 1024;
/// Threads for gix's pack indexing and delta resolution (`pack.threads`): with the limit above
/// the peak is about `PACK_THREADS * ALLOC_LIMIT` rather than one share per core.
const PACK_THREADS: u32 = 2;
/// Most entries (files, folders and submodules, counted at every visit of a tree) a local read
/// looks at. A 512-file activity at the deepest allowed path needs at most
/// `MAX_FILES * PATH_PARTS_MAX` (6144), so only a tree bomb reaches this.
const MAX_VISITED: usize = 100_000;

/// What a local, in-process read may spend: a deadline matching the runner's local timeout and
/// a count of entries seen (trees included, every visit), so a tree bomb that Git's own
/// `ls-tree` would be killed for ends quickly here too.
struct Guard {
    deadline: Instant,
    visited: usize,
}

impl Guard {
    fn new(timeout: Duration) -> Self {
        Self {
            deadline: Instant::now() + timeout,
            visited: 0,
        }
    }

    fn check_time(&self) -> Result<(), Error> {
        if Instant::now() >= self.deadline {
            return Err(GitFailureKind::Timeout.into());
        }
        Ok(())
    }

    /// Counts `n` entries about to be looked at.
    fn visit(&mut self, n: usize) -> Result<(), Error> {
        self.visited = self.visited.saturating_add(n);
        if self.visited > MAX_VISITED {
            return Err(PackError::TooManyFiles.into());
        }
        self.check_time()
    }
}

/// Settings for one built-in fetch, taken from the [`crate::Git`] that runs it.
pub(crate) struct FetchSettings {
    pub timeout: Duration,
    pub cap: u64,
    pub fsck: bool,
    pub plain_http_for_tests: bool,
}

fn failed() -> Error {
    Error::Git(GitFailureKind::Failed)
}

/// Creates the bare cache `repo` the way `git init --bare --template=` does: `HEAD`, `config`,
/// `objects/{info,pack}`, `refs/{heads,tags}`, and nothing else (no hooks, no description).
pub(crate) fn ensure_repo(repo: &Path) -> Result<(), Error> {
    if repo.join("HEAD").is_file() {
        return Ok(());
    }
    for dir in ["objects/info", "objects/pack", "refs/heads", "refs/tags"] {
        std::fs::create_dir_all(repo.join(dir)).map_err(Error::io("create the cache folder"))?;
    }
    std::fs::write(
        repo.join("config"),
        "[core]\n\trepositoryformatversion = 0\n\tfilemode = false\n\tbare = true\n",
    )
    .map_err(Error::io("write the cache configuration"))?;
    // HEAD last: its presence is what marks a finished cache.
    std::fs::write(repo.join("HEAD"), "ref: refs/heads/main\n")
        .map_err(Error::io("write the cache HEAD"))?;
    Ok(())
}

/// Opens a cache with no configuration but its own, no environment and no helpers.
fn open(repo: &Path) -> Result<gix::Repository, Error> {
    let options = gix::open::Options::isolated()
        .open_path_as_is(true)
        .with(gix::sec::Trust::Full)
        .config_overrides([
            format!("gitoxide.objects.allocLimit={ALLOC_LIMIT}"),
            format!("pack.threads={PACK_THREADS}"),
        ]);
    gix::open_opts(repo, options).map_err(|_| failed())
}

/// Git's own ref-name rule (`check-ref-format --allow-onelevel`), via gix.
pub(crate) fn check_ref_format(name: &str) -> Result<(), Error> {
    gix::validate::reference::name_partial(name.as_bytes().into())
        .map(|_| ())
        .map_err(|_| RefError::Rejected.into())
}

/// Fetches `r` (or the remote's default branch) from `url` into `repo`, depth 1, connecting
/// only to `addrs`, and returns the commit. `repo` must exist ([`ensure_repo`]).
pub(crate) fn fetch(
    repo_dir: &Path,
    url: &GitUrl,
    r: Option<&GitRef>,
    addrs: &[SocketAddr],
    settings: &FetchSettings,
) -> Result<Commit, Error> {
    let budget = http::Budget::new(settings.timeout, settings.cap);
    let transport_http = http::PinnedHttp::new(
        url.host(),
        url.effective_port(),
        addrs,
        Arc::clone(&budget),
        settings.plain_http_for_tests,
    )
    .ok_or(Error::Git(GitFailureKind::Offline))?;

    // A watchdog stops gix's pack processing at the deadline, not only its reads.
    let done = Arc::new(AtomicBool::new(false));
    let watchdog = {
        let budget = Arc::clone(&budget);
        let done = Arc::clone(&done);
        std::thread::spawn(move || {
            while !done.load(Ordering::Relaxed) {
                if std::time::Instant::now() >= budget.deadline {
                    budget.time_out();
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        })
    };
    let mut rejected = Vec::new();
    let result = fetch_inner(
        repo_dir,
        url,
        r,
        transport_http,
        &budget,
        settings,
        &mut rejected,
    );
    done.store(true, Ordering::Relaxed);
    let _ = watchdog.join();
    // The repository handle (and its memory maps of the pack) is gone now, so the files can be
    // deleted on Windows too. Objects that failed the check don't stay in the cache, as with
    // Git's quarantine.
    for p in rejected {
        let _ = std::fs::remove_file(p);
    }

    match result {
        Ok(commit) => Ok(commit),
        Err(Inner::Ours(e)) => Err(e),
        Err(Inner::Gix(text)) => Err(classify(&budget.seen(), &text, url, r)),
    }
}

enum Inner {
    /// Already an honest error.
    Ours(Error),
    /// gix failed; its text is used only to pick a category, then dropped.
    Gix(String),
}

fn gx<T, E: std::fmt::Debug>(r: Result<T, E>) -> Result<T, Inner> {
    r.map_err(|e| Inner::Gix(format!("{e:?}")))
}

fn fetch_inner(
    repo_dir: &Path,
    url: &GitUrl,
    r: Option<&GitRef>,
    transport_http: http::PinnedHttp,
    budget: &Arc<http::Budget>,
    settings: &FetchSettings,
    rejected: &mut Vec<std::path::PathBuf>,
) -> Result<Commit, Inner> {
    let repo = open(repo_dir).map_err(Inner::Ours)?;
    let source = r.map_or("HEAD", GitRef::as_str);
    let refspec = format!("+{source}:{INCOMING_REF}");
    let gix_url = gx(gix::url::parse(gix::bstr::BStr::new(url.as_str())))?;
    let remote = gx(repo.remote_at_without_url_rewrite(gix_url.clone()))?
        .with_fetch_tags(gix::remote::fetch::Tags::None);
    let remote = gx(remote.with_refspecs([refspec.as_str()], gix::remote::Direction::Fetch))?;
    let transport = gix_transport::client::blocking_io::http::connect_http(
        transport_http,
        gix_url,
        gix::protocol::transport::Protocol::V2,
        false,
    );
    // Never a credential helper or a prompt: a repository that wants a sign-in needs the
    // user's own Git (decision 28).
    let connection = remote
        .to_connection_with_transport(transport)
        .with_credentials(|_| {
            use gix::error::ErrorExt as _;
            Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied).raise())
        });
    let prepare = gx(connection.prepare_fetch(gix::progress::Discard, Default::default()))?;
    let depth = std::num::NonZeroU32::new(1).expect("1 is not zero");
    let outcome = gx(prepare
        .with_shallow(gix::remote::fetch::Shallow::DepthAtRemote(depth))
        .with_write_packed_refs_only(false)
        .receive(gix::progress::Discard, &budget.interrupt))?;
    let new_pack = match &outcome.status {
        gix::remote::fetch::Status::Change {
            write_pack_bundle, ..
        } => Some((
            write_pack_bundle.data_path.clone(),
            write_pack_bundle.index_path.clone(),
            write_pack_bundle.keep_path.clone(),
        )),
        gix::remote::fetch::Status::NoPackReceived { .. } => None,
    };
    let checked = settle(&repo, r, settings.fsck, &budget.interrupt);
    if checked.is_err()
        && let Some((data, index, keep)) = new_pack
    {
        // Deleted by the caller once `repo` is dropped: a file gix may still have mapped
        // can't be deleted on Windows.
        rejected.extend([data, index, keep].into_iter().flatten());
    }
    let _ = delete_ref(&repo, INCOMING_REF);
    checked.map_err(Inner::Ours)
}

/// After a fetch: peel the incoming ref to a commit, check it, then move the fetched ref.
fn settle(
    repo: &gix::Repository,
    r: Option<&GitRef>,
    fsck_on: bool,
    stop: &AtomicBool,
) -> Result<Commit, Error> {
    let not_found = || Error::Git(GitFailureKind::NotFound);
    let mut incoming = repo
        .try_find_reference(INCOMING_REF)
        .map_err(|_| failed())?
        .ok_or_else(not_found)?;
    let commit = incoming.peel_to_commit().map_err(|_| Error::NotACommit)?;
    let id = commit.id;
    let pinned = Commit::parse(&id.to_hex().to_string()).ok_or(Error::NotACommit)?;
    if let Some(GitRef::Commit(wanted)) = r
        && *wanted != pinned
    {
        return Err(Error::NotACommit);
    }
    if fsck_on {
        fsck::check_commit(repo, id, stop)?;
    }
    repo.reference(
        crate::repo::FETCH_REF,
        id,
        gix::refs::transaction::PreviousValue::Any,
        "lullshift: fetched",
    )
    .map_err(|_| failed())?;
    Ok(pinned)
}

fn delete_ref(repo: &gix::Repository, name: &str) -> Result<(), Error> {
    if let Ok(Some(r)) = repo.try_find_reference(name) {
        r.delete().map_err(|_| failed())?;
    }
    Ok(())
}

/// The honest category for a failed built-in fetch: what the HTTP layer saw first, then the
/// shape of gix's error.
fn classify(seen: &http::Seen, text: &str, url: &GitUrl, r: Option<&GitRef>) -> Error {
    if seen.too_large {
        return GitFailureKind::OutputTooLarge.into();
    }
    if seen.timeout {
        return GitFailureKind::Timeout.into();
    }
    if seen.auth {
        return GitFailureKind::NeedsSystemGit.into();
    }
    if let Some((status, location)) = &seen.redirect {
        return match crate::redirect::moved_to(url, *status, location.as_deref()) {
            Some(moved) => moved.into(),
            None => GitFailureKind::Failed.into(),
        };
    }
    if seen.not_found {
        return GitFailureKind::NotFound.into();
    }
    if seen.offline {
        return GitFailureKind::Offline.into();
    }
    let lower = text.to_ascii_lowercase();
    if lower.contains("matched any of the")
        || lower.contains("not our ref")
        || lower.contains("not our object")
        || lower.contains("upload-pack: not")
    {
        return GitFailureKind::NotFound.into();
    }
    match crate::git::classify_failure(1, &lower) {
        GitFailureKind::Failed
            if matches!(r, Some(GitRef::Commit(_))) && lower.contains("err ") =>
        {
            // The server refused a commit it doesn't serve by id, as Git reports "not our ref".
            GitFailureKind::NotFound.into()
        }
        GitFailureKind::AuthDenied => GitFailureKind::NeedsSystemGit.into(),
        other => other.into(),
    }
}

/// The commit object for a pinned commit, or `NotFound` if this cache doesn't have it.
fn commit_tree(repo: &gix::Repository, commit: &Commit) -> Result<ObjectId, Error> {
    let id = ObjectId::from_hex(commit.as_str().as_bytes()).map_err(|_| Error::NotACommit)?;
    let obj = repo
        .try_find_object(id)
        .map_err(|_| failed())?
        .ok_or(Error::Git(GitFailureKind::NotFound))?;
    let commit = obj.try_into_commit().map_err(|_| Error::NotACommit)?;
    let tree = commit.tree_id().map_err(|_| failed())?;
    Ok(tree.detach())
}

/// The canonical mode and kind text `ls-tree` prints for an entry.
fn mode_and_kind(kind: EntryKind) -> (&'static str, &'static str) {
    match kind {
        EntryKind::Tree => ("040000", "tree"),
        EntryKind::Blob => ("100644", "blob"),
        EntryKind::BlobExecutable => ("100755", "blob"),
        EntryKind::Link => ("120000", "blob"),
        EntryKind::Commit => ("160000", "commit"),
    }
}

fn entry(
    repo: &gix::Repository,
    kind: EntryKind,
    oid: ObjectId,
    path: String,
) -> Result<TreeEntry, Error> {
    let (mode, kind_text) = mode_and_kind(kind);
    let size = if kind_text == "blob" {
        repo.find_header(oid)
            .map_err(|_| PackError::Inconsistent)?
            .size()
    } else {
        0
    };
    Ok(TreeEntry {
        mode: mode.to_owned(),
        kind: kind_text.to_owned(),
        oid: oid.to_hex().to_string(),
        size,
        path,
    })
}

/// The entries named `name` in tree `tree` (more than one only in a malformed tree).
fn children(
    repo: &gix::Repository,
    tree: ObjectId,
    guard: &mut Guard,
) -> Result<Vec<(Vec<u8>, EntryKind, ObjectId)>, Error> {
    guard.check_time()?;
    let tree = repo.find_tree(tree).map_err(|_| PackError::Inconsistent)?;
    let decoded = tree.decode().map_err(|_| PackError::Inconsistent)?;
    // Every entry is counted, folders included, at every visit of the tree.
    guard.visit(decoded.entries.len().max(1))?;
    Ok(decoded
        .entries
        .iter()
        .map(|e| {
            let name: &[u8] = e.filename.as_ref();
            (name.to_vec(), e.mode.kind(), e.oid.to_owned())
        })
        .collect())
}

/// Like `ls-tree -l <commit> -- <paths>`: for each path, the entries at exactly that path.
pub(crate) fn ls_paths(
    repo_dir: &Path,
    commit: &Commit,
    paths: &[String],
    timeout: Duration,
) -> Result<Vec<TreeEntry>, Error> {
    let mut guard = Guard::new(timeout);
    let repo = open(repo_dir)?;
    let root = commit_tree(&repo, commit)?;
    let mut out = Vec::new();
    for path in paths {
        let parts: Vec<&str> = path.split('/').collect();
        let Some((last, dirs)) = parts.split_last() else {
            continue;
        };
        let mut tree = Some(root);
        for dir in dirs {
            let Some(t) = tree else { break };
            tree = children(&repo, t, &mut guard)?
                .into_iter()
                .find(|(n, k, _)| n.as_slice() == dir.as_bytes() && *k == EntryKind::Tree)
                .map(|(_, _, oid)| oid);
        }
        let Some(t) = tree else { continue };
        for (name, kind, oid) in children(&repo, t, &mut guard)? {
            if name.as_slice() == last.as_bytes() {
                out.push(entry(&repo, kind, oid, path.clone())?);
            }
        }
    }
    Ok(out)
}

/// Like `cat-file -t <commit>:<path>` = tree, then `ls-tree -r -z -l` of it: every blob and
/// submodule under `path`, with paths relative to it. `max_output` bounds the listing the way
/// the runner's output cap does.
pub(crate) fn ls_tree_recursive(
    repo_dir: &Path,
    commit: &Commit,
    path: Option<&RelPath>,
    max_output: usize,
    timeout: Duration,
) -> Result<Vec<TreeEntry>, Error> {
    let mut guard = Guard::new(timeout);
    let repo = open(repo_dir)?;
    let root = commit_tree(&repo, commit).map_err(|_| PackError::NotFound)?;
    let start = match path {
        None => root,
        Some(p) => {
            let mut tree = root;
            for part in p.as_str().split('/') {
                tree = children(&repo, tree, &mut guard)?
                    .into_iter()
                    .find(|(n, _, _)| n.as_slice() == part.as_bytes())
                    .filter(|(_, k, _)| *k == EntryKind::Tree)
                    .map(|(_, _, oid)| oid)
                    .ok_or(PackError::NotFound)?;
            }
            tree
        }
    };
    let mut out = Vec::new();
    let mut printed = 0usize;
    // Depth-first in tree order, as `ls-tree -r` prints.
    let mut stack: Vec<(ObjectId, String, usize)> = vec![(start, String::new(), 0)];
    while let Some((tree, prefix, depth)) = stack.pop() {
        // A file under `depth` folders has `depth + 1` path parts.
        if depth >= PATH_PARTS_MAX {
            return Err(PackError::BadPath(
                "(too deep)".to_owned(),
                crate::error::PathError::TooDeep,
            )
            .into());
        }
        let mut subtrees = Vec::new();
        for (name, kind, oid) in children(&repo, tree, &mut guard)? {
            let name = String::from_utf8(name).map_err(|_| {
                PackError::BadPath(
                    "(not UTF-8)".to_owned(),
                    crate::error::PathError::ForbiddenCharacter,
                )
            })?;
            let full = format!("{prefix}{name}");
            if kind == EntryKind::Tree {
                subtrees.push((oid, format!("{full}/"), depth + 1));
                continue;
            }
            // What the line would cost in `ls-tree -z -l` output.
            printed = printed.saturating_add(6 + 1 + 6 + 1 + 40 + 1 + 7 + 1 + full.len() + 1);
            if printed > max_output {
                return Err(PackError::TooManyFiles.into());
            }
            out.push(entry(&repo, kind, oid, full)?);
        }
        stack.extend(subtrees);
    }
    // Restore the order `ls-tree -r` prints: a subtree's files come where the subtree sits.
    // Git's tree order compares a folder as its name plus '/', so the order `ls-tree -r`
    // prints is the byte order of the full paths.
    out.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));
    Ok(out)
}

/// Like `cat-file --batch` for blob ids: each must be a blob of exactly the listed size.
pub(crate) fn read_blobs(
    repo_dir: &Path,
    wanted: &[(String, u64)],
    timeout: Duration,
) -> Result<Vec<Vec<u8>>, Error> {
    let mut guard = Guard::new(timeout);
    let repo = open(repo_dir)?;
    let mut blobs = Vec::with_capacity(wanted.len());
    for (oid, size) in wanted {
        guard.visit(1)?;
        let id = ObjectId::from_hex(oid.as_bytes()).map_err(|_| PackError::Inconsistent)?;
        let obj = repo
            .try_find_object(id)
            .map_err(|_| PackError::Inconsistent)?
            .ok_or(PackError::Inconsistent)?;
        if obj.kind != gix::objs::Kind::Blob || obj.data.len() as u64 != *size {
            return Err(PackError::Inconsistent.into());
        }
        blobs.push(obj.detach().data);
    }
    Ok(blobs)
}

/// Removes what a failed built-in fetch can leave: gix's temporary files in the pack folder
/// (they are deleted on drop, but a crash could leave one) and the incoming ref.
pub(crate) fn remove_partial(repo_dir: &Path) {
    if let Ok(read) = std::fs::read_dir(repo_dir.join("objects").join("pack")) {
        for e in read.flatten() {
            if e.file_name().to_string_lossy().starts_with(".tmp") {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    let _ = std::fs::remove_file(repo_dir.join(INCOMING_REF));
}
