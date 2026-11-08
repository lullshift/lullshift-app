//! Sets written by the user's own Companion model (ADR-028), on the host side.
//!
//! A set is engine data, checked exactly like a content pack's, kept in the local store under a
//! host-made activity id `lullshift.generated.<engine short name>.<12 random a-z0-9>`. To the
//! engine it looks just like a pack: the `lullact` scheme serves the engine's files under the set
//! id (never a pack's pictures: sets have none), `activity_content` hands it the data, and
//! snapshots, storage and play time are keyed by the id, so resume needs nothing new.
//!
//! Trust model:
//! - the id namespace is reserved: installs, index entries and bundled manifests refuse it
//!   (`marketplace::manifest::is_reserved_generated_id`), so no other activity shares a set's id;
//! - the engine of a set is bundled and first-party only, named by the id's short name, and the
//!   stored engine must equal the one the id names (a mismatch marks the row unusable);
//! - the stored data is checked again on every load (`content`), with the same engine schema and
//!   plain-text checks as a pack plus "no pictures"; a row that fails is marked unusable and is
//!   neither handed to an engine nor has its engine files served;
//! - topic, data and model name are never logged; errors carry fixed words only.

use std::borrow::Cow;

use local_store::{GENERATED_ID_PREFIX, GeneratedSet, KEEP_GENERATED_SETS_PER_ENGINE};
use serde::Serialize;
use serde_json::Value;
use time::OffsetDateTime;

use super::{
    Activity, ActivityInfo, ContentError, DATA_MAX, FoundFile, Source, clean_tags, find_engine,
    has_storage, mime, split_path, validate_data, with_store,
};
use crate::Core;

/// Short names of the engines that may have model-written sets, with the engine reference each
/// stands for (ADR-028 section 2). The short name is the part of the set id after the prefix.
const SETS: [(&str, &str); 8] = [
    ("quiz", "lullshift.quiz@1"),
    ("teaser", "lullshift.teaser@1"),
    ("cards", "lullshift.cards@1"),
    ("reflection", "lullshift.reflection@1"),
    ("typing", "lullshift.typing@1"),
    ("movement", "lullshift.movement@1"),
    ("breathing", "lullshift.breathing@1"),
    ("pause", "lullshift.pause@1"),
];

/// Characters of the random part of a set id.
const RANDOM_LEN: usize = 12;
/// Longest model name kept with a set, in characters (the badge shows at most 40 of them).
const MODEL_MAX: usize = 200;
/// Longest model name shown in `ActivityInfo` (ADR-028 section 8).
const MODEL_SHOWN_MAX: usize = 40;
const TOPIC_MAX: usize = 200;
const HOST_MAX: usize = 255;

/// The name shown for a set whose title can't be used.
const FALLBACK_NAME: &str = "Set from your model";

/// The engine reference (`id@major`) behind a short name, or None.
pub(crate) fn engine_ref(short: &str) -> Option<&'static str> {
    SETS.iter().find(|(s, _)| *s == short).map(|(_, e)| *e)
}

/// True for any id under the reserved namespace, valid or not. Such an id is only ever resolved
/// as a set, never as a bundled or installed activity.
pub(crate) fn is_set_id(id: &str) -> bool {
    id.starts_with(GENERATED_ID_PREFIX)
}

/// A parsed set id.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ParsedSetId {
    pub(crate) short: &'static str,
    pub(crate) engine: &'static str,
}

/// Splits `lullshift.generated.<short>.<12 random a-z0-9>`; anything else is None.
pub(crate) fn parse_set_id(id: &str) -> Option<ParsedSetId> {
    let rest = id.strip_prefix(GENERATED_ID_PREFIX)?;
    let (short, random) = rest.split_once('.')?;
    let (short, engine) = SETS.iter().find(|(s, _)| *s == short)?;
    let ok = random.len() == RANDOM_LEN
        && random
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit());
    ok.then_some(ParsedSetId { short, engine })
}

/// A fresh set id for an engine short name. Twelve characters from the operating system's random
/// number generator, drawn without modulo bias (36 symbols, bytes below 252 only).
pub(crate) fn new_set_id(short: &str) -> Result<String, String> {
    engine_ref(short).ok_or_else(|| "unknown_engine".to_owned())?;
    const ABC: &[u8; 36] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut random = String::with_capacity(RANDOM_LEN);
    while random.len() < RANDOM_LEN {
        let mut buf = [0_u8; 32];
        getrandom::fill(&mut buf).map_err(|_| "internal".to_owned())?;
        for b in buf {
            if b < 252 && random.len() < RANDOM_LEN {
                random.push(char::from(ABC[usize::from(b) % 36]));
            }
        }
    }
    Ok(format!("{GENERATED_ID_PREFIX}{short}.{random}"))
}

