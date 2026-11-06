use super::*;
use serde_json::json;

/// A valid RGB PNG of the given size, encoded with the `png` crate.
fn png_bytes(width: u32, height: u32) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, width, height);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().expect("header");
        let pixels: Vec<u8> = (0..width * height * 3).map(|i| (i % 251) as u8).collect();
        w.write_image_data(&pixels).expect("data");
    }
    out
}

/// A valid lossless WebP of the given size, encoded with `image-webp`.
fn webp_bytes(width: u32, height: u32) -> Vec<u8> {
    let mut out = Vec::new();
    let pixels: Vec<u8> = (0..width * height * 4).map(|i| (i % 253) as u8).collect();
    image_webp::WebPEncoder::new(&mut out)
        .encode(&pixels, width, height, image_webp::ColorType::Rgba8)
        .expect("webp");
    out
}

fn crc(kind: &[u8], body: &[u8]) -> [u8; 4] {
    let mut h = crc32fast::Hasher::new();
    h.update(kind);
    h.update(body);
    h.finalize().to_be_bytes()
}

/// `png` with an extra chunk inserted right after IHDR (offset 33).
fn with_png_chunk(png: &[u8], kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = png[..33].to_vec();
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(body);
    out.extend_from_slice(&crc(kind, body));
    out.extend_from_slice(&png[33..]);
    out
}

/// A WebP with a VP8X header (given flags) in front of the bitstream of `simple`.
fn extended_webp(simple: &[u8], flags: u8, extra: Option<(&[u8; 4], &[u8])>) -> Vec<u8> {
    // The simple file is RIFF header (12 bytes), then "VP8L", its length and the 0x2f signature;
    // width and height follow as two 14-bit fields.
    let info = {
        let bits = u32::from_le_bytes([simple[21], simple[22], simple[23], simple[24]]);
        ((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1)
    };
    let mut body = Vec::new();
    body.extend_from_slice(b"VP8X");
    body.extend_from_slice(&10u32.to_le_bytes());
    body.push(flags);
    body.extend_from_slice(&[0, 0, 0]);
    body.extend_from_slice(&(info.0 - 1).to_le_bytes()[..3]);
    body.extend_from_slice(&(info.1 - 1).to_le_bytes()[..3]);
    if let Some((kind, data)) = extra {
        body.extend_from_slice(kind);
        body.extend_from_slice(&(data.len() as u32).to_le_bytes());
        body.extend_from_slice(data);
        if data.len() % 2 == 1 {
            body.push(0);
        }
    }
    body.extend_from_slice(&simple[12..]);
    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&((body.len() + 4) as u32).to_le_bytes());
    out.extend_from_slice(b"WEBP");
    out.extend_from_slice(&body);
    out
}

#[test]
fn good_png_and_webp_pass() {
    let png = png_bytes(64, 40);
    let info = check_image("images/a-1.png", &png).expect("png");
    assert_eq!(
        info,
        ImageInfo {
            kind: ImageKind::Png,
            width: 64,
            height: 40
        }
    );
    assert_eq!(info.kind.content_type(), "image/png");
    let webp = webp_bytes(30, 20);
    let info = check_image("images/b.webp", &webp).expect("webp");
    assert_eq!(
        info,
        ImageInfo {
            kind: ImageKind::Webp,
            width: 30,
            height: 20
        }
    );
    assert_eq!(info.kind.content_type(), "image/webp");
    // A plain VP8X wrapper (alpha flag only) is fine.
    check_image("images/c.webp", &extended_webp(&webp, 0x10, None)).expect("vp8x");
    check_image("images/max.png", &png_bytes(2048, 1)).expect("edge");
}

#[test]
fn paths_are_strict() {
    for bad in [
        "a.png",
        "images/../a.png",
        "images/sub/a.png",
        "images/A.png",
        "images/.a.png",
        "images/-a.png",
        "images/a.PNG",
        "images/a.svg",
        "images/a.gif",
        "images/a.jpg",
        "images/a b.png",
        "images/a\\b.png",
        "/images/a.png",
        "images/.png",
        "images/é.png",
    ] {
        assert!(!valid_image_path(bad), "{bad}");
        assert_eq!(check_image(bad, &png_bytes(1, 1)), Err(ImageError::BadPath));
    }
    let long = format!("images/{}.png", "a".repeat(65));
    assert!(!valid_image_path(&long));
    assert!(valid_image_path(&format!("images/{}.png", "a".repeat(64))));
}

