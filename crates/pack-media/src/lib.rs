//! Pictures in content packs (ADR-016 amendment, S12-T11).
//!
//! A pack may list up to [`MAX_IMAGES`] PNG or WebP files in its manifest's `images`, each in
//! the pack's `images/` folder. Every image is untrusted input, so before the host stores,
//! hashes for serving or serves one, [`check_image`]:
//!
//! 1. checks the path ([`valid_image_path`]), the size (at most [`MAX_IMAGE_BYTES`]) and that the
//!    leading bytes match the file's extension;
//! 2. walks the container (PNG chunks, WebP RIFF chunks) and refuses anything but the chunks a
//!    still picture needs: no animation (APNG `acTL`/`fcTL`/`fdAT`, WebP `ANIM`/`ANMF` or the
//!    animation flag), no metadata (text, EXIF, XMP, time stamps), no colour profiles, no unknown
//!    chunks, nothing after the end of the image;
//! 3. reads the width and height from the header and refuses anything over
//!    [`MAX_DIMENSION`] on either side **before** a decoder allocates a pixel buffer;
//! 4. decodes the whole image with a memory-safe decoder (`png`, `image-webp`; both
//!    `forbid(unsafe_code)`) under an allocation limit, and refuses it on any error.
//!
//! The bytes are not re-encoded: the bytes that pass are exactly the bytes that are hashed,
//! installed and served, and the refusals in step 2 already keep metadata out.
//!
//! [`image_references`] finds every string in a pack's data that names an image, so the host
//! and the CLI can require that each one is declared in the manifest.

use std::io::Cursor;

use serde_json::Value;

/// Most images in one pack.
pub const MAX_IMAGES: usize = 20;
/// Largest image file.
pub const MAX_IMAGE_BYTES: usize = 512 * 1024;
/// Largest width or height, in pixels.
pub const MAX_DIMENSION: u32 = 2048;
/// Largest decoded pixel buffer: 2048 x 2048 at 16 bits per channel, RGBA.
const MAX_DECODED_BYTES: usize = 2048 * 2048 * 8;
/// Folder that holds a pack's images (and the path prefix the engine uses for them).
pub const IMAGE_DIR: &str = "images/";

/// The two image formats a pack may carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageKind {
    Png,
    Webp,
}

impl ImageKind {
    /// The content type the host serves this image with.
    #[must_use]
    pub fn content_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Webp => "image/webp",
        }
    }
}

/// A checked image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageInfo {
    pub kind: ImageKind,
    pub width: u32,
    pub height: u32,
}

/// Why an image was refused. `Display` is plain words for creators and the install review.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageError {
    BadPath,
    TooLarge,
    WrongType,
    TooManyPixels { width: u32, height: u32 },
    Animated,
    Metadata(String),
    Malformed(&'static str),
    Undecodable,
}

impl std::fmt::Display for ImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadPath => write!(
                f,
                "must be a .png or .webp file directly in images/, named with lowercase letters, digits and dashes"
            ),
            Self::TooLarge => write!(f, "is over 512 KiB"),
            Self::WrongType => write!(f, "isn't the PNG or WebP image its name says"),
            Self::TooManyPixels { width, height } => write!(
                f,
                "is {width} x {height} pixels; the most is {MAX_DIMENSION} x {MAX_DIMENSION}"
            ),
            Self::Animated => write!(f, "is animated; only still pictures are allowed"),
            Self::Metadata(what) => write!(
                f,
                "carries {what}; save it without metadata or a colour profile"
            ),
            Self::Malformed(why) => write!(f, "is damaged ({why})"),
            Self::Undecodable => write!(f, "can't be decoded"),
        }
    }
}

impl std::error::Error for ImageError {}

/// `images/<name>.png` or `images/<name>.webp`, the name in lowercase letters, digits and
/// dashes (1 to 64 characters, starting with a letter or digit). Nothing else can name an image,
/// so a path can never leave the folder, hide, or differ only by case.
#[must_use]
pub fn valid_image_path(path: &str) -> bool {
    image_kind(path).is_some()
}

