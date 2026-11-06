//! Shared builders for the marketplace tests: real Git repositories in temp folders, crafted
//! with plumbing commands so tests can create trees (symlinks, submodules, odd modes) that no
//! checkout would produce. Only compiled for tests.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};
use tempfile::TempDir;

use crate::git::Git;
use crate::pack::AvailableEngine;
use crate::validate::{Commit, GitUrl};

/// Set to `builtin` to run the fixture suite on the built-in Git (decision 28, ADR-023): the
/// test binary re-runs itself with it (`tests::builtin::whole_suite_on_the_builtin_git`).
pub const BACKEND_VAR: &str = "LULLSHIFT_TEST_GIT_BACKEND";

/// True when this test process runs the suite on the built-in Git.
pub fn builtin_mode() -> bool {
    std::env::var(BACKEND_VAR).is_ok_and(|v| v == "builtin")
}

/// The address of a repository folder that may not exist, for the backend under test.
pub fn url_for(path: &Path) -> GitUrl {
    if builtin_mode() {
        crate::testserver::repo_url(path)
    } else {
        GitUrl::for_test_file(path)
    }
}

/// A test-mode `Git` for the backend this process tests.
fn test_git(app: &Path) -> Git {
    if builtin_mode() {
        Git::builtin_for_tests(app)
    } else {
        Git::for_tests(app).expect("git on PATH")
    }
}

/// An app folder with a test-mode `Git`, cache folders and an install root.
pub struct Env {
    pub tmp: TempDir,
    pub git: Git,
}

impl Env {
    pub fn new() -> Self {
        let tmp = tempfile::tempdir().expect("tempdir");
        let git = test_git(&tmp.path().join("app"));
        Self { tmp, git }
    }

    /// Like `new`, but Git's own object check is off, so trees that it would refuse at fetch
    /// (`..` entries, hostile `.gitmodules`) reach the pack validation behind it.
    pub fn lenient() -> Self {
        let tmp = tempfile::tempdir().expect("tempdir");
        let git = test_git(&tmp.path().join("app")).without_fsck_for_tests();
        Self { tmp, git }
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.tmp.path().join(name)
    }

    pub fn install_root(&self) -> PathBuf {
        self.path("installed")
    }

    /// A fresh source repository under the temp folder.
    pub fn src(&self, name: &str) -> Src {
        Src::new(&self.path(name))
    }
}

/// One file to put in a commit.
#[derive(Clone)]
pub struct F {
    pub path: String,
    /// Git mode: `100644`, `100755`, `120000` (content is the link target) or `160000`.
    pub mode: &'static str,
    pub bytes: Vec<u8>,
}

impl F {
    pub fn new(path: &str, bytes: impl AsRef<[u8]>) -> Self {
        Self {
            path: path.to_owned(),
            mode: "100644",
            bytes: bytes.as_ref().to_vec(),
        }
    }

    pub fn mode(mut self, mode: &'static str) -> Self {
        self.mode = mode;
        self
    }
}

/// A non-bare source repository whose commits are built by plumbing, never by checkout.
pub struct Src {
    pub dir: PathBuf,
    config: PathBuf,
}

impl Src {
    fn new(dir: &Path) -> Self {
        std::fs::create_dir_all(dir).expect("mkdir");
        let config = dir.with_extension("gitconfig");
        std::fs::write(&config, "[user]\n\tname = Test\n\temail = test@example.com\n[commit]\n\tgpgsign = false\n[core]\n\tignorecase = false\n")
            .expect("gitconfig");
        let s = Self {
            dir: dir.to_path_buf(),
            config,
        };
        s.git(&["init", "--quiet", "--initial-branch=main"], None);
        s
    }

    /// Runs plain git (not the hardened runner) in the source repository; panics on failure.
    pub fn git(&self, args: &[&str], stdin: Option<&[u8]>) -> String {
        self.git_env(args, stdin, &[])
    }