#[test]
fn wrong_type_svg_and_disguised_files_are_refused() {
    let png = png_bytes(4, 4);
    let webp = webp_bytes(4, 4);
    assert_eq!(
        check_image("images/a.webp", &png),
        Err(ImageError::WrongType)
    );
    assert_eq!(
        check_image("images/a.png", &webp),
        Err(ImageError::WrongType)
    );
    let svg = br#"<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>"#;
    assert_eq!(check_image("images/a.png", svg), Err(ImageError::WrongType));
    let html = b"<!doctype html><script>fetch('x')</script>";
    assert_eq!(
        check_image("images/a.webp", html),
        Err(ImageError::WrongType)
    );
    assert_eq!(check_image("images/a.png", b""), Err(ImageError::WrongType));
    // A PNG signature followed by HTML.
    let mut polyglot = PNG_SIGNATURE.to_vec();
    polyglot.extend_from_slice(b"<html><script>alert(1)</script></html>");
    assert!(check_image("images/a.png", &polyglot).is_err());
}

#[test]
fn size_limits_are_checked_before_decoding() {
    let big = vec![0u8; MAX_IMAGE_BYTES + 1];
    assert_eq!(check_image("images/a.png", &big), Err(ImageError::TooLarge));
    assert_eq!(
        check_image("images/a.png", &png_bytes(2049, 1)),
        Err(ImageError::TooManyPixels {
            width: 2049,
            height: 1
        })
    );
    assert_eq!(
        check_image("images/a.webp", &webp_bytes(1, 2049)),
        Err(ImageError::TooManyPixels {
            width: 1,
            height: 2049
        })
    );
    // A tiny file whose header claims a huge image: refused from the header alone, without
    // the decoder allocating anything.
    let mut liar = png_bytes(1, 1);
    liar[16..20].copy_from_slice(&100_000u32.to_be_bytes());
    liar[20..24].copy_from_slice(&100_000u32.to_be_bytes());
    let fixed = crc(b"IHDR", &liar[16..29]);
    liar[29..33].copy_from_slice(&fixed);
    assert_eq!(
        check_image("images/a.png", &liar),
        Err(ImageError::TooManyPixels {
            width: 100_000,
            height: 100_000
        })
    );
    let mut zero = png_bytes(1, 1);
    zero[16..20].copy_from_slice(&0u32.to_be_bytes());
    assert!(check_image("images/a.png", &zero).is_err());
}

#[test]
fn png_animation_metadata_and_unknown_chunks_are_refused() {
    let png = png_bytes(8, 8);
    let actl = with_png_chunk(&png, b"acTL", &[0, 0, 0, 1, 0, 0, 0, 0]);
    assert_eq!(
        check_image("images/a.png", &actl),
        Err(ImageError::Animated)
    );
    for (kind, body) in [
        (b"tEXt", b"Comment\0hi".as_slice()),
        (b"iTXt", b"x\0\0\0\0\0y".as_slice()),
        (b"zTXt", b"x\0\0abc".as_slice()),
        (b"eXIf", b"MM\0*".as_slice()),
        (b"tIME", &[7, 234, 1, 1, 0, 0, 0]),
        (b"iCCP", b"p\0\0abc".as_slice()),
        (b"prVt", b"anything".as_slice()),
    ] {
        let bytes = with_png_chunk(&png, kind, body);
        assert!(
            matches!(
                check_image("images/a.png", &bytes),
                Err(ImageError::Metadata(_))
            ),
            "{}",
            String::from_utf8_lossy(kind)
        );
    }
    // A harmless ancillary chunk is fine.
    let phys = with_png_chunk(&png, b"pHYs", &[0, 0, 11, 19, 0, 0, 11, 19, 1]);
    check_image("images/a.png", &phys).expect("pHYs");
}

