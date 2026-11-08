//! Model-written activity sets: the Tauri commands behind Settings > Companion and Activities
//! (Sprint 13, ADR-028 sections 3, 4, 5, 7 and 8).
//!
//! Rules kept here:
//!
//! - Nothing is sent unless the user pressed **Fresh set from my model**, the matching switch is
//!   on, and a provider, model, key and budget are in order. A refusal happens before any
//!   network use.
//! - The request holds only what `companion::generate` puts in it: the topic the dialog sends,
//!   difficulty, size, an optional avoid list and (breathing) the user's pattern. The topic comes
//!   only from the UI's topic field: no command here reads a pack's title, description or tags.
//! - The avoid list is the newest saved sets of the same engine and topic (model-written text)
//!   and, only for a **bundled** origin pack, that pack's own items. Installed packs' text is
//!   never used.
//! - Nothing here logs. Topic, data and reply text never appear in an error or an event.
//! - Usage of every provider call is booked, cancelled ones included (through the late-usage
//!   queue the Companion already uses).

use std::sync::Arc;

use ::companion::generate::{
    BREATHING_ROUNDS, BreathingChoice, BreathingPattern, CallUsage, Difficulty, EngineKind,
    GenerateError, GenerateFailure, GenerationRequest, Hooks, MAX_AVOID_ITEMS, generate_with_http,
    preview_request_text,
};
use ::companion::models_dev;
use ::companion::{CancelToken, CompanionError};
use local_store::CompanionProfile;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use super::{
    CommandError, CoreState, Prepared, ProviderReasonDto, QueuedUsage, Res, book,
    budget_still_ok_plus, check_request_id, kind_of, load_prefs, prepare, run, save_prefs,
    with_store,
};
use crate::Core;
use crate::activities::generated::{self as sets, SetMeta};
use crate::activities::{ActivityInfo, info_for_core};

// ---- settings ----------------------------------------------------------------------------

/// The two switches. `body_enabled` is true only while `enabled` is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContentGenerationPrefs {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub body_enabled: bool,
}

impl ContentGenerationPrefs {
    /// The stored form, repaired: body never stays on without the main switch.
    fn normalized(self) -> Self {
        Self {
            enabled: self.enabled,
            body_enabled: self.enabled && self.body_enabled,
        }
    }
}

/// Sets both switches. Turning `enabled` on needs a Companion provider with a model; turning it
/// off turns the body switch off too; the body switch can be on only while `enabled` is.
pub fn set_settings(core: &Core, enabled: bool, body_enabled: bool) -> Res<ContentGenerationPrefs> {
    if enabled {
        let configured = with_store(core, |s| s.companion_profiles())?
            .iter()
            .any(|p| !p.model.trim().is_empty());
        if !configured {
            return Err(CommandError::invalid(
                "Set up a provider and a model in Settings > Companion first.",
            ));
        }
    }
    if body_enabled && !enabled {
        return Err(CommandError::invalid(
            "Turn on model-written activity content first.",
        ));
    }
    let mut prefs = load_prefs(core);
    prefs.content_generation = ContentGenerationPrefs {
        enabled,
        body_enabled,
    }
    .normalized();
    save_prefs(core, &prefs)?;
    Ok(prefs.content_generation)
}

fn settings(core: &Core) -> ContentGenerationPrefs {
    load_prefs(core).content_generation.normalized()
}

// ---- request -----------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BreathingDto {
    /// `even`, `longerOut` or `boxGentle`.
    pub pattern: String,
    pub rounds: u32,
}

/// What the dialog sends. There is deliberately no field for a pack's title, description or
/// tags: the topic is whatever the user typed (or accepted) in the topic field.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GenerateRequestDto {
    /// Engine short name: `quiz`, `teaser`, `cards`, `reflection`, `typing`, `movement`,
    /// `breathing` or `pause`.
    pub engine: String,
    pub topic: String,
    /// `gentle`, `medium` or `tricky`. Ignored for body engines (always gentle).
    pub difficulty: String,
    pub size: u32,
    pub breathing: Option<BreathingDto>,
    /// Include earlier items of this engine and topic (and a built-in origin pack's own items).
    pub avoid: bool,
    /// The activity id of the pack the dialog was opened from, if any.
    pub origin_pack: Option<String>,
    /// The provider and model for this request; None uses the default provider and its model.
    pub profile_id: Option<String>,
    pub model: Option<String>,
}

