# ADR-028: Fresh activity content written by your own model (opt-in)

- **Status:** Accepted (owner decisions, 2026-11-08; see "Owner decisions" at the end). Built in Sprint 13.
- **Date:** 2026-11-08
- **Owner:** Implementation lead
- **Related:** ADR-003 (activity model), ADR-012 (sandbox and bridge), ADR-016 (engines and content packs), ADR-020 (BYOK Companion), ADR-027 section 5 (asking your model for an idea), PRD principle 5 (network use only when the user starts it)

## Context

The owner finds the bundled activities repetitive: a quiz pack has a fixed set of questions, so after a few rounds every question is familiar. The Quick Quiz engine already shuffles the order (`shuffle`, with a seeded generator), but it can't add questions that aren't in the pack.

The owner asked whether, with the user's consent, the Companion model the user set up (BYOK) could write some of the content.

What is already in place:

- A content pack is code-free JSON. The host resolves the engine, checks the data against the engine's strict data schema with the `jsonschema` crate, runs the plain-text checks (`pack_data_problems`) and passes the checked data to the sandboxed engine as `init.content` (ADR-016). Engines treat `init.content` as untrusted text.
- The Companion already makes BYOK calls, with keys in the OS keychain, a monthly budget, limits on timeouts and size, and cleaned text (ADR-020).
- ADR-027 section 5 set the pattern for a one-off model request that is not a conversation: off by default, consent that shows the exact text, one request per button press, a narrowly checked JSON reply, and a quiet fallback.

Constraints: no accounts or hosted services; network use only when the user starts it; task or session data is never uploaded; every activity and marketplace is untrusted; the sandbox and CSP are never weakened to make content work.

## Decision

### 1. What it is

A model-written **set** is engine data that the user's model writes on request, the host checks exactly like a pack's data, and the unchanged engine runs. To the engine, a set looks just like a pack. The activity sandbox, CSP, bridge and permissions do not change.

### 2. Which engines (v1)

Only first-party engines, in two groups.

**Reading engines** (text a person reads and can judge for themselves), under the main switch:

| Engine | Generated | Not generated |
|---|---|---|
| `lullshift.quiz@1` | `title`, `intro`, `questions[].prompt`, `choices`, `answer`, `explanation` | `image`, `shuffle` (the host sets `shuffle: true`) |
| `lullshift.teaser@1` | the whole data | |
| `lullshift.cards@1` (calm cards) | the whole data | |
| `lullshift.reflection@1` | `title`, `intro`, `prompts` | `allowNotes` (the host sets `true`, as six of the seven built-in reflection packs do; notes stay on this device and are never sent) |
| `lullshift.typing@1` | the whole data | |

**Body engines** (instructions a person follows with their body), under a separate switch (section 3) and the stricter rules of section 2a:

| Engine | Generated | Set by the host, never by the model |
|---|---|---|
| `lullshift.breathing@1` | `title`, `intro`, `phases[].cue`, `closing` | `phases` kinds and seconds, and `rounds`: the user picks one of the host's fixed gentle patterns (section 2a) |
| `lullshift.movement@1` | `title`, `intro`, `moves[].name`, `steps`, `side`, `position`, and `seconds` or `reps` within the host caps | `wayOut` (fixed host text), `restSeconds` (15), `closing` (fixed host text) |
| `lullshift.pause@1` | `title`, `intro`, `steps[].text`, `look`, and `seconds` within the host caps | `closing` (fixed host text) |

**Not offered:** `away` and `toy`, where generation adds little. **Third-party engines are not offered** either: their schemas and descriptions are untrusted text that would go into the prompt, and the host can't write a prompt for an engine it doesn't know. A later ADR can open this up.

### 2a. Extra rules for body engines

Model-written physical or breathing instructions could be unsafe, and the user can't check them in the moment. So, on top of everything else in this ADR:

- **Strict instruction.** Each body profile's fixed instruction says, in plain words: gentle, desk-friendly moves for an adult at a computer; seated or standing beside the desk; no equipment, no floor work, no jumping, running, lifting or weights; never push into pain or hold a stretch "as far as you can"; no neck circles or rolls; no breath holds longer than the host pattern; no closing the eyes while standing; no medical, therapy or health claims; always allow stopping at any moment.
- **Host-owned timings.**
  - Breathing: the model writes no timings. The user picks one of the host's patterns: **Even** (inhale 4, exhale 4), **Longer out** (inhale 4, exhale 6), **Box, gentle** (inhale 4, hold 2, exhale 4, hold 2). Rounds: 4, 6 or 8. These sit well inside the engine's limits (at most 8 s per phase).
  - Movement: `seconds` at most 60 (the engine allows 120), `reps` at most 10 (the engine allows 20), at most 8 moves per set.
  - Pause: `seconds` at most 60 per step (the engine allows 120), at most 8 steps.
  The generation schema carries these lower caps, so a reply over them fails the check.
- **Wording check.** After the schema, every generated string in a body set is checked against a host-owned list of refused phrases (case-folded, whole words), kept in Rust with tests: for example "hold your breath", "as far as", "until it hurts", "through the pain", "jump", "lift", "weights", "lie down", "on the floor", "neck roll", "neck circle", "close your eyes" (movement only), "cure", "treat", "therapy", "diagnos". A hit refuses the whole set (it counts as a failed check, so the one retry of section 5 applies, with a fixed sentence naming only the category: "A step asked for something the activity doesn't allow"). The list errs on the side of refusing; a refused gentle set costs one retry, while a harmful one reaching the user would cost far more.
- **Fixed safety text.** `wayOut` and `closing` are host text, for example "Stop at any time. If something hurts or feels wrong, stop and rest." The badge (section 8) adds "Stop if anything feels uncomfortable."
- **No suggestions.** Body sets are never offered by Home suggestions or the Companion's "Suggest a break" lookup; the user starts them from Activities.

Each offered engine gets a **generation profile** in the host (Rust): a host-written instruction, a **generation schema** derived from the engine's data schema (same limits, with the excluded fields removed and lower item counts, see section 6), and the fields the host fills itself. The profiles live in Lullshift's code, not in activity folders, so they can't come from a marketplace.

### 3. Consent and when anything is sent

- **Off by default.** Settings > Companion gets **Let my model write activity content**. It can be turned on only when a Companion model is set up. Turning it on shows the exact text that would be sent for one example (a quiz on a sample topic) and asks for consent. Turning it off stops all generation and leaves saved sets alone (they can be deleted, section 7).
- **A second switch for body engines.** Below it, **Also let my model write movement, breathing and pause routines**, off by default even when the first switch is on, and usable only while the first is on. Turning it on shows its own consent text: the content is AI-generated and can be wrong; Lullshift limits timings and refuses risky wording but cannot guarantee a routine suits your body; stop if anything feels uncomfortable; check with a professional if you have an injury or a health condition. Turning the first switch off also turns this one off.
- **Only on a button press.** When the setting is on, the card of a supported pack (and of the engine group in Activities) gains **Fresh set from my model**. Each press, after the user confirms the topic, sends one request (one retry at most, section 5). Nothing is sent when Home or Activities opens, on a timer, before a set runs out, or from the tray window.
- The request uses the Companion provider and model the user chose (the default model, or one picked in the dialog, as with per-message choice in ADR-020 decision 36), goes only to that provider, counts against the monthly budget and is not stored in a Companion thread.
- With a local model (an OpenAI-compatible server on loopback), the whole feature works offline.

### 4. What is sent

- The engine profile's fixed instruction and its generation schema.
- **The topic:** one plain-text line, 1 to 80 characters, cleaned of control and hidden characters (`agent_core::text::is_invisible`). The dialog suggests one:
  - for a **built-in** pack, the pack's title (first-party text, so trusted);
  - for an **installed third-party** pack, nothing: the field starts empty and its title, description and tags are never sent (untrusted text could carry instructions to the model, and the provider would learn what the user has installed, the same reasoning as ADR-027);
  - typing a different topic is always allowed.
