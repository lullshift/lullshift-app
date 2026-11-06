//! S08-T05: malicious repositories are rejected, with a specific reason.

use serde_json::json;

use crate::error::{Error, GitFailureKind, IndexError, PackError, PathError};
use crate::testutil::*;

const ID: &str = "com.evil.pack";
const DIR: &str = "packs/x";

fn good() -> Vec<F> {
    cards_pack(DIR, ID, "1.0.0", &[])
}

fn run(files: Vec<F>) -> Result<crate::pack::PreparedPack, Error> {
    local_pipeline(&Env::new(), files, ID, DIR)
}

fn pack_err(files: Vec<F>) -> PackError {
    match run(files) {
        Err(Error::Pack(e)) => e,
        other => panic!("expected a pack error, got {:?}", other.map(|p| p.preview)),
    }
}

#[test]
fn the_control_pack_is_accepted() {
    let p = run(good()).expect("a plain pack installs");
    assert_eq!(p.preview.file_count, 3);
}

#[test]
fn symlink_entry() {
    let mut f = good();
    f.push(F::new("packs/x/link.txt", "deck.json").mode("120000"));
    assert!(matches!(pack_err(f), PackError::Symlink(p) if p.ends_with("link.txt")));
}

#[test]
fn symlink_pointing_outside() {
    let mut f = good();
    f.push(F::new("packs/x/notes.md", "../../../../etc/passwd").mode("120000"));
    assert!(matches!(pack_err(f), PackError::Symlink(_)));
}

#[test]
fn submodule_entry() {
    let mut f = good();
    f.push(F::new("packs/x/vendor", "1".repeat(40)).mode("160000"));
    assert!(matches!(pack_err(f), PackError::Submodule(p) if p.ends_with("vendor")));
}

#[test]
fn executable_bit() {
    let mut f = good();
    f[2] = F::new("packs/x/README.md", "# x\n").mode("100755");
    assert!(matches!(pack_err(f), PackError::Executable(_)));
}

/// `git update-index` refuses `..` and `.git`, so these trees are made with `git mktree`.
fn pack_with_raw_entry(env: &Env, name: &str, is_tree: bool) -> crate::validate::Commit {
    let src = env.src("src");
    let readme = src.blob(b"# x\n");
    let manifest = src.blob(cards_manifest(ID, "1.0.0", &[]).to_string().as_bytes());
    let deck = src.blob(
        json!({"title": "T", "cards": [{"front": "f"}]})
            .to_string()
            .as_bytes(),
    );
    let evil = if is_tree {
        let inner = src.mktree(&[format!("100644 blob {readme}\tconfig")]);
        format!("040000 tree {inner}\t{name}")
    } else {
        format!("100644 blob {readme}\t{name}")
    };
    let x = src.mktree(&[
        format!("100644 blob {manifest}\tmanifest.json"),
        format!("100644 blob {deck}\tdeck.json"),
        evil,
    ]);
    let packs = src.mktree(&[format!("040000 tree {x}\tx")]);
    let idx = src.blob(index_json("acme-tools", &[(ID, json!({"path": DIR}))]).as_bytes());
    let dot = src.mktree(&[format!("100644 blob {idx}\tmarketplace.json")]);
    let root = src.mktree(&[
        format!("040000 tree {dot}\t.lullshift"),
        format!("040000 tree {packs}\tpacks"),
    ]);
    src.commit_tree(&root)
}

fn raw_entry_error(name: &str, is_tree: bool) -> PackError {
    let env = Env::lenient();
    pack_with_raw_entry(&env, name, is_tree);
    match pipeline(&env, &env.src("src"), ID, false) {
        Err(Error::Pack(e)) => e,
        other => panic!(
            "{name}: expected a pack error, got {:?}",
            other.map(|p| p.preview)
        ),
    }
}

#[test]
fn dot_dot_entry_made_by_mktree() {
    assert!(matches!(
        raw_entry_error("..", false),
        PackError::BadPath(_, PathError::BadSegment)
    ));
}

#[test]
fn dot_dot_directory_made_by_mktree() {
    assert!(matches!(
        raw_entry_error("..", true),
        PackError::BadPath(_, PathError::BadSegment)
    ));
}

#[test]
fn dot_git_directory_made_by_mktree() {
    assert!(matches!(
        raw_entry_error(".git", true),
        PackError::BadPath(_, PathError::GitDir)
    ));
    assert!(matches!(
        raw_entry_error(".GIT", true),
        PackError::BadPath(_, PathError::GitDir)
    ));
}