    pub fn git_env(&self, args: &[&str], stdin: Option<&[u8]>, env: &[(&str, &Path)]) -> String {
        let mut cmd = Command::new("git");
        cmd.arg("-C")
            .arg(&self.dir)
            .args(args)
            .env("GIT_CONFIG_GLOBAL", &self.config)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (k, v) in env {
            cmd.env(k, v);
        }
        let mut child = cmd.spawn().expect("spawn git");
        let mut pipe = child.stdin.take().expect("stdin");
        if let Some(data) = stdin {
            pipe.write_all(data).expect("write stdin");
        }
        drop(pipe);
        let out = child.wait_with_output().expect("git output");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    /// The repository's address for the backend under test: `file://` for installed Git, the
    /// loopback test server for the built-in Git.
    pub fn url(&self) -> GitUrl {
        url_for(&self.dir)
    }

    /// Always the loopback test server address, whichever backend this process tests.
    pub fn http_url(&self) -> GitUrl {
        crate::testserver::repo_url(&self.dir)
    }

    /// Commits exactly `files` (no other content) on `main`, advancing it, and returns the
    /// commit. Three git calls however many files there are.
    pub fn commit(&self, files: &[F]) -> Commit {
        let scratch = self.dir.join(".git").join("scratch");
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).expect("scratch");
        let mut paths = String::new();
        let mut blobs = Vec::new();
        for (i, f) in files.iter().enumerate() {
            if f.mode == "160000" {
                continue;
            }
            let p = scratch.join(i.to_string());
            std::fs::write(&p, &f.bytes).expect("write blob");
            paths.push_str(&p.to_string_lossy());
            paths.push('\n');
            blobs.push(i);
        }
        let oids = self.git(
            &["hash-object", "-w", "--stdin-paths"],
            Some(paths.as_bytes()),
        );
        let mut oids = oids.lines();
        let mut info = String::new();
        for (i, f) in files.iter().enumerate() {
            let oid = if f.mode == "160000" {
                f.bytes.iter().map(|b| *b as char).collect::<String>()
            } else {
                assert!(blobs.contains(&i));
                oids.next().expect("oid").to_owned()
            };
            info.push_str(&format!(
                "{} {}	{}
",
                f.mode, oid, f.path
            ));
        }
        let index = self.dir.join(".git").join("tmp-index");
        let _ = std::fs::remove_file(&index);
        let env = [("GIT_INDEX_FILE", index.as_path())];
        self.git_env(
            &["update-index", "--add", "--index-info"],
            Some(info.as_bytes()),
            &env,
        );
        let tree = self.git_env(&["write-tree"], None, &env);
        self.commit_tree(&tree)
    }

    /// Creates a commit for `tree` on top of the current `main`, if any, and moves `main`.
    pub fn commit_tree(&self, tree: &str) -> Commit {
        let head = Command::new("git")
            .arg("-C")
            .arg(&self.dir)
            .args(["rev-parse", "--verify", "--quiet", "refs/heads/main"])
            .env("GIT_CONFIG_GLOBAL", &self.config)
            .output()
            .expect("rev-parse");
        let mut args = vec!["commit-tree", tree, "-m", "test"];
        let parent_sha = String::from_utf8_lossy(&head.stdout).trim().to_owned();
        if head.status.success() && !parent_sha.is_empty() {
            args.extend(["-p", &parent_sha]);
        }
        let sha = self.git(&args, None);
        self.git(&["update-ref", "refs/heads/main", &sha], None);
        Commit::parse(&sha).expect("commit sha")
    }

    /// `git mktree --missing` of listing lines `"<mode> <type> <oid>\t<name>"`: for trees that
    /// `update-index` would refuse (names like `..` or `.git`).
    pub fn mktree(&self, lines: &[String]) -> String {
        let mut input = lines.join("\n");
        input.push('\n');
        self.git(&["mktree", "--missing"], Some(input.as_bytes()))
    }

    pub fn blob(&self, bytes: &[u8]) -> String {
        self.git(&["hash-object", "-w", "--stdin"], Some(bytes))
    }
}