- **Difficulty** (`gentle`, `medium`, `tricky`) and **count**, from host-owned choices.
- **Avoid list (optional, on by default):** up to 20 prompts (for body engines, move or step names) of earlier sets on the same engine and topic, cut to 120 characters each, so the new set doesn't repeat them. This is text the model wrote earlier plus the built-in pack's own questions, never the user's. The dialog shows that it is included and lets the user switch it off for that request.
- **Never sent:** session titles, task or prompt text, code, file paths, tool or agent names, Companion threads, notes written in reflection activities, third-party activity text, keys.

The dialog shows a short, fixed summary of what is sent and a **Show exact text** disclosure with the full request body.

### 5. What is accepted

- **Plain JSON in the reply text, no special API features.** Many OpenAI-compatible servers don't support structured output or tool calls reliably, so the host asks for one JSON object and parses it itself. It accepts the first top-level JSON object in the reply, optionally inside one Markdown code fence; anything else is refused. Reasoning or thinking output is ignored and never stored.
- **Limits:** 64 KiB of reply text, a 120 s hard cap per call (60 s to the first byte, as ADR-020 sets for streaming), no tools sent.
- **Checks, in order:** parse; the generation schema; host-filled fields added; the **engine's own data schema** with the same `jsonschema` gate as packs; `pack_data_problems` (combining marks and the rest); for the quiz, the profile's extra checks (choices unique after trimming and case folding; `answer` within `choices`). A set that fails is refused as a whole; no partial sets are kept.
- **One retry:** if the first reply fails a check, the host sends one more request with a fixed sentence naming the kind of problem ("The reply was not valid JSON", "A question had more than five choices"), never the reply text itself. If that also fails, nothing is stored and the user sees a calm card: "Your model's set didn't fit this activity, so Lullshift didn't start it. Nothing was lost." with a **Play the built-in set** button. Offline, no key, a timeout or a provider error show the same card with the provider reason that ADR-020 already allows.
- **Budget:** usage is counted for every call, including failed ones, with the ADR-020 rules.

### 6. Size per set

Smaller than packs, so one request stays quick and cheap:

| Engine | Items per set (user picks) |
|---|---|
| quiz | 5, 8 or 12 questions |
| teaser | 3, 5 or 8 teasers |
| calm cards | 5, 8 or 12 cards |
| reflection | 3, 5 or 8 prompts |
| typing | 5, 8 or 12 lines |
| movement | 3, 5 or 8 moves |
| pause | 3, 5 or 8 steps |
| breathing | one routine (the pattern and rounds come from the host) |

### 7. Storage, identity and resume