/// A set id resolved against the store: the bundled engine that runs it.
pub(crate) struct Handle {
    pub(crate) id: String,
    pub(crate) engine_ref: &'static str,
    pub(crate) engine: &'static Activity,
    /// Marked unusable: it fails the load-time check, so nothing is served for it.
    pub(crate) unusable: bool,
}

/// Looks a set id up. `unknown_activity` for an id that doesn't parse, doesn't exist, or whose
/// stored engine isn't the one the id names (the row is then marked unusable).
pub(crate) fn resolve(core: &Core, id: &str) -> Result<Handle, String> {
    let parsed = parse_set_id(id).ok_or_else(|| "unknown_activity".to_owned())?;
    let (stored, unusable) = with_store(core, |s| s.generated_set_state(id))?
        .ok_or_else(|| "unknown_activity".to_owned())?;
    if stored != parsed.engine {
        // A row can't change engine except by hand-editing the database.
        let _ = with_store(core, |s| s.mark_generated_set_unusable(id));
        return Err("unknown_activity".into());
    }
    let (engine, _) = find_engine(parsed.engine).map_err(|_| "internal".to_owned())?;
    Ok(Handle {
        id: id.to_owned(),
        engine_ref: parsed.engine,
        engine,
        unusable,
    })
}

/// The engine file for a request path `/<set id>/<file>`, or None. Only for a usable set, only
/// the engine's own files, never anything under `images/` (a set has no pictures).
pub(crate) fn serve(core: &Core, path: &str) -> Option<FoundFile> {
    let (id, file) = split_path(path)?;
    let content_type = mime(file)?;
    let handle = resolve(core, id).ok()?;
    if handle.unusable || file.starts_with(pack_media::IMAGE_DIR) {
        return None;
    }
    let found = handle
        .engine
        .dir
        .get_file(handle.engine.dir.path().join(file))?;
    Some((Cow::Borrowed(found.contents()), content_type))
}

pub(crate) fn unusable_error() -> ContentError {
    ContentError::new(
        "generated_unusable",
        "This set from your model can't be used any more. You can delete it in Settings. Nothing else is affected.",
    )
}

/// The checks every set's data passes, on acceptance and again on every load: the engine's data
/// schema and the plain-text rule (the same `validate_data` as a pack), and no picture
/// references (sets have no pictures). Problems are short sentences without the data's text.
fn data_problems(schema: &Value, data: &Value) -> Vec<String> {
    let mut problems = marketplace::pack_text::pack_data_problems(schema, data);
    if let Err(why) = pack_media::check_declared(&[], data) {
        problems.push(why);
    }
    problems
}

/// A set's data for `init.content`: the row is read and checked again now. A failure marks the
/// row unusable and gives the calm `generated_unusable` error.
pub(crate) fn content(core: &Core, handle: &Handle) -> Result<Value, ContentError> {
    if handle.unusable {
        return Err(unusable_error());
    }
    let internal = || ContentError::new("internal", "the check didn't finish");
    let row = with_store(core, |s| s.generated_set(&handle.id))
        .map_err(|_| internal())?
        .ok_or_else(|| ContentError::new("unknown_activity", ""))?;
    let (_, schema) = find_engine(handle.engine_ref)?;
    let checked = (row.engine == handle.engine_ref)
        .then(|| validate_data(schema, "the set", row.data_json.as_bytes()).ok())
        .flatten()
        .filter(|data| pack_media::check_declared(&[], data).is_ok())
        // Body routines are checked again against the generation rules (caps, refused phrases,
        // plain ASCII) on the model-written fields; the host-written ones are left alone.
        .filter(|data| {
            ::companion::generate::EngineKind::from_engine_ref(handle.engine_ref)
                .is_none_or(|e| !::companion::generate::body_problem(e, data))
        });
    match checked {
        Some(data) => apply_skips(core, &handle.id, handle.engine_ref, data),
        None => {
            let _ = with_store(core, |s| s.mark_generated_set_unusable(&handle.id));
            Err(unusable_error())
        }
    }
}