fn invalid(message: &str) -> CommandError {
    CommandError::invalid(message)
}

fn typed(kind: &'static str, message: &str) -> CommandError {
    CommandError {
        kind,
        message: message.to_owned(),
        provider_reason: None,
    }
}

/// The user's dialog choices as a [`GenerationRequest`], without the avoid list.
fn base_request(dto: &GenerateRequestDto) -> Res<GenerationRequest> {
    let engine = EngineKind::from_short(&dto.engine)
        .ok_or_else(|| invalid("That activity can't have model-written sets."))?;
    let difficulty = Difficulty::parse(&dto.difficulty)
        .ok_or_else(|| invalid("Choose gentle, medium or tricky."))?;
    let breathing = match &dto.breathing {
        None => None,
        Some(b) => {
            let pattern = BreathingPattern::parse(&b.pattern)
                .ok_or_else(|| invalid("Choose one of the breathing patterns."))?;
            if !BREATHING_ROUNDS.contains(&b.rounds) {
                return Err(invalid("Rounds must be 4, 6 or 8."));
            }
            Some(BreathingChoice {
                pattern,
                rounds: b.rounds,
            })
        }
    };
    Ok(GenerationRequest {
        engine,
        topic: dto.topic.clone(),
        difficulty,
        size: dto.size,
        avoid: Vec::new(),
        breathing,
    })
}

/// Same-topic comparison: cleaned, trimmed, case-folded.
fn topic_key(s: &str) -> String {
    ::companion::generate::clean_line(s).to_lowercase()
}

/// The avoid list (ADR-028 section 4): items of the newest saved sets of this engine with the
/// same topic, then, only for a bundled origin pack, that pack's own items; the most recent 20.
fn avoid_list(core: &Core, req: &GenerationRequest, origin: Option<&str>) -> Res<Vec<String>> {
    let short = req.engine.short();
    let mut sets_rows = with_store(core, |s| s.generated_sets(Some(req.engine.engine_ref())))?;
    sets_rows.sort_by_key(|r| std::cmp::Reverse(r.created));
    let want = topic_key(&req.topic);
    // Oldest first within the final list, so the "most recent 20" are the last 20.
    let mut items: Vec<String> = Vec::new();
    if let Some(pack) = origin {
        items.extend(sets::builtin_pack_items(pack, short));
    }
    let mut from_sets: Vec<String> = Vec::new();
    for row in sets_rows
        .iter()
        .filter(|r| !r.unusable && topic_key(&r.topic) == want)
    {
        if let Ok(data) = serde_json::from_str::<serde_json::Value>(&row.data_json) {
            let mut one = ::companion::generate::avoid_items(req.engine, &data);
            one.reverse();
            from_sets.extend(one);
        }
        if from_sets.len() >= MAX_AVOID_ITEMS {
            break;
        }
    }
    // Newest sets come first in `from_sets`; they matter most, so they go last.
    from_sets.reverse();
    items.extend(from_sets);
    let skip = items.len().saturating_sub(MAX_AVOID_ITEMS);
    Ok(items.split_off(skip))
}

/// Builds the request the provider would get, avoid list included when asked for.
fn full_request(core: &Core, dto: &GenerateRequestDto) -> Res<GenerationRequest> {
    let mut req = base_request(dto)?;
    if dto.avoid {
        let origin = dto.origin_pack.as_deref().filter(|p| p.len() <= 128);
        req.avoid = avoid_list(core, &req, origin)?;
    }
    Ok(req)
}

fn error_of(e: GenerateError) -> CommandError {
    CommandError {
        kind: e.kind.as_str(),
        message: e.message,
        provider_reason: e.reason.map(|r| ProviderReasonDto {
            speaker: r.speaker.to_owned(),
            text: r.text,
        }),
    }
}

