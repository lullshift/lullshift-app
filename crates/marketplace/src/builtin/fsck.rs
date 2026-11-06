//! The built-in Git's object check after a fetch: the part of `git fsck --strict`
//! (`transfer.fsckObjects`) that matters for what Lullshift reads (ADR-023).
//!
//! gix already verifies every object's hash and the pack checksum as the pack is indexed
//! (`Mode::Verify`), and refuses objects that don't decode. On top of that, the fetched commit
//! and every tree reachable from it are checked here before the fetched ref moves:
//!
//! - the commit decodes and its tree exists (connectivity);
//! - every entry has a mode Git writes (`40000`, `100644`, `100755`, `120000`, `160000`; no
//!   zero-padding, as `--strict` requires);
//! - names are not empty, `.`, `..`, contain no `/`, and are not `.git` in any spelling Git
//!   refuses (any case, trailing dots or spaces, the NTFS short name `git~1`, an NTFS stream
//!   suffix, HFS+ ignorable characters);
//! - `.gitmodules` is not a symlink;
//! - entries are sorted the way Git sorts them, with no duplicates;
//! - no entry is the null object id, and every blob and tree it names exists.
//!
//! Gaps against `git fsck --strict` are listed in ADR-023: commit and tag header details
//! (dates, e-mail syntax), `.gitmodules` contents and symlink target checks. Lullshift's own pack checks refuse links,
//! submodules and `.git` paths in what it installs regardless.

use gix::ObjectId;
use gix::objs::tree::EntryKind;

use crate::error::{Error, GitFailureKind};

/// Upper bound on trees visited, so a hostile repository can't make the check run forever.
/// Far above anything the 512-file pack limit needs.
const MAX_TREES: usize = 200_000;

fn fail() -> Error {
    Error::Git(GitFailureKind::Failed)
}

/// Checks `commit` and every tree under it in `repo`.
pub(crate) fn check_commit(repo: &gix::Repository, commit: ObjectId) -> Result<(), Error> {
    let commit = repo.find_commit(commit).map_err(|_| fail())?;
    let tree = commit.decode().map_err(|_| fail())?.tree();
    let mut stack = vec![tree];
    let mut seen = std::collections::HashSet::new();
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        if seen.len() > MAX_TREES {
            return Err(fail());
        }
        let tree = repo.find_tree(id).map_err(|_| fail())?;
        let decoded = tree.decode().map_err(|_| fail())?;
        let mut previous: Option<(Vec<u8>, bool)> = None;
        let mut names = std::collections::HashSet::new();
        for entry in &decoded.entries {
            let name: &[u8] = entry.filename.as_ref();
            let mut raw = [0u8; 6];
            let mode = entry.mode.as_bytes(&mut raw);
            let mode: &[u8] = mode.as_ref();
            let kind = entry.mode.kind();
            let canonical: &[u8] = match kind {
                EntryKind::Tree => b"40000",
                EntryKind::Blob => b"100644",
                EntryKind::BlobExecutable => b"100755",
                EntryKind::Link => b"120000",
                EntryKind::Commit => b"160000",
            };
            if mode != canonical {
                return Err(fail());
            }
            check_name(name)?;
            if kind == EntryKind::Link && is_gitmodules(name) {
                return Err(fail());
            }
            let is_tree = kind == EntryKind::Tree;
            // A blob and a tree of the same name are duplicates too, even when not adjacent.
            if !names.insert(name.to_vec()) {
                return Err(fail());
            }
            if let Some((prev, prev_tree)) = &previous
                && git_order(prev, *prev_tree, name, is_tree) != std::cmp::Ordering::Less
            {
                return Err(fail());
            }
            previous = Some((name.to_vec(), is_tree));
            let oid = entry.oid.to_owned();
            if oid.is_null() {
                return Err(fail());
            }
            match kind {
                EntryKind::Commit => {}
                EntryKind::Tree => stack.push(oid),
                _ => {
                    if !repo.has_object(oid) {
                        return Err(fail());
                    }
                }
            }
        }
    }
    Ok(())
}