- **Saved locally so they replay offline.** A new table `generated_sets` (migration 0021, with migration and rollback tests as CLAUDE.md rule 8 requires): id, engine `id@major`, the topic, difficulty, the checked data, created time, provider host and model name, and the pack it was started from (if any). Not a secret and not task data; never logged (only counts and error kinds are).
- **At most 10 sets per engine.** The oldest set without a save is removed first; a set with a save is kept until the user deletes it or finishes it.
- **Identity.** Each set gets a host-made activity id `lullshift.generated.<engine short name>.<12 random lowercase letters and digits>` (valid under the manifest id pattern, under a namespace that bundled and installed activities can't use: the marketplace and bundling checks refuse `lullshift.generated.` ids). The scheme handler serves the engine's files under that id exactly as for a pack (ADR-016), so snapshots, play time, storage and the paused-view summary are all keyed by the set and **resume needs no new mechanism**. `activity_content` for a set id reads the row and **checks it again** (engine schema plus text checks) on every load, like an installed pack; a row that fails is marked unusable, not served.
- **Storage permission.** A set may use `activity.storage` only if its engine declares it, as for packs. It is deleted with the set.
- **Deletion.** Settings > Companion lists the saved sets (title, topic, engine, date, model) with **Delete** per set and **Delete all model-written sets**. Deleting a set deletes its snapshot, storage and play time. Turning the setting off doesn't delete sets; uninstalling the app's data does.

### 8. How sets look in the app

- In Activities, under the engine's packs, a **Made by your model** group lists the saved sets, each with a small host-owned **AI-generated** badge and the model name.
- Every set shows, in host-owned text above the engine frame: **"AI-generated by <model name>. It can be wrong."** The model name is the one stored with the set, cleaned and cut to 40 characters; if it is unknown the text is "AI-generated by your model. It can be wrong." Body sets add **"Stop if anything feels uncomfortable."** The quiz's explanations come from the model too, so the badge stays on the whole time and is not dismissed. It is in the page, not only a tooltip, and the frame's accessible name starts with "AI-generated" so screen readers announce it.
- The paused view, the activity card and the Settings list all carry the same AI-generated label, so a set is never shown without it.
- **This looks wrong** (host-owned button beside the badge, quiz and teaser only) marks the current item as skipped for this set, stored locally. Nothing is sent and the model is not told.
- The paused-view summary and help come from the engine's own manifest; the set's title is shown as plain text (it is model text, checked like pack text).
- Sets are **not** ranked in Home suggestions (ADR-027) in v1: suggestions stay built from installed activities. A set can still be resumed from its save like any activity.

### 9. Threat model additions

| Threat | Mitigation |
|---|---|
| The model returns script, markup or links | Engines render text only (unchanged); the engine data schema and plain-text checks apply; the activity CSP keeps `connect-src 'none'` |
| A huge or deeply nested reply | 64 KiB reply cap (well under the 256 KiB pack data cap); the generation and engine schemas fix the shape, so extra nesting is refused by `additionalProperties: false`, `maxItems` and `maxLength` |
| Third-party text steers the model (prompt injection) | Third-party pack text and engine schemas are never sent; topic is user-typed or first-party |
| The user's work leaks to the provider | Only topic, difficulty, count and the optional avoid list (model-written or first-party text) are sent; a test asserts the request body contains nothing else |
| A forged set id loads another activity's save | Set ids are host-made under a reserved namespace that installs refuse; snapshot commands already scope by the calling activity (ADR-012) |
| A tampered database row | The data is checked again on every load |
| Wrong facts presented as true | The permanent "AI-generated by <model>. It can be wrong." badge; built-in packs stay as they are |
| An unsafe movement, breathing or pause instruction | Separate opt-in with its own consent; strict instruction; host-owned breathing patterns; lower caps on timings and counts; refused-phrase check on every string; fixed host safety text; never suggested automatically |
| Runaway cost | One request per press, one retry, the monthly budget checked before each call |

### 10. A companion change without a model: draw a round from the pack

Separately from model writing, and useful without any network: the quiz, teaser and calm-cards engines gain an optional `roundSize` field (integer, at least 3 and at most the item count). When set, each new round shows `roundSize` items drawn with the existing seeded generator, preferring items not seen in the last rounds (seen ids are kept in the engine's own snapshot). Bundled packs with 12 or more items set it. Adding an optional field changes the data schema, and the strict schema (`additionalProperties: false`) means an older host would refuse packs that use it. v1 has not shipped, so (as with ADR-027's manifest fields) it is changed in place without a new major. Fixtures, the shared corpus and creator docs are updated.

## Consequences

- Activities stop feeling fixed for users who have a Companion model, without any change to the sandbox, CSP or engines' code (apart from `roundSize`).
- One new network call type, within the PRD's existing rule (BYOK Companion calls the user starts). On acceptance, PRD §4.6 and the network bullet in §6 get one sentence each naming it, and the capability notes in `docs/` list it.
- One new table with migration and rollback tests; a reserved id namespace checked by the marketplace and bundling code.
- Generation profiles are host code, so adding an engine to the feature needs a Lullshift change and a review, not a pack.
- The quality of the content depends on the user's model; small local models may often fail the checks. The honest failure card and the built-in fallback keep this calm.

## Alternatives considered

- **Let engines call the model themselves:** needs network or IPC inside the sandbox. Rejected outright (CLAUDE.md, ADR-012).
- **Generate in the background before a set runs out:** smoother, but breaks "network only when the user starts it" and spends the user's money unasked. Rejected.
- **Send pack names and descriptions for better topics:** rejected for third-party packs for the same reasons as ADR-027; allowed for first-party packs only.
- **Use provider structured output or tool calls:** more reliable with the big providers, but uneven across compatible servers. Plain JSON plus the host's own checks works everywhere; using structured output where supported can be added later behind the same checks.
- **Let third-party engines ship their own generation prompts:** the prompt itself would be untrusted and could ask the model for anything, with the user's key and budget. Deferred to a later ADR.
- **Personalised content from the user's current task** (for example a quiz about the library they're using): would send task text to the provider. Not part of this ADR; it would need its own opt-in on top of ADR-013's capture rules.
- **Don't store sets (generate each time):** simpler, but no replay offline, more cost, and resume would break. Rejected.