#[test]
fn damaged_pngs_are_refused() {
    let png = png_bytes(16, 16);
    // Data after IEND (for example an appended script or archive).
    let mut trailing = png.clone();
    trailing.extend_from_slice(b"<script>alert(1)</script>");
    assert!(matches!(
        check_image("images/a.png", &trailing),
        Err(ImageError::Malformed(_))
    ));
    // Truncated.
    assert!(check_image("images/a.png", &png[..png.len() - 20]).is_err());
    // A flipped bit in the compressed pixels: the CRC or the inflate catches it.
    let mut flipped = png.clone();
    let idat = flipped.windows(4).position(|w| w == b"IDAT").expect("idat");
    flipped[idat + 6] ^= 0xff;
    assert!(check_image("images/a.png", &flipped).is_err());
    // A chunk claiming to be longer than the file.
    let mut long = png.clone();
    long[33..37].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(check_image("images/a.png", &long).is_err());
    // No IHDR first.
    let mut no_header = png.clone();
    no_header[12..16].copy_from_slice(b"IHDX");
    assert!(check_image("images/a.png", &no_header).is_err());
}

#[test]
fn webp_animation_metadata_and_damage_are_refused() {
    let webp = webp_bytes(10, 10);
    assert_eq!(
        check_image("images/a.webp", &extended_webp(&webp, 0x02, None)),
        Err(ImageError::Animated)
    );
    assert!(matches!(
        check_image("images/a.webp", &extended_webp(&webp, 0x08, None)),
        Err(ImageError::Metadata(_))
    ));
    assert!(matches!(
        check_image("images/a.webp", &extended_webp(&webp, 0x20, None)),
        Err(ImageError::Metadata(_))
    ));
    for kind in [b"EXIF", b"XMP ", b"ICCP", b"zzzz"] {
        let bytes = extended_webp(&webp, 0, Some((kind, b"abc")));
        assert!(
            matches!(
                check_image("images/a.webp", &bytes),
                Err(ImageError::Metadata(_))
            ),
            "{}",
            String::from_utf8_lossy(kind)
        );
    }
    let anim = extended_webp(&webp, 0, Some((b"ANIM", &[0, 0, 0, 0, 0, 0])));
    assert_eq!(
        check_image("images/a.webp", &anim),
        Err(ImageError::Animated)
    );
    // RIFF size that doesn't match, data after the end, truncation.
    let mut wrong_size = webp.clone();
    wrong_size[4] ^= 0x01;
    assert!(check_image("images/a.webp", &wrong_size).is_err());
    let mut trailing = webp.clone();
    trailing.extend_from_slice(b"<script>");
    assert!(check_image("images/a.webp", &trailing).is_err());
    assert!(check_image("images/a.webp", &webp[..webp.len() / 2]).is_err());
    // Corrupt pixels: the decoder refuses.
    let mut flipped = webp.clone();
    let last = flipped.len() - 3;
    for b in &mut flipped[25..last] {
        *b ^= 0x5a;
    }
    assert!(check_image("images/a.webp", &flipped).is_err());
}

#[test]
fn references_must_be_declared() {
    let data = json!({
        "questions": [
            {"prompt": "x", "image": {"src": "images/a.png", "alt": "A"}},
            {"prompt": "y", "image": {"src": "images/b.webp", "alt": "B"}},
            {"prompt": "z"}
        ],
        "note": "images are nice"
    });
    assert_eq!(
        image_references(&data),
        vec!["images/a.png", "images/b.webp"]
    );
    let both = vec!["images/a.png".to_owned(), "images/b.webp".to_owned()];
    check_declared(&both, &data).expect("declared");
    let err = check_declared(&both[..1], &data).expect_err("undeclared");
    assert!(err.contains("images/b.webp"), "{err}");
    let dup = vec!["images/a.png".to_owned(), "images/a.png".to_owned()];
    assert!(check_declared(&dup, &json!({})).is_err());
    let bad = vec!["images/../x.png".to_owned()];
    assert!(check_declared(&bad, &json!({})).is_err());
    let many: Vec<String> = (0..=MAX_IMAGES)
        .map(|i| format!("images/i{i}.png"))
        .collect();
    assert!(check_declared(&many, &json!({})).is_err());
    // Odd text in a reference is shown cleaned.
    let err = check_declared(&[], &json!({"src": "images/\u{202e}x.png"})).expect_err("x");
    assert!(!err.contains('\u{202e}'), "{err}");
}