#[test]
fn drive_letter_and_absolute_style_names() {
    for name in ["C:evil", "C:", "\\evil", "a\\b"] {
        assert!(
            matches!(
                raw_entry_error(name, false),
                PackError::BadPath(_, PathError::ForbiddenCharacter)
            ),
            "{name}"
        );
    }
}

#[test]
fn dot_git_file_in_a_pack() {
    let mut f = good();
    f.push(F::new(
        "packs/x/.gitattributes",
        "* filter=evil
",
    ));
    assert!(matches!(
        pack_err(f),
        PackError::BadPath(_, PathError::ForbiddenCharacter)
    ));
}

/// Git for Windows refuses to index some of these names, so they are made with `git mktree`.
#[test]
fn windows_reserved_and_trailing_dot_names() {
    for (name, why) in [
        ("con.txt", PathError::WindowsReserved),
        ("NUL", PathError::WindowsReserved),
        ("com1.md", PathError::WindowsReserved),
        ("note.", PathError::ForbiddenCharacter),
        ("-rf.md", PathError::LooksLikeOption),
        ("café.md", PathError::ForbiddenCharacter),
        ("a b.md", PathError::ForbiddenCharacter),
    ] {
        assert!(
            matches!(raw_entry_error(name, false), PackError::BadPath(_, w) if w == why),
            "{name}"
        );
    }
}

#[test]
fn case_collision() {
    let mut f = good();
    f.push(F::new("packs/x/Deck.json", "{}"));
    assert!(matches!(
        pack_err(f),
        PackError::BadPath(_, PathError::CaseCollision)
    ));
    let mut f = good();
    f.push(F::new("packs/x/readme.MD", "x"));
    assert!(matches!(
        pack_err(f),
        PackError::BadPath(_, PathError::CaseCollision)
    ));
}

#[test]
fn oversized_file() {
    let mut f = good();
    f.push(F::new("packs/x/big.txt", vec![b'a'; 5 * 1024 * 1024 + 1]));
    assert!(matches!(pack_err(f), PackError::FileTooLarge(_)));
}

#[test]
fn oversized_pack() {
    let mut f = good();
    for i in 0..5 {
        f.push(F::new(
            &format!("packs/x/part{i}.txt"),
            vec![b'a' + i as u8; 4_800_000],
        ));
    }
    assert_eq!(pack_err(f), PackError::PackTooLarge);
}

#[test]
fn too_many_files() {
    let mut f = good();
    for i in 0..510 {
        f.push(F::new(&format!("packs/x/notes/n{i}.txt"), "x"));
    }
    assert_eq!(
        f.iter().filter(|x| x.path.starts_with("packs/x")).count(),
        513
    );
    assert_eq!(pack_err(f), PackError::TooManyFiles);
}

#[test]
fn exactly_512_files_is_fine() {
    let mut f = good();
    for i in 0..509 {
        f.push(F::new(&format!("packs/x/notes/n{i}.txt"), "x"));
    }
    assert_eq!(run(f).expect("512 files").preview.file_count, 512);
}

#[test]
fn too_deep() {
    let mut f = good();
    f.push(F::new("packs/x/a/b/c/d/e/f/g/h/i.txt", "x"));
    assert!(matches!(
        pack_err(f),
        PackError::BadPath(_, PathError::TooDeep)
    ));
}

/// Git cannot hold some shapes (a file and a folder with one name, modes other than the four
/// it normalizes to), so the listing check is also tested on hand-made listings.
#[test]
fn listing_check_on_hand_made_entries() {
    use crate::pack::check_listing;
    use crate::repo::TreeEntry;
    let e = |mode: &str, kind: &str, path: &str| TreeEntry {
        mode: mode.to_owned(),
        kind: kind.to_owned(),
        oid: "0".repeat(40),
        size: 1,
        path: path.to_owned(),
    };
    assert!(check_listing(&[e("100644", "blob", "a.md")]).is_ok());
    assert!(matches!(
        check_listing(&[e("100664", "blob", "a.md")]),
        Err(PackError::UnsupportedMode(_))
    ));
    assert!(matches!(
        check_listing(&[e("100644", "tree", "a.md")]),
        Err(PackError::UnsupportedMode(_))
    ));
    assert!(matches!(
        check_listing(&[e("100644", "commit", "a")]),
        Err(PackError::Submodule(_))
    ));
    assert!(matches!(
        check_listing(&[
            e("100644", "blob", "notes.md"),
            e("100644", "blob", "notes.md/more.md")
        ]),
        Err(PackError::BadPath(_, PathError::CaseCollision))
    ));
    assert!(matches!(
        check_listing(&[e("100644", "blob", "a.md"), e("100644", "blob", "a.md")]),
        Err(PackError::BadPath(_, PathError::CaseCollision))
    ));
    // Normalization-style collisions are impossible: non-ASCII names are refused outright.
    assert!(matches!(
        check_listing(&[e("100644", "blob", "é.md"), e("100644", "blob", "é.md")]),
        Err(PackError::BadPath(_, PathError::ForbiddenCharacter))
    ));
}

