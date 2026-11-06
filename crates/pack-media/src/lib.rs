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
//!    chunks, nothing after the end of the image. For PNG it also inflates the image data under
//!    a hard cap and requires the zlib stream to end exactly where the concatenated `IDAT` data
//!    ends and to hold exactly the pixel bytes the header promises (so no extra compressed
//!    data, and no bytes after the stream end, can hide inside `IDAT`);
//! 3. reads the width and height from the header and refuses anything over
//!    [`MAX_DIMENSION`] on either side **before** a decoder allocates a pixel buffer;
//! 4. decodes the whole image with a memory-safe decoder (`png`, `image-webp`; both
//!    `forbid(unsafe_code)`) under an allocation limit, and refuses it on any error.
//!
//! The bytes are not re-encoded: the bytes that pass are exactly the bytes that are hashed,
//! installed and served, and the refusals in step 2 already keep metadata out.
//!
//! What is **not** detected (WebP): a `VP8 ` (lossy) chunk's last partition has no stored
//! length, so bytes after the real data are indistinguishable from data. For `VP8L` the decoder
//! does not report the exact end of its bitstream; the check reads how far the decoder pulled
//! from the chunk and refuses more than 8 unread bytes (its look-ahead), so up to 8 trailing
//! bytes in a `VP8L` chunk, and any in a `VP8 ` chunk, can pass. They are inert (no decoder or
//! browser reads them, they cannot be a second image or metadata chunk, the RIFF and chunk
//! lengths must add up exactly, and the whole file is capped at [`MAX_IMAGE_BYTES`]) but they
//! are not refused.
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
    // Structure first (no decompression), then size limits, then the strict stream check, and
    // only then a full decode.
    let (width, height, png, vp8l_end) = match kind {
        ImageKind::Png => {
            let m = png_structure(bytes)?;
            (m.width, m.height, Some(m), None)
        }
        ImageKind::Webp => {
            let (w, h, end) = webp_structure(bytes)?;
            (w, h, None, end)
        }
    };
    if width == 0 || height == 0 {
        return Err(ImageError::Malformed("it has no pixels"));
    }
    if width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(ImageError::TooManyPixels { width, height });
    }
    match png {
        Some(m) => {
            check_idat_stream(&m)?;
            decode_png(bytes, width, height)?;
        }
        None => decode_webp(bytes, width, height, vp8l_end)?,
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

/// What the PNG walk found.
struct PngMeta {
    width: u32,
    height: u32,
    /// The concatenated `IDAT` bodies: the zlib stream.
    idat: Vec<u8>,
    /// The inflated size the header promises (filter bytes and padding included).
    raw_len: u64,
}

/// Bytes of inflated image data for these IHDR fields, or `None` for a combination PNG forbids.
fn png_raw_len(width: u32, height: u32, depth: u8, color: u8, interlace: u8) -> Option<u64> {
    let channels: u64 = match (color, depth) {
        (0, 1 | 2 | 4 | 8 | 16) | (3, 1 | 2 | 4 | 8) => 1,
        (4, 8 | 16) => 2,
        (2, 8 | 16) => 3,
        (6, 8 | 16) => 4,
        _ => return None,
    };
    let bits = channels * u64::from(depth);
    let pass = |w: u64, h: u64| -> u64 {
        if w == 0 || h == 0 {
            0
        } else {
            h * (1 + (w * bits).div_ceil(8))
        }
    };
    let (w, h) = (u64::from(width), u64::from(height));
    match interlace {
        0 => Some(pass(w, h)),
        1 => {
            // Adam7: (x offset, y offset, x step, y step) per pass.
            const PASSES: [(u64, u64, u64, u64); 7] = [
                (0, 0, 8, 8),
                (4, 0, 8, 8),
                (0, 4, 4, 8),
                (2, 0, 4, 4),
                (0, 2, 2, 4),
                (1, 0, 2, 2),
                (0, 1, 1, 2),
            ];
            Some(
                PASSES
                    .iter()
                    .map(|&(x0, y0, dx, dy)| {
                        pass(
                            w.saturating_sub(x0).div_ceil(dx),
                            h.saturating_sub(y0).div_ceil(dy),
                        )
                    })
                    .sum(),
            )
        }
        _ => None,
    }
}

/// Inflates the concatenated `IDAT` data under a hard cap and requires the zlib stream to end
/// exactly at the end of that data after producing exactly the promised pixel bytes. Extra
/// compressed data (for example a zero-filled tail that inflates to hundreds of megabytes) and
/// raw bytes after the stream end are both refused. At most `raw_len + 64 KiB` bytes are ever
/// produced, in a fixed 64 KiB buffer, and the loop stops at the first byte over `raw_len`.
fn check_idat_stream(m: &PngMeta) -> Result<(), ImageError> {
    use flate2::{Decompress, FlushDecompress, Status};
    let mut inflater = Decompress::new(true);
    let mut out = vec![0u8; 64 * 1024];
    loop {
        let (in_before, out_before) = (inflater.total_in(), inflater.total_out());
        let start = usize::try_from(in_before).map_err(|_| ImageError::Undecodable)?;
        let status = inflater
            .decompress(&m.idat[start..], &mut out, FlushDecompress::None)
            .map_err(|_| ImageError::Undecodable)?;
        if inflater.total_out() > m.raw_len {
            return Err(ImageError::Malformed(
                "its image data holds more than the pixels",
            ));
        }
        match status {
            Status::StreamEnd => break,
            Status::Ok | Status::BufError => {
                if inflater.total_in() == in_before && inflater.total_out() == out_before {
                    // No progress: the stream is cut short.
                    return Err(ImageError::Undecodable);
                }
            }
        }
    }
    if inflater.total_out() != m.raw_len {
        return Err(ImageError::Malformed(
            "its image data is shorter than the header says",
        ));
    }
    if inflater.total_in() != m.idat.len() as u64 {
        return Err(ImageError::Malformed(
            "it has data after the end of its image data",
        ));
    }
    Ok(())
}

/// Walks the PNG chunks without decompressing anything.
fn png_structure(bytes: &[u8]) -> Result<PngMeta, ImageError> {
    if bytes.len() < 8 || &bytes[..8] != PNG_SIGNATURE {
        return Err(ImageError::WrongType);
    }
    let mut at = 8;
    let mut head: Option<(u32, u32, u64)> = None;
    let mut idat: Vec<u8> = Vec::new();
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
            let (w, h) = (be32(body), be32(&body[4..]));
            // A header over our limits is refused by size before the stream check, so only
            // bound the arithmetic here.
            let raw = if w > MAX_DIMENSION || h > MAX_DIMENSION {
                0
            } else {
                png_raw_len(w, h, body[8], body[9], body[12])
                    .ok_or(ImageError::Malformed("its header is invalid"))?
            };
            head = Some((w, h, raw));
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
                idat.extend_from_slice(body);
            }
            b"IEND" => {
                if !seen_idat {
                    return Err(ImageError::Malformed("it has no image data"));
                }
                if at != bytes.len() {
                    return Err(ImageError::Malformed("it has data after the end"));
                }
                let (width, height, raw_len) =
                    head.ok_or(ImageError::Malformed("it has no header"))?;
                return Ok(PngMeta {
                    width,
                    height,
                    idat,
                    raw_len,
                });
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
fn webp_structure(bytes: &[u8]) -> Result<(u32, u32, Option<usize>), ImageError> {
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
    let mut vp8l_end: Option<usize> = None;
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
        if padded != len && bytes[at + 8 + len] != 0 {
            return Err(ImageError::Malformed("a chunk's padding byte isn't zero"));
        }
        let chunk_end = at + 8 + len;
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
                vp8l_end = Some(chunk_end);
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
    let (w, h) = size.ok_or(ImageError::Malformed("it has no size"))?;
    Ok((w, h, vp8l_end))
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

fn decode_webp(
    bytes: &[u8],
    width: u32,
    height: u32,
    vp8l_end: Option<usize>,
) -> Result<(), ImageError> {
    // `new` reads only headers; our own walk has already refused every chunk the picture
    // doesn't need. The crate applies its memory limit to metadata reads only (which we never
    // make); its pixel buffers scale with the dimensions, which were capped before this call.
    let mut cursor = Cursor::new(bytes);
    let mut decoder =
        image_webp::WebPDecoder::new(&mut cursor).map_err(|_| ImageError::Undecodable)?;
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
    drop(decoder);
    // The VP8L bit reader looks ahead at most 8 bytes, so if the decoder stopped pulling bytes
    // more than that short of the chunk's end, the rest is data the picture doesn't use.
    if let Some(end) = vp8l_end {
        let pulled = usize::try_from(cursor.position()).unwrap_or(usize::MAX);
        if end.saturating_sub(pulled) > 8 {
            return Err(ImageError::Malformed(
                "its picture chunk has unused data at the end",
            ));
        }
    }
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