## Implementation notes (Sprint 13)

Where these notes differ from the sections above, the notes describe what was built.

- **Where things live.**
  - `crates/companion/src/generate/` holds the profiles, generation schemas, request building, `preview_request_text`, reply checks and the refused-phrase list.
  - `crates/local-store/src/generated_sets.rs` with migration 0021 holds the stored sets and skips.
  - `apps/desktop/src-tauri/src/activities/generated.rs` holds set ids, serving and the re-check on every load.
  - `apps/desktop/src-tauri/src/companion_commands/generate_sets.rs` holds the nine Tauri commands, permitted only for the main window.
- **One schema gate.** The Companion crate checks generation schemas with `marketplace::data_schema`, the same `jsonschema` 0.60.2 gate with the same keyword subset and pattern translation as packs. No second validator was added.
- **Generation schemas are derived from the engine schemas** with a list of allowed properties, and every string's `pattern` and `maxLength` is copied from the engine. A field added to an engine later, such as `roundSize`, never reaches the model.
- **Breathing.** The model writes one cue per phase of the chosen pattern. The host joins the cues with its own kinds and seconds.
- **Body difficulty.** Movement, breathing and pause requests always send "gentle".
- **The topic and avoid items are sent as JSON strings** inside the user message, labelled as information only, so a quote or newline in them can't start a new instruction line. A topic over 80 characters is refused, not cut.
- **The retry adds one fixed sentence** to the same user message. It contains no reply text and no engine error text, and a test checks this.
- **Budget.** The budget check before each call receives the usage of the calls already made, so a first call that reaches the limit stops the retry. Cancelled or timed-out calls book their real usage when it arrives.
- **Refused phrases.** The built-in movement, breathing and pause packs use "as far as feels easy", "feet on the floor", shoulder lifts and "Gentle squats". To let them pass, phrases were narrowed rather than the rule loosened: "as far as you can", "lie/sit on the floor", "lift weights/heavy/yourself", "deep/full squat". "Close your eyes" is refused only for movement, because `pause-eye-rest` uses it. Medical words are listed as explicit word forms, so "treaty" passes while "treat yourself" is refused.
- **This looks wrong** opens a list of the set's items, and the user picks the wrong one. It is skipped from the next start: `activity_content` filters skipped items, and a set with nothing left is refused calmly. The sandboxed engine doesn't tell the host which item is showing, and adding that to the bridge wasn't worth it.
- **Progress phases** are `waiting` (sending and writing), `checking` and `retrying`, then done or failed. Only one generation runs at a time.
- **Serving.** For each file request, the `lullact` handler checks that the set exists, its engine matches and it isn't marked unusable. The data itself is checked by `activity_content` on every load, before any of it reaches the frame.
- **`roundSize`.**
  - "At most the item count" can't be written in the host's schema keyword subset. Engines play the whole pack when `roundSize` is too large, `lullshift validate` warns, and the bundled-pack check makes it an error (ADR-016 Sprint 13 amendment).
  - Only `wonder-facts` (12 cards) had 12 or more items, so it is the only bundled pack with `roundSize` (6). Every other bundled pack has 6 to 11 items.

## Owner decisions (2026-11-08)

1. **Engines:** all five reading engines, plus movement, breathing and pause behind a separate opt-in, with strict instructions to the model and a clear AI-generated label (sections 2, 2a, 3 and 8). The first draft had left the body engines out.
2. **Avoid list:** on by default, with at most 20 earlier prompts (the draft said 40).
3. **Saved sets:** 10 per engine, as drafted.
4. **`roundSize`:** in the same sprint.
5. **Badge:** "AI-generated by <model name>. It can be wrong.", plus "Stop if anything feels uncomfortable." for body sets.