/// The array and the text field of each item, for the engines whose items the user can mark
/// "This looks wrong" (quiz and teaser only).
fn skippable(engine_ref: &str) -> Option<(&'static str, &'static str)> {
    match engine_ref {
        "lullshift.quiz@1" => Some(("questions", "prompt")),
        "lullshift.teaser@1" => Some(("teasers", "question")),
        _ => None,
    }
}

/// Removes the items the user marked wrong (ADR-028 section 8) from checked data before the
/// engine sees it. Skips take effect the next time a set starts. At least one item always stays;
/// if every item was marked, the set is refused with a calm message.
fn apply_skips(
    core: &Core,
    id: &str,
    engine_ref: &str,
    mut data: Value,
) -> Result<Value, ContentError> {
    let Some((list, _)) = skippable(engine_ref) else {
        return Ok(data);
    };
    let skips = with_store(core, |s| s.generated_skips(id))
        .map_err(|_| ContentError::new("internal", "the check didn't finish"))?;
    if skips.is_empty() {
        return Ok(data);
    }
    if let Some(items) = data.get_mut(list).and_then(Value::as_array_mut) {
        let kept: Vec<Value> = items
            .iter()
            .enumerate()
            .filter(|(i, _)| !skips.contains(&u32::try_from(*i).unwrap_or(u32::MAX)))
            .map(|(_, v)| v.clone())
            .collect();
        if kept.is_empty() {
            return Err(ContentError::new(
                "generated_all_skipped",
                "Every item in this set was marked as wrong. You can delete it in Settings.",
            ));
        }
        *items = kept;
    }
    Ok(data)
}

/// The item prompts of a quiz or teaser set as plain text, in order, for the "This looks wrong"
/// picker. Not cleaned of anything the data check did not already allow; empty for other engines.
pub(crate) fn set_items(core: &Core, id: &str) -> Result<Vec<String>, String> {
    let handle = resolve(core, id)?;
    let Some((list, field)) = skippable(handle.engine_ref) else {
        return Ok(Vec::new());
    };
    if handle.unusable {
        return Err("generated_unusable".into());
    }
    let row =
        with_store(core, |s| s.generated_set(id))?.ok_or_else(|| "unknown_activity".to_owned())?;
    let data: Value = serde_json::from_str(&row.data_json).map_err(|_| "internal".to_owned())?;
    Ok(data
        .get(list)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|item| {
            clean_line(
                item.get(field).and_then(Value::as_str).unwrap_or_default(),
                300,
            )
        })
        .collect())
}

/// Marks item `index` of a quiz or teaser set as wrong; it is left out the next time the set
/// starts. Nothing is sent anywhere.
pub(crate) fn skip_item(core: &Core, id: &str, index: u32) -> Result<(), String> {
    let count = set_items(core, id)?.len();
    if usize::try_from(index).map_or(true, |i| i >= count) {
        return Err("invalid_item".into());
    }
    with_store(core, |s| s.add_generated_skip(id, index))
}

/// The item prompts of a bundled (first-party) pack of `engine_short`, for the avoid list. An
/// installed pack's text is never used: it is untrusted (ADR-028 section 4).
pub(crate) fn builtin_pack_items(pack_id: &str, engine_short: &str) -> Vec<String> {
    let (Some(reference), Some(kind)) = (
        engine_ref(engine_short),
        ::companion::generate::EngineKind::from_short(engine_short),
    ) else {
        return Vec::new();
    };
    let Some(pack) = super::runnable(pack_id) else {
        return Vec::new();
    };
    if !matches!(pack.role, super::Role::Pack) || pack.manifest.engine.as_deref() != Some(reference)
    {
        return Vec::new();
    }
    match super::pack_content(pack) {
        Ok((_, data)) => ::companion::generate::avoid_items(kind, data),
        Err(_) => Vec::new(),
    }
}

/// Whether a set may use `activity.storage`: only if its engine declares it, as for packs.
pub(crate) fn storage(handle: &Handle) -> bool {
    has_storage(handle.engine)
}

/// What the host knows about a model-written set, for the UI's badge, card and Settings list.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedInfo {
    /// The model's name as stored, cut to 40 characters; None if unknown.
    pub(crate) model: Option<String>,
    /// The engine reference (`id@major`).
    pub(crate) engine: String,
    pub(crate) created_utc: String,
    /// What the user asked for. User or first-party text, shown as plain text.
    pub(crate) topic: String,
    pub(crate) difficulty: String,
    /// True if the set failed its load-time check; it can only be deleted.
    pub(crate) unusable: bool,
}

