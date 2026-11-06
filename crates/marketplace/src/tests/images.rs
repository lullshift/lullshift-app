//! S12-T11: pictures in third-party content packs get the same checks at install as when the
//! app loads a pack (`pack-media`): declared, in `images/`, PNG or WebP, still, no metadata,
//! within the size and pixel limits, and fully decodable.

use std::path::PathBuf;

use serde_json::json;

use crate::error::{Error, PackError};
use crate::testutil::*;

const ID: &str = "com.example.pictures";
const DIR: &str = "packs/pics";

/// A real image from the bundled example pack.
fn bundled(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../activities/content/quiz/quiz-shapes-and-sides/images")
        .join(name);
    std::fs::read(path).expect("bundled image")
}

/// A Calm Cards pack declaring `images` and holding `extra` files (paths inside the pack).
fn pack(images: &[&str], extra: Vec<(&str, Vec<u8>)>, back: &str) -> Vec<F> {
    let mut manifest = cards_manifest(ID, "1.0.0", &[]);
    if !images.is_empty() {
        manifest["images"] = json!(images);
    }
    let mut files = vec![
        F::new(&format!("{DIR}/manifest.json"), manifest.to_string()),
        F::new(
            &format!("{DIR}/deck.json"),
            json!({"title": "Shapes", "cards": [{"front": "A shape", "back": back}]}).to_string(),
        ),
    ];
    for (path, bytes) in extra {
        files.push(F::new(&format!("{DIR}/{path}"), bytes));
    }
    files
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

fn image_err(files: Vec<F>) -> String {
    match pack_err(files) {
        PackError::ImageInvalid(why) => why,
        other => panic!("expected an image error, got {other:?}"),
    }
}

#[test]
fn declared_png_and_webp_images_install() {
    let p = run(pack(
        &["images/octagon.png", "images/pentagon.webp"],
        vec![
            ("images/octagon.png", bundled("octagon.png")),
            ("images/pentagon.webp", bundled("pentagon.webp")),
        ],
        "Eight sides",
    ))
    .expect("installs");
    let paths: Vec<&str> = p.files.iter().map(|f| f.path.as_str()).collect();
    assert!(paths.contains(&"images/octagon.png") && paths.contains(&"images/pentagon.webp"));
    assert_eq!(p.preview.file_count, 4);
}

#[test]
fn undeclared_or_missing_images_are_refused() {
    // A file in images/ the manifest doesn't list is not something a pack may carry.
    let undeclared = pack(
        &[],
        vec![("images/octagon.png", bundled("octagon.png"))],
        "x",
    );
    assert!(matches!(pack_err(undeclared), PackError::DisallowedFile(_)));
    // Declared but absent.
    assert!(matches!(
        pack_err(pack(&["images/octagon.png"], vec![], "x")),
        PackError::DataMissing(_)
    ));
    // The data names a picture the manifest doesn't list.
    let why = image_err(pack(
        &["images/octagon.png"],
        vec![("images/octagon.png", bundled("octagon.png"))],
        "images/secret.png",
    ));
    assert!(why.contains("images/secret.png"), "{why}");
}

#[test]
fn disguised_animated_and_damaged_images_are_refused() {
    let png = bundled("octagon.png");
    let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" onload="alert(1)"/>"#.to_vec();
    let html = b"<!doctype html><script>fetch('https://example.com')</script>".to_vec();
    let mut apng = png[..33].to_vec();
    apng.extend_from_slice(&8u32.to_be_bytes());
    apng.extend_from_slice(b"acTL");
    apng.extend_from_slice(&[0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0]);
    apng.extend_from_slice(&png[33..]);
    let mut trailing = png.clone();
    trailing.extend_from_slice(b"PK\x03\x04 an appended archive");
    let truncated = png[..png.len() / 2].to_vec();
    for (name, bytes, expect) in [
        ("svg", svg, "isn't the PNG or WebP image"),
        ("html", html, "isn't the PNG or WebP image"),
        ("apng", apng, "animated"),
        ("trailing", trailing, "after the end"),
        ("truncated", truncated, "damaged"),
        (
            "webp-as-png",
            bundled("pentagon.webp"),
            "isn't the PNG or WebP image",
        ),
    ] {
        let why = image_err(pack(&["images/a.png"], vec![("images/a.png", bytes)], "x"));
        assert!(why.contains(expect), "{name}: {why}");
    }
}

#[test]
fn oversized_images_are_refused_before_they_are_read() {
    let mut big = bundled("octagon.png");
    big.resize(pack_media::MAX_IMAGE_BYTES + 1, 0);
    let why = image_err(pack(&["images/a.png"], vec![("images/a.png", big)], "x"));
    assert!(why.contains("512 KiB"), "{why}");
}

#[test]
fn bad_image_lists_make_the_manifest_invalid() {
    for images in [
        json!(["images/../deck.png"]),
        json!(["images/sub/a.png"]),
        json!(["pictures/a.png"]),
        json!(["images/a.svg"]),
        json!(["images/A.png"]),
        json!(["images/a.png", "images/a.png"]),
        json!(
            (0..21)
                .map(|i| format!("images/i{i}.png"))
                .collect::<Vec<_>>()
        ),
    ] {
        let mut files = pack(&[], vec![], "x");
        let mut manifest = cards_manifest(ID, "1.0.0", &[]);
        manifest["images"] = images.clone();
        files[0] = F::new(&format!("{DIR}/manifest.json"), manifest.to_string());
        assert!(
            matches!(pack_err(files), PackError::ManifestInvalid(_)),
            "{images}"
        );
    }
    // Code activities can't declare pack images.
    let mut app = app_manifest(ID);
    app["images"] = json!(["images/a.png"]);
    assert!(crate::manifest::parse_manifest(&app.to_string()).is_none());
}