/// The profile and model a request names: the given pair (the model must be one set up for the
/// profile) or the default provider with its own model. Nothing is read from the keychain.
fn choose(
    core: &Core,
    profile_id: Option<&str>,
    model: Option<&str>,
) -> Res<(CompanionProfile, String)> {
    let profile = match profile_id {
        Some(id) => with_store(core, |s| s.companion_profile(id))?,
        None => with_store(core, |s| s.companion_profiles())?
            .into_iter()
            .next(),
    }
    .ok_or_else(|| invalid("Set up a provider and a model in Settings > Companion first."))?;
    let model = match model {
        None => profile.model.clone(),
        Some(m) => {
            let configured = models_dev::valid_model_id(m)
                && with_store(core, |s| s.companion_model(&profile.id, m))?.is_some();
            if !configured {
                return Err(invalid(
                    "That model isn't set up for this provider. Add it in Settings, then try again.",
                ));
            }
            m.to_owned()
        }
    };
    Ok((profile, model))
}

/// The exact first request body for the chosen provider and model: for both consent dialogs
/// (with a fixed sample request) and "Show exact text". Nothing is sent and no key is read.
/// It works while the switches are off, because the consent comes first.
pub fn preview(core: &Core, dto: &GenerateRequestDto) -> Res<String> {
    let req = full_request(core, dto)?;
    let (profile, model) = choose(core, dto.profile_id.as_deref(), dto.model.as_deref())?;
    preview_request_text(kind_of(&profile)?, &model, &req).map_err(error_of)
}

// ---- generating --------------------------------------------------------------------------

/// What the progress view hears. Every event carries the request id.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum GenerationEvent {
    /// `waiting` (asked; the model is writing), `checking`, `retrying` (the first reply did not
    /// fit; asking once more).
    Phase {
        request_id: String,
        phase: &'static str,
    },
    Done {
        request_id: String,
        set_id: String,
    },
    Failed {
        request_id: String,
        kind: &'static str,
    },
}

pub type GenerationEmit<'a> = &'a (dyn Fn(GenerationEvent) + Send + Sync);

/// Marks the request as running and removes the mark when dropped. One generation at a time.
struct Running<'a> {
    core: &'a Core,
    request_id: String,
    token: CancelToken,
}

impl<'a> Running<'a> {
    fn begin(core: &'a Core, request_id: &str) -> Res<Self> {
        let mut m = core
            .companion
            .generations
            .lock()
            .map_err(|_| CommandError::internal())?;
        if !m.is_empty() {
            return Err(typed(
                "busy",
                "Your model is already writing a set. Wait for it or cancel it first.",
            ));
        }
        let token = CancelToken::new();
        m.insert(request_id.to_owned(), token.clone());
        Ok(Self {
            core,
            request_id: request_id.to_owned(),
            token,
        })
    }
}

impl Drop for Running<'_> {
    fn drop(&mut self) {
        if let Ok(mut m) = self.core.companion.generations.lock() {
            m.remove(&self.request_id);
        }
    }
}

const OFF_TEXT: &str =
    "Model-written activity content is turned off. You can turn it on in Settings > Companion.";
const DID_NOT_FIT_TEXT: &str =
    "Your model's set didn't fit this activity, so Lullshift didn't start it. Nothing was lost.";
const BODY_OFF_TEXT: &str = "Model-written movement, breathing and pause routines are turned off. You can turn them on in Settings > Companion.";