/// A content pack for `lullshift.cards@1` under `dir/` (use "" for the repository root).
pub fn cards_pack(dir: &str, id: &str, version: &str, permissions: &[&str]) -> Vec<F> {
    let p = |name: &str| {
        if dir.is_empty() {
            name.to_owned()
        } else {
            format!("{dir}/{name}")
        }
    };
    vec![
        F::new(&p("manifest.json"), cards_manifest(id, version, permissions).to_string()),
        F::new(
            &p("deck.json"),
            json!({"title": "Facts", "cards": [{"front": "Honey keeps", "back": "It barely spoils."}]}).to_string(),
        ),
        F::new(&p("README.md"), "# Facts\n"),
    ]
}

pub fn cards_manifest(id: &str, version: &str, permissions: &[&str]) -> Value {
    json!({
        "schemaVersion": 1, "kind": "content", "id": id, "version": version, "name": "Facts",
        "description": "A few facts.", "category": "break", "minHostVersion": "0.1.0",
        "sdkVersion": "0.2.0", "permissions": permissions, "offline": true,
        "engine": "lullshift.cards@1", "data": "deck.json"
    })
}

pub fn app_manifest(id: &str) -> Value {
    json!({
        "schemaVersion": 1, "kind": "app", "id": id, "version": "1.0.0", "name": "Game",
        "category": "arcade", "minHostVersion": "0.1.0", "sdkVersion": "0.2.0",
        "permissions": [], "offline": true, "entrypoint": "index.html",
        "runtime": "isolated-web-v1",
        "interruptibility": {"pause": true, "resume": true, "persistState": true}
    })
}

/// The real Calm Cards engine schema from this repository.
pub fn cards_engine() -> AvailableEngine {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../activities/engines/calm-cards/data.schema.json");
    let schema: Value =
        serde_json::from_slice(&std::fs::read(path).expect("read schema")).expect("json");
    AvailableEngine {
        reference: "lullshift.cards@1".to_owned(),
        schema,
    }
}

/// A marketplace index listing `entries` (`(id, source json)`).
pub fn index_json(name: &str, entries: &[(&str, Value)]) -> String {
    json!({
        "schemaVersion": 1, "name": name,
        "activities": entries.iter().map(|(id, s)| json!({"id": id, "source": s})).collect::<Vec<_>>()
    })
    .to_string()
}

pub fn index_file(name: &str, entries: &[(&str, Value)]) -> F {
    F::new(".lullshift/marketplace.json", index_json(name, entries))
}

/// Fetches the marketplace repo `src`, reads its index, resolves `id` and prepares the pack:
/// the whole install path up to (not including) writing files.
pub fn pipeline(
    env: &Env,
    src: &Src,
    id: &str,
    builtin: bool,
) -> Result<crate::pack::PreparedPack, crate::Error> {
    use crate::repo::MarketplaceRef;
    let cache = env.path("cache/market/repo.git");
    let commit = env.git.fetch(&cache, &src.url(), None)?;
    let index = env.git.read_index(&cache, &commit, builtin)?;
    let entry = index.entry(id).expect("the index lists the entry").clone();
    let ext = env.path("cache/external");
    let url = src.url();
    let market = MarketplaceRef {
        repo: &cache,
        commit: &commit,
        url: &url,
        external_cache_root: &ext,
    };
    let resolved = env.git.resolve_entry(&market, &entry)?;
    env.git.prepare_pack(
        &resolved,
        &crate::pack::PrepareRequest {
            marketplace: &index.name,
            builtin,
            entry_id: id,
            engines: &[cards_engine()],
        },
    )
}

/// Commits `files` plus an index listing `id` at `path`, then runs [`pipeline`].
pub fn local_pipeline(
    env: &Env,
    mut files: Vec<F>,
    id: &str,
    path: &str,
) -> Result<crate::pack::PreparedPack, crate::Error> {
    let src = env.src("src");
    files.push(index_file("acme-tools", &[(id, json!({"path": path}))]));
    src.commit(&files);
    pipeline(env, &src, id, false)
}