fn image_kind(path: &str) -> Option<ImageKind> {
    let name = path.strip_prefix(IMAGE_DIR)?;
    let (stem, ext) = name.rsplit_once('.')?;
    let kind = match ext {
        "png" => ImageKind::Png,
        "webp" => ImageKind::Webp,
        _ => return None,
    };
    let ok = (1..=64).contains(&stem.len())
        && stem
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        && stem
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    ok.then_some(kind)
}

/// Checks one pack image fully (see the module docs). The path decides the expected format.
pub fn check_image(path: &str, bytes: &[u8]) -> Result<ImageInfo, ImageError> {
    let kind = image_kind(path).ok_or(ImageError::BadPath)?;
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(ImageError::TooLarge);
    }
    let (width, height) = match kind {
        ImageKind::Png => png_structure(bytes)?,
        ImageKind::Webp => webp_structure(bytes)?,
    };
    if width == 0 || height == 0 {
        return Err(ImageError::Malformed("it has no pixels"));
    }
    if width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(ImageError::TooManyPixels { width, height });
    }
    match kind {
        ImageKind::Png => decode_png(bytes, width, height)?,
        ImageKind::Webp => decode_webp(bytes, width, height)?,
    }
    Ok(ImageInfo {
        kind,
        width,
        height,
    })
}

const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
/// Chunks a still PNG may carry besides IHDR, IDAT and IEND. Colour profiles (`iCCP`) are
/// compressed and can be large, so they are refused with the metadata.
const PNG_ALLOWED: &[&[u8; 4]] = &[
    b"PLTE", b"tRNS", b"gAMA", b"cHRM", b"sRGB", b"sBIT", b"bKGD", b"pHYs",
];