/// Writes one set with the user's model and stores it. Refuses, before any network use, when
/// the switch (or, for body engines, the body switch) is off.
pub fn write_set(
    core: &Core,
    request_id: &str,
    dto: &GenerateRequestDto,
    emit: GenerationEmit<'_>,
) -> Res<ActivityInfo> {
    check_request_id(request_id)?;
    let switches = settings(core);
    if !switches.enabled {
        return Err(typed("generationOff", OFF_TEXT));
    }
    let engine = EngineKind::from_short(&dto.engine)
        .ok_or_else(|| invalid("That activity can't have model-written sets."))?;
    if engine.is_body() && !switches.body_enabled {
        return Err(typed("generationOff", BODY_OFF_TEXT));
    }
    // Everything the request holds is checked before the key is read or anything is sent.
    let req = full_request(core, dto)?;
    let checked = req.validate().map_err(error_of)?;
    let (profile, model) = choose(core, dto.profile_id.as_deref(), dto.model.as_deref())?;
    let running = Running::begin(core, request_id)?;
    let prepared = prepare(core, &profile.id, Some(&model))?;
    let result = run_generation(core, &running, &prepared, &req, request_id, emit);
    let (data, used_model) = match result {
        Ok(v) => v,
        Err(e) => {
            emit(GenerationEvent::Failed {
                request_id: request_id.to_owned(),
                kind: e.kind,
            });
            return Err(e);
        }
    };
    // A cancel that raced the end of the last call: nothing is stored.
    if running.token.is_cancelled() {
        let e = CommandError::from(CompanionError::cancelled());
        emit(GenerationEvent::Failed {
            request_id: request_id.to_owned(),
            kind: e.kind,
        });
        return Err(e);
    }
    let origin = dto
        .origin_pack
        .clone()
        .filter(|p| p.len() <= 128 && marketplace::manifest::valid_id(p));
    let meta = SetMeta {
        topic: checked.topic.clone(),
        difficulty: checked.difficulty.as_str().to_owned(),
        provider_host: prepared.host.clone(),
        model: Some(used_model),
        origin_pack: origin,
    };
    let id = match sets::accept_generated_set(core, engine.short(), data, &meta) {
        Ok(id) => id,
        Err(_) => {
            // The same gate ran inside the loop, so this is a database problem; the data is
            // dropped like a reply that did not fit.
            let e = typed("didNotFit", DID_NOT_FIT_TEXT);
            emit(GenerationEvent::Failed {
                request_id: request_id.to_owned(),
                kind: e.kind,
            });
            return Err(e);
        }
    };
    emit(GenerationEvent::Done {
        request_id: request_id.to_owned(),
        set_id: id.clone(),
    });
    info_for_core(core, &id).map_err(|_| CommandError::internal())
}

/// The provider call(s). Books the usage of every call, whatever the outcome.
fn run_generation(
    core: &Core,
    running: &Running<'_>,
    prepared: &Prepared,
    req: &GenerationRequest,
    request_id: &str,
    emit: GenerationEmit<'_>,
) -> Res<(serde_json::Value, String)> {
    let queue = Arc::clone(&core.companion.late_usage);
    let profile_id = prepared.profile.id.clone();
    let mut calls = 0u32;
    let phase = |p: &'static str| {
        emit(GenerationEvent::Phase {
            request_id: request_id.to_owned(),
            phase: p,
        });
    };
    let mut budget_check = |made: &[CallUsage]| -> Result<(), CompanionError> {
        calls += 1;
        // Calls made so far in this request are not booked until it ends: count them here, so a
        // retry can't run past the budget. Usage of earlier cancelled calls is flushed first.
        let (input, output) = made.iter().fold((0i64, 0i64), |(i, o), u| {
            (
                i.saturating_add(u.input_tokens),
                o.saturating_add(u.output_tokens),
            )
        });
        budget_still_ok_plus(core, &prepared.profile, input, output)?;
        phase(if calls == 1 { "waiting" } else { "retrying" });
        Ok(())
    };
    let inner = sets::engine_check(req.engine.short());
    let engine_check = |data: &serde_json::Value| {
        phase("checking");
        inner(data)
    };
    let mut hooks = Hooks {
        cancel: &running.token,
        budget_check: &mut budget_check,
        engine_check: &engine_check,
        late_usage: Arc::new(move |u| {
            if let Ok(mut q) = queue.lock() {
                q.push(QueuedUsage {
                    profile_id: profile_id.clone(),
                    at: OffsetDateTime::now_utc(),
                    input_tokens: u.input_tokens,
                    output_tokens: u.output_tokens,
                });
            }
        }),
    };
    let result = generate_with_http(
        &core.companion.gen_http,
        &prepared.spec,
        &prepared.key,
        req,
        &mut hooks,
    );
    match result {
        Ok(out) => {
            for u in &out.usage {
                book(core, &prepared.profile.id, u.input_tokens, u.output_tokens);
            }
            Ok((out.data, out.model))
        }
        Err(GenerateFailure { error, usage }) => {
            for u in &usage {
                book(core, &prepared.profile.id, u.input_tokens, u.output_tokens);
            }
            Err(error_of(error))
        }
    }
}