/// The `Source` shown for a set: not a marketplace activity.
fn source() -> Source {
    Source {
        marketplace_id: "generated".to_owned(),
        builtin: false,
        commit: None,
    }
}

/// A set's title from its data, if it passes the plain-text rule; the data is checked again in
/// full when it is loaded, so this only has to be safe to show.
fn title_of(row: &GeneratedSet) -> Option<String> {
    let value: Value = serde_json::from_str(&row.data_json).ok()?;
    let title = value.get("title")?.as_str()?;
    marketplace::manifest::plain(title, 100).then(|| title.to_owned())
}

/// The `ActivityInfo` of a set: the engine's help, category and preview, the set's title as its
/// name and the marker that it was generated.
pub(crate) fn info(
    row: &GeneratedSet,
    engine: &Activity,
    saved_utc: Option<String>,
    played_seconds: i64,
) -> ActivityInfo {
    let m = &engine.manifest;
    let title = if row.unusable { None } else { title_of(row) };
    let name = title.unwrap_or_else(|| FALLBACK_NAME.to_owned());
    ActivityInfo {
        id: row.id.clone(),
        name,
        description: "Written for you by your own model. It can be wrong.".to_owned(),
        category: m.category.clone(),
        tags: clean_tags(&m.tags),
        version: m.version.clone(),
        storage: has_storage(engine),
        saved_utc,
        help: m.help.clone(),
        preview: m.preview.is_some(),
        played_seconds,
        engine: Some(row.engine.clone()),
        source: source(),
        generated: Some(GeneratedInfo {
            model: row
                .model
                .as_deref()
                .map(|m| m.chars().take(MODEL_SHOWN_MAX).collect()),
            engine: row.engine.clone(),
            created_utc: row
                .created
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default(),
            topic: row.topic.clone(),
            difficulty: row.difficulty.clone(),
            unusable: row.unusable,
        }),
    }
}

/// The saved sets as info rows, newest first. `engine` filters by engine: a reference
/// (`lullshift.quiz@1`) or a short name (`quiz`). Includes unusable sets (marked) so the user
/// can delete them. Not part of the normal activity list.
pub(crate) fn list_generated(
    core: &Core,
    engine: Option<&str>,
) -> Result<Vec<ActivityInfo>, String> {
    let filter = match engine {
        None => None,
        Some(e) if e.contains('@') => Some(e.to_owned()),
        Some(short) => Some(
            engine_ref(short)
                .ok_or_else(|| "unknown_engine".to_owned())?
                .to_owned(),
        ),
    };
    let rows = with_store(core, |s| s.generated_sets(filter.as_deref()))?;
    let saved: std::collections::BTreeMap<String, String> = with_store(core, |s| s.snapshots())
        .unwrap_or_default()
        .into_iter()
        .collect();
    let played: std::collections::BTreeMap<String, i64> = with_store(core, |s| s.play_times())
        .unwrap_or_default()
        .into_iter()
        .collect();
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        // A row whose id and engine disagree (hand-edited database) is shown as unusable so it
        // can be deleted, never as playable.
        let Some(parsed) = parse_set_id(&row.id) else {
            continue;
        };
        let Ok((engine, _)) = find_engine(parsed.engine) else {
            continue;
        };
        let mut row = row;
        if row.engine != parsed.engine {
            row.engine = parsed.engine.to_owned();
            row.unusable = true;
        }
        let saved_utc = saved.get(&row.id).cloned();
        let played_seconds = played.get(&row.id).copied().unwrap_or(0);
        out.push(info(&row, engine, saved_utc, played_seconds));
    }
    Ok(out)
}

/// Where a set came from, supplied by the generation code. Cleaned and cut here as well.
#[derive(Debug, Clone)]
pub(crate) struct SetMeta {
    pub(crate) topic: String,
    pub(crate) difficulty: String,
    pub(crate) provider_host: String,
    pub(crate) model: Option<String>,
    /// The pack the set was started from, if any (an activity id).
    pub(crate) origin_pack: Option<String>,
}

/// One line of plain text: control and invisible characters dropped, trimmed, cut to `max`.
fn clean_line(text: &str, max: usize) -> String {
    text.chars()
        .filter(|c| !c.is_control() && !agent_core::text::is_invisible(*c))
        .collect::<String>()
        .trim()
        .chars()
        .take(max)
        .collect()
}

/// The callback type of [`engine_check`]: the problems, or Ok.
pub(crate) type EngineCheck = Box<dyn Fn(&Value) -> Result<(), Vec<String>> + Send + Sync>;