fn be32(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

/// Walks the PNG chunks without decompressing anything. Returns the size from IHDR.
fn png_structure(bytes: &[u8]) -> Result<(u32, u32), ImageError> {
    if bytes.len() < 8 || &bytes[..8] != PNG_SIGNATURE {
        return Err(ImageError::WrongType);
    }
    let mut at = 8;
    let mut size = None;
    let mut seen_idat = false;
    let mut idat_done = false;
    let mut first = true;
    loop {
        if bytes.len() - at < 12 {
            return Err(ImageError::Malformed("it ends in the middle of a chunk"));
        }
        let len = be32(&bytes[at..]) as usize;
        let kind: [u8; 4] = bytes[at + 4..at + 8].try_into().unwrap_or_default();
        if len > bytes.len() - at - 12 {
            return Err(ImageError::Malformed("a chunk is longer than the file"));
        }
        let body = &bytes[at + 8..at + 8 + len];
        at += 12 + len;
        if first {
            if &kind != b"IHDR" || len != 13 {
                return Err(ImageError::Malformed("it doesn't start with a header"));
            }
            size = Some((be32(body), be32(&body[4..])));
            first = false;
            continue;
        }
        match &kind {
            b"IHDR" => return Err(ImageError::Malformed("it has two headers")),
            b"IDAT" => {
                if idat_done {
                    return Err(ImageError::Malformed("its image data is split up"));
                }
                seen_idat = true;
            }
            b"IEND" => {
                if !seen_idat {
                    return Err(ImageError::Malformed("it has no image data"));
                }
                if at != bytes.len() {
                    return Err(ImageError::Malformed("it has data after the end"));
                }
                return size.ok_or(ImageError::Malformed("it has no header"));
            }
            b"acTL" | b"fcTL" | b"fdAT" => return Err(ImageError::Animated),
            b"tEXt" | b"zTXt" | b"iTXt" => return Err(ImageError::Metadata("text".into())),
            b"eXIf" => return Err(ImageError::Metadata("EXIF data".into())),
            b"tIME" => return Err(ImageError::Metadata("a time stamp".into())),
            b"iCCP" => return Err(ImageError::Metadata("a colour profile".into())),
            k if PNG_ALLOWED.contains(&k) => {}
            _ => return Err(ImageError::Metadata("an unknown chunk".into())),
        }
        // Any chunk after the image data closes it: IDAT chunks must be consecutive.
        if &kind != b"IDAT" && seen_idat {
            idat_done = true;
        }
    }
}

fn decode_png(bytes: &[u8], width: u32, height: u32) -> Result<(), ImageError> {
    let limits = png::Limits {
        bytes: MAX_DECODED_BYTES,
    };
    let mut reader = png::Decoder::new_with_limits(Cursor::new(bytes), limits)
        .read_info()
        .map_err(|_| ImageError::Undecodable)?;
    if reader.info().size() != (width, height) {
        return Err(ImageError::Malformed(
            "its header disagrees with the decoder",
        ));
    }
    if reader.info().is_animated() {
        return Err(ImageError::Animated);
    }
    let need = reader
        .output_buffer_size()
        .filter(|n| *n <= MAX_DECODED_BYTES)
        .ok_or(ImageError::TooManyPixels { width, height })?;
    let mut buf = vec![0; need];
    reader
        .next_frame(&mut buf)
        .map_err(|_| ImageError::Undecodable)?;
    reader.finish().map_err(|_| ImageError::Undecodable)?;
    Ok(())
}

fn le24(b: &[u8]) -> u32 {
    u32::from(b[0]) | u32::from(b[1]) << 8 | u32::from(b[2]) << 16
}

/// Walks the WebP RIFF chunks. Returns the canvas size.
fn webp_structure(bytes: &[u8]) -> Result<(u32, u32), ImageError> {
    if bytes.len() < 20 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WEBP" {
        return Err(ImageError::WrongType);
    }
    let riff = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
    if riff.checked_add(8) != Some(bytes.len()) {
        return Err(ImageError::Malformed(
            "its RIFF size doesn't match the file",
        ));
    }
    let mut at = 12;
    let mut size: Option<(u32, u32)> = None;
    let mut extended = false;
    let mut bitstream = false;
    let mut index = 0;
    while at < bytes.len() {
        if bytes.len() - at < 8 {
            return Err(ImageError::Malformed("it ends in the middle of a chunk"));
        }
        let kind: [u8; 4] = bytes[at..at + 4].try_into().unwrap_or_default();
        let len = u32::from_le_bytes([bytes[at + 4], bytes[at + 5], bytes[at + 6], bytes[at + 7]])
            as usize;
        let padded = len + (len & 1);
        if padded > bytes.len() - at - 8 {
            return Err(ImageError::Malformed("a chunk is longer than the file"));
        }
        let body = &bytes[at + 8..at + 8 + len];
        at += 8 + padded;
        match &kind {
            b"VP8X" => {
                if index != 0 || len < 10 {
                    return Err(ImageError::Malformed("its extended header is misplaced"));
                }
                let flags = body[0];
                if flags & 0x02 != 0 {
                    return Err(ImageError::Animated);
                }
                if flags & 0x20 != 0 {
                    return Err(ImageError::Metadata("a colour profile".into()));
                }
                if flags & 0x0c != 0 {
                    return Err(ImageError::Metadata("EXIF or XMP data".into()));
                }
                size = Some((le24(&body[4..]) + 1, le24(&body[7..]) + 1));
                extended = true;
            }
            b"ALPH" => {
                if !extended || bitstream {
                    return Err(ImageError::Malformed("its alpha chunk is misplaced"));
                }
            }
            b"VP8 " => {
                if bitstream || len < 10 {
                    return Err(ImageError::Malformed("it has a broken picture chunk"));
                }
                // Frame tag (3 bytes), start code, then 14-bit width and height.
                if body[3..6] != [0x9d, 0x01, 0x2a] {
                    return Err(ImageError::Malformed("it has a broken picture chunk"));
                }
                let w = u32::from(u16::from_le_bytes([body[6], body[7]]) & 0x3fff);
                let h = u32::from(u16::from_le_bytes([body[8], body[9]]) & 0x3fff);
                bitstream = true;
                check_canvas(&mut size, extended, (w, h))?;
            }
            b"VP8L" => {
                if bitstream || len < 5 || body[0] != 0x2f {
                    return Err(ImageError::Malformed("it has a broken picture chunk"));
                }
                let bits = u32::from_le_bytes([body[1], body[2], body[3], body[4]]);
                let w = (bits & 0x3fff) + 1;
                let h = ((bits >> 14) & 0x3fff) + 1;
                bitstream = true;
                check_canvas(&mut size, extended, (w, h))?;
            }
            b"ANIM" | b"ANMF" => return Err(ImageError::Animated),
            b"EXIF" | b"XMP " => return Err(ImageError::Metadata("EXIF or XMP data".into())),
            b"ICCP" => return Err(ImageError::Metadata("a colour profile".into())),
            _ => return Err(ImageError::Metadata("an unknown chunk".into())),
        }
        index += 1;
    }
    if !bitstream {
        return Err(ImageError::Malformed("it has no picture"));
    }
    size.ok_or(ImageError::Malformed("it has no size"))
}

