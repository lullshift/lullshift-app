//! Activity manifest checks shared by the desktop host and the marketplace installer.
//!
//! Moved out of the desktop crate (Sprint 08) so a pack read from Git is judged by exactly the
//! same rules as a bundled one. The desktop crate imports these; its behaviour is unchanged.

use serde::{Deserialize, Serialize};

/// The subset of `reference/ACTIVITY_MANIFEST_V1.schema.json` the host relies on. Unknown fields
/// are accepted here (the schema is checked in `pnpm check`), but every field used is checked.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub schema_version: u32,
    pub kind: String,
    pub id: String,
    pub version: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub author: Option<String>,
    pub category: String,
    /// Untrusted labels (`^[a-z0-9-]{1,30}$`, checked on install). The UI only matches them
    /// against its own fixed tag map; they never become labels.
    #[serde(default)]
    pub tags: Vec<String>,
    pub min_host_version: String,
    pub sdk_version: String,
    pub permissions: Vec<String>,
    pub entrypoint: Option<String>,
    pub runtime: Option<String>,
    pub interruptibility: Option<Interruptibility>,
    pub help: Option<Help>,
    pub preview: Option<String>,
    /// Content packs only: `id@major` of the engine that runs the pack.
    pub engine: Option<String>,
    /// Content packs only: the data file, relative to the pack folder.
    pub data: Option<String>,
    /// Content packs only: pictures the engine may show (`images/<name>.png|webp`, at most 20).
    #[serde(default)]
    pub images: Vec<String>,
}

/// How-to-play help (S05-T07). Plain text only; the UI renders it as text.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Help {
    pub summary: String,
    pub goal: Option<String>,
    pub how_to_play: Vec<String>,
    #[serde(default)]
    pub controls: Vec<HelpControl>,
    #[serde(default)]
    pub tips: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HelpControl {
    pub keys: String,
    pub action: String,
}

impl Help {
    /// The same limits as the manifest schema's `help`.
    fn is_valid(&self) -> bool {
        let list = |items: &[String], min: usize, max_items: usize, max: usize| {
            items.len() >= min && items.len() <= max_items && items.iter().all(|t| plain(t, max))
        };
        plain(&self.summary, 200)
            && self.goal.as_deref().is_none_or(|g| plain(g, 300))
            && list(&self.how_to_play, 1, 8, 300)
            && self.controls.len() <= 12
            && self
                .controls
                .iter()
                .all(|c| plain(&c.keys, 40) && plain(&c.action, 120))
            && list(&self.tips, 0, 6, 200)
    }
}

/// Plain text of 1 to `max` characters with no control, bidirectional-override or invisible
/// formatting characters. Mirrors the manifest schema and `isPlainText` in the SDK.
pub fn plain(text: &str, max: usize) -> bool {
    !text.is_empty() && text_ok(text, max)
}

/// Like [`plain`] but the text may be empty. The one check the index and manifests share:
/// length, no control or invisible characters, no long run of stacked combining marks.
pub fn text_ok(text: &str, max: usize) -> bool {
    text.chars().count() <= max && !text.chars().any(hidden_char) && !long_mark_run(text)
}

/// Most combining marks allowed in a row (stacking more smears text over its neighbours).
const MAX_MARK_RUN: usize = 4;