/// Git's tree order: names compared as bytes, a tree's name as if it ended in `/`.
fn git_order(a: &[u8], a_tree: bool, b: &[u8], b_tree: bool) -> std::cmp::Ordering {
    let common = a.len().min(b.len());
    match a[..common].cmp(&b[..common]) {
        std::cmp::Ordering::Equal => {}
        other => return other,
    }
    let next = |n: &[u8], tree: bool| -> Option<u8> {
        n.get(common)
            .copied()
            .or(if tree { Some(b'/') } else { None })
    };
    next(a, a_tree).cmp(&next(b, b_tree))
}

/// Characters HFS+ ignores in names, so `.g\u{200c}it` is `.git` there.
const HFS_IGNORED: &[char] = &[
    '\u{200c}', '\u{200d}', '\u{200e}', '\u{200f}', '\u{202a}', '\u{202b}', '\u{202c}', '\u{202d}',
    '\u{202e}', '\u{206a}', '\u{206b}', '\u{206c}', '\u{206d}', '\u{206e}', '\u{206f}', '\u{feff}',
];

fn check_name(name: &[u8]) -> Result<(), Error> {
    if name.is_empty() || name == b"." || name == b".." || name.contains(&b'/') || name.contains(&0)
    {
        return Err(fail());
    }
    if is_dotgit(name) {
        return Err(fail());
    }
    Ok(())
}

/// `.git` the way Git's fsck sees it on any file system.
fn is_dotgit(name: &[u8]) -> bool {
    let lower = name.to_ascii_lowercase();
    // NTFS: trailing dots and spaces are dropped, and `name:stream` is `name`.
    let base = match lower.iter().position(|b| *b == b':') {
        Some(i) => &lower[..i],
        None => &lower[..],
    };
    let trimmed = {
        let mut end = base.len();
        while end > 0 && matches!(base[end - 1], b'.' | b' ') {
            end -= 1;
        }
        &base[..end]
    };
    if trimmed == b".git" || trimmed == b"git~1" {
        return true;
    }
    // HFS+: ignorable code points are dropped before comparing.
    if let Ok(text) = std::str::from_utf8(&lower) {
        let folded: String = text.chars().filter(|c| !HFS_IGNORED.contains(c)).collect();
        if folded == ".git" {
            return true;
        }
    }
    false
}

fn is_gitmodules(name: &[u8]) -> bool {
    let lower = name.to_ascii_lowercase();
    let mut end = lower.len();
    while end > 0 && matches!(lower[end - 1], b'.' | b' ') {
        end -= 1;
    }
    let trimmed = &lower[..end];
    trimmed == b".gitmodules" || trimmed == b"gitmod~1"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dotgit_spellings_are_refused() {
        for bad in [
            &b".git"[..],
            b".GIT",
            b".Git.",
            b".git ",
            b".git. .",
            b"git~1",
            b"GIT~1",
            b".git::$INDEX_ALLOCATION",
            ".g\u{200c}it".as_bytes(),
            "\u{feff}.git".as_bytes(),
            b"",
            b".",
            b"..",
            b"a/b",
        ] {
            assert!(
                check_name(bad).is_err(),
                "{:?}",
                String::from_utf8_lossy(bad)
            );
        }
        for good in [&b".gitignore"[..], b"git", b"x.git", b".github", b"git~2"] {
            assert!(
                check_name(good).is_ok(),
                "{:?}",
                String::from_utf8_lossy(good)
            );
        }
        assert!(is_gitmodules(b".GITMODULES"));
        assert!(is_gitmodules(b"gitmod~1"));
        assert!(!is_gitmodules(b".gitmodules2"));
    }

    #[test]
    fn git_tree_order() {
        use std::cmp::Ordering::*;
        assert_eq!(git_order(b"a", false, b"b", false), Less);
        // "a.b" (0x2e) sorts before the tree "a" ("a/", 0x2f), which sorts before "a0".
        assert_eq!(git_order(b"a.b", false, b"a", true), Less);
        assert_eq!(git_order(b"a", true, b"a0", false), Less);
        // The blob "a" sorts before "a.b".
        assert_eq!(git_order(b"a", false, b"a.b", false), Less);
        assert_eq!(git_order(b"b", false, b"a", false), Greater);
    }
}