fn index_error(index: &str) -> Error {
    let env = Env::new();
    let src = env.src("src");
    let mut f = good();
    f.push(F::new(".lullshift/marketplace.json", index));
    src.commit(&f);
    pipeline_index_only(&env, &src).expect_err("the index is refused")
}

fn pipeline_index_only(env: &Env, src: &Src) -> Result<(), Error> {
    let cache = env.path("cache/market/repo.git");
    let commit = env.git.fetch(&cache, &src.url(), None)?;
    env.git.read_index(&cache, &commit, false).map(|_| ())
}

#[test]
fn bad_index_json() {
    for bad in ["{not json", "", "[]", "null", "{\"schemaVersion\":1}"] {
        assert!(
            matches!(index_error(bad), Error::Index(IndexError::Parse(_))),
            "{bad}"
        );
    }
}

#[test]
fn index_with_unknown_fields() {
    let e =
        index_error(r#"{"schemaVersion":1,"name":"acme","activities":[],"onInstall":"rm -rf"}"#);
    assert!(matches!(e, Error::Index(IndexError::Parse(_))));
    let e = index_error(
        r#"{"schemaVersion":1,"name":"acme","activities":[{"id":"a.b","source":{"path":"x"},"script":"x"}]}"#,
    );
    assert!(matches!(e, Error::Index(IndexError::Parse(_))));
}

#[test]
fn index_with_credentials_in_a_url() {
    let e = index_error(
        // secretlint-disable-next-line -- fake credential: the test checks it is refused
        r#"{"schemaVersion":1,"name":"acme","activities":[{"id":"a.b","source":{"git":"https://user:SECRETTOKEN@example.com/o/r"}}]}"#,
    );
    assert!(matches!(e, Error::Index(IndexError::Entry { .. })));
    assert!(!e.to_string().contains("SECRETTOKEN") && !format!("{e:?}").contains("SECRETTOKEN"));
}

#[test]
fn index_that_claims_the_reserved_name() {
    let e = index_error(r#"{"schemaVersion":1,"name":"lullshift","activities":[]}"#);
    assert!(matches!(
        e,
        Error::Index(IndexError::Name(crate::error::NameError::Reserved))
    ));
}

#[test]
fn missing_and_symlinked_index() {
    let env = Env::new();
    let src = env.src("src");
    src.commit(&good());
    assert_eq!(
        pipeline_index_only(&env, &src),
        Err(Error::Index(IndexError::Missing))
    );

    let env = Env::new();
    let src = env.src("src");
    let mut f = good();
    f.push(F::new(".lullshift/marketplace.json", "../packs/x/manifest.json").mode("120000"));
    src.commit(&f);
    assert_eq!(
        pipeline_index_only(&env, &src),
        Err(Error::Index(IndexError::NotARegularFile))
    );

    let env = Env::new();
    let src = env.src("src");
    let mut f = good();
    f.push(F::new(".lullshift", "packs").mode("120000"));
    src.commit(&f);
    assert_eq!(
        pipeline_index_only(&env, &src),
        Err(Error::Index(IndexError::Missing))
    );
}

#[test]
fn oversized_index() {
    let big = format!(
        r#"{{"schemaVersion":1,"name":"acme","description":"{}","activities":[]}}"#,
        "a".repeat(1024 * 1024)
    );
    assert_eq!(index_error(&big), Error::Index(IndexError::TooLarge));
}

#[test]
fn manifest_id_differs_from_the_index_entry() {
    let f = cards_pack(DIR, "com.evil.other", "1.0.0", &[]);
    match run(f) {
        Err(Error::Pack(PackError::IdMismatch { expected, found })) => {
            assert_eq!((expected.as_str(), found.as_str()), (ID, "com.evil.other"));
        }
        other => panic!("{:?}", other.map(|p| p.preview)),
    }
}

#[test]
fn content_pack_with_code() {
    for name in [
        "packs/x/run.js",
        "packs/x/index.html",
        "packs/x/style.css",
        "packs/x/tool.exe",
        "packs/x/noext",
        "packs/x/extra.json",
    ] {
        let mut f = good();
        f.push(F::new(name, "alert(1)"));
        assert!(
            matches!(pack_err(f), PackError::DisallowedFile(_)),
            "{name}"
        );
    }
}

#[test]
fn unknown_permissions() {
    for perm in [
        "network",
        "fs.read",
        "clipboard",
        "activity.storage.extra",
        "",
    ] {
        let f = cards_pack(DIR, ID, "1.0.0", &[perm]);
        assert!(
            matches!(pack_err(f), PackError::UnknownPermission(_)),
            "{perm}"
        );
    }
    let f = cards_pack(DIR, ID, "1.0.0", &["activity.storage", "activity.storage"]);
    assert!(matches!(pack_err(f), PackError::ManifestInvalid(_)));
}

#[test]
fn missing_engine() {
    let mut f = good();
    let mut m = cards_manifest(ID, "1.0.0", &[]);
    m["engine"] = json!("lullshift.nope@1");
    f[0] = F::new("packs/x/manifest.json", m.to_string());
    assert!(matches!(pack_err(f), PackError::EngineMissing(e) if e == "lullshift.nope@1"));
    let mut f = good();
    let mut m = cards_manifest(ID, "1.0.0", &[]);
    m["engine"] = json!("lullshift.cards@2");
    f[0] = F::new("packs/x/manifest.json", m.to_string());
    assert!(matches!(pack_err(f), PackError::EngineMissing(_)));
}

#[test]
fn bad_manifests() {
    let mutate = |k: &str, v: serde_json::Value| {
        let mut f = good();
        let mut m = cards_manifest(ID, "1.0.0", &[]);
        m[k] = v;
        f[0] = F::new("packs/x/manifest.json", m.to_string());
        pack_err(f)
    };
    assert!(matches!(
        mutate("onInstall", json!("x")),
        PackError::ManifestInvalid(_)
    ));
    assert!(matches!(
        mutate("offline", json!(false)),
        PackError::ManifestInvalid(_)
    ));
    assert!(matches!(
        mutate("kind", json!("plugin")),
        PackError::ManifestInvalid(_)
    ));
    assert!(matches!(
        mutate("data", json!("../deck.json")),
        PackError::ManifestInvalid(_)
    ));
    assert!(matches!(
        mutate("data", json!("/etc/passwd")),
        PackError::ManifestInvalid(_)
    ));
    assert!(matches!(
        mutate("name", json!("a\u{202e}b")),
        PackError::ManifestInvalid(_)
    ));
    assert!(matches!(
        mutate("tags", json!(["UPPER"])),
        PackError::ManifestInvalid(_)
    ));
    assert!(matches!(
        mutate("entrypoint", json!("index.html")),
        PackError::ManifestInvalid(_)
    ));
    assert!(matches!(
        mutate("data", json!("missing.json")),
        PackError::DataMissing(_)
    ));

    let mut f = good();
    f[0] = F::new("packs/x/manifest.json", "{ nope");
    assert!(matches!(pack_err(f), PackError::ManifestInvalid(_)));
    let f: Vec<F> = good()
        .into_iter()
        .filter(|x| !x.path.ends_with("manifest.json"))
        .collect();
    assert_eq!(pack_err(f), PackError::MissingManifest);
}

#[test]
fn bad_content_data() {
    let mut f = good();
    f[1] = F::new(
        "packs/x/deck.json",
        json!({"title": "T", "cards": []}).to_string(),
    );
    assert!(matches!(pack_err(f), PackError::DataInvalid(p) if !p.is_empty()));
    let mut f = good();
    f[1] = F::new(
        "packs/x/deck.json",
        json!({"title": "T", "cards": [{"front": "f"}], "x": 1}).to_string(),
    );
    assert!(matches!(pack_err(f), PackError::DataInvalid(_)));
    let mut f = good();
    f[1] = F::new("packs/x/deck.json", "not json");
    assert!(matches!(pack_err(f), PackError::DataNotJson(_)));
    let mut f = good();
    f[1] = F::new("packs/x/deck.json", vec![b' '; 256 * 1024 + 1]);
    assert!(matches!(pack_err(f), PackError::DataTooLarge(_)));
}

#[test]
fn hidden_characters_and_mark_runs_in_pack_data_are_refused_at_install() {
    // Security review LOW-2: the same plain-text rule as manifests, for every string in the data.
    for text in [
        "tag\u{E0041}".to_owned(),
        "filler\u{3164}".to_owned(),
        "half\u{FFA0}".to_owned(),
        "anchor\u{FFF9}".to_owned(),
        "cgj\u{34F}".to_owned(),
        format!("e{}", "\u{301}".repeat(5)),
    ] {
        let mut f = good();
        f[1] = F::new(
            "packs/x/deck.json",
            json!({"title": "T", "cards": [{"front": text}]}).to_string(),
        );
        assert!(
            matches!(pack_err(f), PackError::DataInvalid(ref p) if p.iter().any(|l| l.contains("cards[0].front"))),
            "{text:?}"
        );
    }
}

#[test]
fn lfs_pointer_is_read_raw_and_refused() {
    let pointer = "version https://git-lfs.github.com/spec/v1\noid sha256:4d7a214614ab2935c943f9e0ff69d22eadbb8f32b1258daaa5e2ca24d17e2393\nsize 12345\n";
    let mut f = good();
    f[1] = F::new("packs/x/deck.json", pointer);
    assert!(matches!(pack_err(f), PackError::DataNotJson(_)));
}

#[test]
fn missing_or_non_folder_path() {
    let env = Env::new();
    let src = env.src("src");
    let mut f = good();
    f.push(index_file(
        "acme-tools",
        &[(ID, json!({"path": "packs/nowhere"}))],
    ));
    src.commit(&f);
    assert_eq!(
        pipeline(&env, &src, ID, false).err(),
        Some(Error::Pack(PackError::NotFound))
    );

    let env = Env::new();
    let src = env.src("src");
    let mut f = good();
    f.push(index_file(
        "acme-tools",
        &[(ID, json!({"path": "packs/x/manifest.json"}))],
    ));
    src.commit(&f);
    assert_eq!(
        pipeline(&env, &src, ID, false).err(),
        Some(Error::Pack(PackError::NotFound))
    );
}

#[test]
fn third_party_code_activity_is_blocked() {
    let app = || {
        vec![
            F::new("packs/x/manifest.json", app_manifest(ID).to_string()),
            F::new("packs/x/index.html", "<!doctype html><title>x</title>"),
            F::new("packs/x/main.js", "console.log(1)"),
        ]
    };
    assert_eq!(pack_err(app()), PackError::ThirdPartyCodeBlocked);
    assert!(!crate::third_party_code_allowed());

    // The built-in marketplace's own apps are not third-party code.
    let env = Env::new();
    let src = env.src("src");
    let mut f = app();
    f.push(index_file("lullshift", &[(ID, json!({"path": DIR}))]));
    src.commit(&f);
    let p = pipeline(&env, &src, ID, true).expect("built-in app");
    assert!(!p.preview.third_party);
}

#[test]
fn third_party_code_is_blocked_before_its_files_are_read() {
    // An app with an executable file and a symlink: the gate answers first only if the listing
    // is fine; the listing check still runs first and names the real problem.
    let mut f = vec![
        F::new("packs/x/manifest.json", app_manifest(ID).to_string()),
        F::new("packs/x/index.html", "x"),
    ];
    f.push(F::new("packs/x/l.js", "x").mode("120000"));
    assert!(matches!(pack_err(f), PackError::Symlink(_)));
}

#[test]
fn nonexistent_repository_is_not_found() {
    let env = Env::new();
    let missing = crate::validate::GitUrl::for_test_file(&env.path("no-such-repo"));
    let e = env
        .git
        .fetch(&env.path("cache/m/repo.git"), &missing, None)
        .expect_err("fails");
    assert_eq!(e.git_kind(), Some(GitFailureKind::NotFound), "{e}");
}

#[test]
fn nonexistent_ref_is_not_found() {
    let env = Env::new();
    let src = env.src("src");
    src.commit(&good());
    let r = crate::validate::GitRef::parse("no-such-branch").expect("syntax ok");
    let e = env
        .git
        .fetch(&env.path("cache/m/repo.git"), &src.url(), Some(&r))
        .expect_err("fails");
    assert_eq!(e.git_kind(), Some(GitFailureKind::NotFound), "{e}");
}

#[test]
fn errors_never_contain_git_stderr_or_the_path() {
    let env = Env::new();
    let missing = crate::validate::GitUrl::for_test_file(&env.path("secret-folder-name"));
    let e = env
        .git
        .fetch(&env.path("cache/m/repo.git"), &missing, None)
        .expect_err("fails");
    let text = format!("{e} {e:?}");
    assert!(
        !text.contains("secret-folder-name") && !text.contains("fatal"),
        "{text}"
    );
}