/// The picture's own size must match the extended header's canvas, if there is one.
fn check_canvas(
    size: &mut Option<(u32, u32)>,
    extended: bool,
    own: (u32, u32),
) -> Result<(), ImageError> {
    if extended {
        if *size != Some(own) {
            return Err(ImageError::Malformed("its picture and canvas sizes differ"));
        }
    } else {
        *size = Some(own);
    }
    Ok(())
}

fn decode_webp(bytes: &[u8], width: u32, height: u32) -> Result<(), ImageError> {
    // `new` reads only headers; our own walk has already refused every chunk the picture
    // doesn't need. The crate applies its memory limit to metadata reads only (which we never
    // make); its pixel buffers scale with the dimensions, which were capped before this call.
    let mut decoder =
        image_webp::WebPDecoder::new(Cursor::new(bytes)).map_err(|_| ImageError::Undecodable)?;
    decoder.set_memory_limit(MAX_DECODED_BYTES);
    if decoder.dimensions() != (width, height) {
        return Err(ImageError::Malformed(
            "its header disagrees with the decoder",
        ));
    }
    if decoder.is_animated() {
        return Err(ImageError::Animated);
    }
    let need = decoder
        .output_buffer_size()
        .filter(|n| *n <= MAX_DECODED_BYTES)
        .ok_or(ImageError::TooManyPixels { width, height })?;
    let mut buf = vec![0; need];
    decoder
        .read_image(&mut buf)
        .map_err(|_| ImageError::Undecodable)?;
    Ok(())
}

/// Every string in `data` that looks like an image reference (starts with `images/`), in
/// document order. The host and CLI require each to be a declared image.
#[must_use]
pub fn image_references(data: &Value) -> Vec<&str> {
    fn walk<'a>(v: &'a Value, out: &mut Vec<&'a str>) {
        match v {
            Value::String(s) if s.starts_with(IMAGE_DIR) => out.push(s),
            Value::Array(l) => l.iter().for_each(|x| walk(x, out)),
            Value::Object(o) => o.values().for_each(|x| walk(x, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(data, &mut out);
    out
}

/// Checks a pack's declared images as a set: count, unique paths, each a valid image path, and
/// every reference in `data` declared. The files themselves are checked by [`check_image`].
pub fn check_declared(images: &[String], data: &Value) -> Result<(), String> {
    if images.len() > MAX_IMAGES {
        return Err(format!("a pack may list at most {MAX_IMAGES} images"));
    }
    for (i, p) in images.iter().enumerate() {
        if !valid_image_path(p) {
            return Err(format!("{} {}", shown(p), ImageError::BadPath));
        }
        if images[..i].contains(p) {
            return Err(format!("{} is listed twice", shown(p)));
        }
    }
    for r in image_references(data) {
        if !images.iter().any(|p| p == r) {
            return Err(format!(
                "the data names {}, which isn't in the manifest's images",
                shown(r)
            ));
        }
    }
    Ok(())
}

/// A path for messages: at most 80 characters, anything outside printable ASCII as `?`.
fn shown(p: &str) -> String {
    p.chars()
        .take(80)
        .map(|c| {
            if c.is_ascii_graphic() || c == ' ' {
                c
            } else {
                '?'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests;