/// Stops the running request, if any. Whatever the abandoned call costs is booked when it ends.
pub fn cancel(core: &Core, request_id: &str) {
    if let Ok(m) = core.companion.generations.lock()
        && let Some(t) = m.get(request_id)
    {
        t.cancel();
    }
}

// ---- saved sets --------------------------------------------------------------------------

fn from_text(e: String) -> CommandError {
    match e.as_str() {
        "unknown_engine" => invalid("That activity can't have model-written sets."),
        "unknown_activity" => typed("notFound", "That set no longer exists."),
        "invalid_item" => invalid("That item isn't in this set."),
        "generated_unusable" => typed(
            "generatedUnusable",
            "This set from your model can't be used any more. You can delete it in Settings.",
        ),
        _ => CommandError::internal(),
    }
}

pub fn list(core: &Core, engine: Option<&str>) -> Res<Vec<ActivityInfo>> {
    sets::list_generated(core, engine).map_err(from_text)
}

pub fn delete_one(core: &Core, id: &str) -> Res<bool> {
    sets::delete_generated_set(core, id).map_err(from_text)
}

pub fn delete_all(core: &Core) -> Res<usize> {
    sets::delete_all_generated_sets(core).map_err(from_text)
}

pub fn items(core: &Core, id: &str) -> Res<Vec<String>> {
    sets::set_items(core, id).map_err(from_text)
}

pub fn skip(core: &Core, id: &str, index: u32) -> Res<()> {
    sets::skip_item(core, id, index).map_err(from_text)
}

// ---- commands ----------------------------------------------------------------------------

#[tauri::command]
pub async fn generation_set_settings(
    core: CoreState<'_>,
    enabled: bool,
    body_enabled: bool,
) -> Res<ContentGenerationPrefs> {
    run(&core, move |c| set_settings(c, enabled, body_enabled)).await
}

#[tauri::command]
pub async fn generation_preview(core: CoreState<'_>, req: GenerateRequestDto) -> Res<String> {
    run(&core, move |c| preview(c, &req)).await
}

#[tauri::command]
pub async fn generate_set(
    core: CoreState<'_>,
    request_id: String,
    req: GenerateRequestDto,
    on_event: tauri::ipc::Channel<GenerationEvent>,
) -> Res<ActivityInfo> {
    run(&core, move |c| {
        let emit = move |e: GenerationEvent| {
            let _ = on_event.send(e);
        };
        write_set(c, &request_id, &req, &emit)
    })
    .await
}

#[tauri::command]
pub async fn generation_cancel(core: CoreState<'_>, request_id: String) -> Res<()> {
    cancel(&core, &request_id);
    Ok(())
}

#[tauri::command]
pub async fn generated_sets_list(
    core: CoreState<'_>,
    engine: Option<String>,
) -> Res<Vec<ActivityInfo>> {
    run(&core, move |c| list(c, engine.as_deref())).await
}

#[tauri::command]
pub async fn generated_set_delete(core: CoreState<'_>, id: String) -> Res<bool> {
    run(&core, move |c| delete_one(c, &id)).await
}

#[tauri::command]
pub async fn generated_sets_delete_all(core: CoreState<'_>) -> Res<usize> {
    run(&core, delete_all).await
}

#[tauri::command]
pub async fn generated_set_items(core: CoreState<'_>, id: String) -> Res<Vec<String>> {
    run(&core, move |c| items(c, &id)).await
}

#[tauri::command]
pub async fn generated_set_skip(core: CoreState<'_>, id: String, item_index: u32) -> Res<()> {
    run(&core, move |c| skip(c, &id, item_index)).await
}

#[cfg(test)]
pub(crate) mod tests;