/// True if `text` has more than [`MAX_MARK_RUN`] combining marks in a row.
pub fn long_mark_run(text: &str) -> bool {
    let mut run = 0;
    for c in text.chars() {
        if combining_mark(c) {
            run += 1;
            if run > MAX_MARK_RUN {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}

fn combining_mark(c: char) -> bool {
    matches!(c,
        '\u{300}'..='\u{36F}' | '\u{483}'..='\u{489}' | '\u{591}'..='\u{5BD}'
        | '\u{610}'..='\u{61A}' | '\u{64B}'..='\u{65F}' | '\u{1AB0}'..='\u{1AFF}'
        | '\u{1DC0}'..='\u{1DFF}' | '\u{20D0}'..='\u{20FF}' | '\u{FE20}'..='\u{FE2F}')
}

/// Control and invisible characters. Also covers the line and paragraph separators, filler
/// characters that render as blanks and the interlinear annotation controls (review L6); the
/// set matches `agent-core`'s `is_invisible` plus the control ranges.
pub fn hidden_char(c: char) -> bool {
    matches!(c,
        '\u{0}'..='\u{1F}' | '\u{7F}'..='\u{9F}' | '\u{AD}' | '\u{34F}' | '\u{61C}'
        | '\u{115F}'..='\u{1160}' | '\u{17B4}'..='\u{17B5}' | '\u{180B}'..='\u{180E}'
        | '\u{200B}'..='\u{200F}' | '\u{2028}'..='\u{202E}' | '\u{2060}'..='\u{2064}'
        | '\u{2066}'..='\u{206F}' | '\u{3164}' | '\u{FEFF}' | '\u{FFA0}'
        | '\u{FFF9}'..='\u{FFFB}' | '\u{1BCA0}'..='\u{1BCA3}' | '\u{1D173}'..='\u{1D17A}'
        | '\u{E0000}'..='\u{E007F}')
}

/// A relative file path whose segments start with a letter, digit or underscore (so no `.`,
/// `..`, hidden names or leading `-`), at most 8 deep, ending in one of `exts`.
pub fn valid_rel_path(path: &str, exts: &[&str]) -> bool {
    let segments: Vec<&str> = path.split('/').collect();
    let ext_ok = path
        .rsplit_once('.')
        .is_some_and(|(_, e)| exts.contains(&e));
    path.len() <= 256
        && segments.len() <= 8
        && ext_ok
        && segments.iter().all(|s| {
            s.len() <= 64
                && s.bytes()
                    .next()
                    .is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_')
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        })
}

/// `MAJOR.MINOR.PATCH` with an optional pre-release, as in the manifest schema.
pub fn valid_semver(v: &str) -> bool {
    let (core, pre) = v.split_once('-').map_or((v, None), |(c, p)| (c, Some(p)));
    let nums: Vec<&str> = core.split('.').collect();
    v.len() <= 64
        && nums.len() == 3
        && nums
            .iter()
            .all(|n| (1..=5).contains(&n.len()) && n.bytes().all(|b| b.is_ascii_digit()))
        && pre.is_none_or(|p| {
            (1..=40).contains(&p.len())
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-'))
        })
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Interruptibility {
    pub pause: bool,
    pub resume: bool,
    pub persist_state: bool,
}

/// `id@major` as in the manifest schema: a manifest id, `@`, and a positive major.
pub fn valid_engine_ref(reference: &str) -> bool {
    reference.split_once('@').is_some_and(|(id, major)| {
        valid_id(id)
            && (1..=4).contains(&major.len())
            && major.bytes().all(|b| b.is_ascii_digit())
            && !major.starts_with('0')
    })
}

/// A manifest the host can run: a sandboxed web app that supports host-owned interruption, or
/// a content pack that names an engine and a data file.
pub fn parse_manifest(json: &str) -> Option<Manifest> {
    let m: Manifest = serde_json::from_str(json).ok()?;
    let interruptible = m
        .interruptibility
        .as_ref()
        .is_some_and(|i| i.pause && i.resume && i.persist_state);
    let shape = match m.kind.as_str() {
        "app" => {
            m.entrypoint.as_deref() == Some("index.html")
                && m.runtime.as_deref() == Some("isolated-web-v1")
                && interruptible
                && m.engine.is_none()
                && m.data.is_none()
                && m.images.is_empty()
        }
        "content" => {
            m.entrypoint.is_none()
                && m.runtime.is_none()
                && m.interruptibility.is_none()
                && m.engine.as_deref().is_some_and(valid_engine_ref)
                && m.data
                    .as_deref()
                    .is_some_and(|d| valid_rel_path(d, &["json"]))
                && m.images.len() <= pack_media::MAX_IMAGES
                && m.images
                    .iter()
                    .enumerate()
                    .all(|(i, p)| pack_media::valid_image_path(p) && !m.images[..i].contains(p))
        }
        _ => false,
    };
    let ok = m.schema_version == 1
        && shape
        && valid_id(&m.id)
        && plain(&m.name, 100)
        && (m.description.is_empty() || plain(&m.description, 500))
        && m.author
            .as_deref()
            .is_none_or(|a| a.is_empty() || plain(a, 100))
        && valid_semver(&m.version)
        && valid_semver(&m.min_host_version)
        && valid_semver(&m.sdk_version)
        && m.help.as_ref().is_none_or(Help::is_valid)
        && m.preview
            .as_deref()
            .is_none_or(|p| valid_rel_path(p, &["png", "jpg", "jpeg", "webp", "svg"]))
        && ["arcade", "break", "productivity", "interactive"].contains(&m.category.as_str())
        && m.permissions.iter().all(|p| p == "activity.storage");
    ok.then_some(m)
}

/// Manifest ID pattern `^[a-z0-9]+(\.[a-z0-9-]+)+$`, at most 160 characters.
pub fn valid_id(id: &str) -> bool {
    let mut parts = id.split('.');
    let first_ok = parts.next().is_some_and(|p| {
        !p.is_empty()
            && p.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
    });
    let rest: Vec<&str> = parts.collect();
    id.len() <= 160
        && first_ok
        && !rest.is_empty()
        && rest.iter().all(|p| {
            !p.is_empty()
                && p.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}