/// A check for the generation code to run on a model's reply before acceptance: the engine's
/// own data schema and plain-text rule (the same gate as a pack) plus "no pictures". Problems
/// are short sentences that quote none of the data. Unknown engine: always a problem.
pub(crate) fn engine_check(engine_short: &str) -> EngineCheck {
    let schema = engine_ref(engine_short).and_then(|r| find_engine(r).ok().map(|(_, s)| s));
    Box::new(move |data| match schema {
        None => Err(vec![
            "this activity can't have model-written sets".to_owned(),
        ]),
        Some(schema) => {
            let problems = data_problems(schema, data);
            if problems.is_empty() {
                Ok(())
            } else {
                Err(problems)
            }
        }
    })
}

const SAVE_FAILED: &str = "Lullshift couldn't save the set.";

/// Checks a model's data against the engine (`engine_check`), stores it as a new set, prunes the
/// engine to ten sets (sets with a save are kept) and returns the new set id. On any problem
/// nothing is stored and the problems are returned. Never logs the data or topic.
pub(crate) fn accept_generated_set(
    core: &Core,
    engine_short: &str,
    data: Value,
    meta: &SetMeta,
) -> Result<String, Vec<String>> {
    let reference = engine_ref(engine_short)
        .ok_or_else(|| vec!["this activity can't have model-written sets".to_owned()])?;
    engine_check(engine_short)(&data)?;
    let json = serde_json::to_string(&data).map_err(|_| vec![SAVE_FAILED.to_owned()])?;
    if json.len() > DATA_MAX {
        return Err(vec!["The set is over 256 KiB.".to_owned()]);
    }
    let topic = clean_line(&meta.topic, TOPIC_MAX);
    let difficulty = match clean_line(&meta.difficulty, 32) {
        d if d.is_empty() => "unknown".to_owned(),
        d => d,
    };
    let host = clean_line(&meta.provider_host, HOST_MAX);
    let model = meta
        .model
        .as_deref()
        .map(|m| clean_line(m, MODEL_MAX))
        .filter(|m| !m.is_empty());
    let origin = meta
        .origin_pack
        .as_deref()
        .filter(|p| p.len() <= 128 && marketplace::manifest::valid_id(p));
    let now = OffsetDateTime::now_utc();
    // The id is random; a clash (one in 36^12) is retried rather than reported.
    for _ in 0..3 {
        let id = new_set_id(engine_short).map_err(|_| vec![SAVE_FAILED.to_owned()])?;
        let stored = with_store(core, |s| {
            s.insert_generated_set(&local_store::NewGeneratedSet {
                id: &id,
                engine: reference,
                topic: &topic,
                difficulty: &difficulty,
                data_json: &json,
                created: now,
                provider_host: &host,
                model: model.as_deref(),
                origin_pack: origin,
            })
        });
        match stored {
            Ok(()) => {
                let removed = with_store(core, |s| {
                    s.prune_generated_sets(reference, KEEP_GENERATED_SETS_PER_ENGINE)
                })
                .unwrap_or_default();
                for gone in &removed {
                    forget(core, gone);
                }
                return Ok(id);
            }
            // `with_store` hides the kind of store error; only a clash is worth retrying, and
            // that is the one failure a fresh id fixes.
            Err(_) => continue,
        }
    }
    Err(vec![SAVE_FAILED.to_owned()])
}

/// Drops what the host holds in memory for a deleted set: its launch key, so a frame still open
/// on it stops loading files. (A set has no content cache: its data is read and checked on every
/// load.)
fn forget(core: &Core, id: &str) {
    core.launches.end(id);
}

/// Deletes one set with its skips, snapshot, activity storage and play time, and revokes its
/// launch key. Returns whether it existed. Only ids in the reserved namespace are touched.
pub(crate) fn delete_generated_set(core: &Core, id: &str) -> Result<bool, String> {
    if !is_set_id(id) {
        return Err("unknown_activity".into());
    }
    let removed = with_store(core, |s| s.delete_generated_set(id))?;
    forget(core, id);
    Ok(removed)
}

/// Deletes every set the same way. Returns how many were removed.
pub(crate) fn delete_all_generated_sets(core: &Core) -> Result<usize, String> {
    let ids = with_store(core, |s| s.delete_all_generated_sets())?;
    for id in &ids {
        forget(core, id);
    }
    Ok(ids.len())
}

#[cfg(test)]
mod tests;
