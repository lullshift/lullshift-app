# ADR-027: Smarter suggestions, and opt-in ideas from your own model

- **Status:** Accepted (owner decision, 2026-11-07)
- **Date:** 2026-11-07
- **Related:** ADR-026 (Home, the fits-your-wait suggestion, `suggest.ts`), ADR-003 (activity model), ADR-020 (Companion, BYOK), ADR-026 section 3 (the wait estimate), PRD principles 5 and 7

## Context

ADR-026 suggests an activity from one signal: how long the user's agents usually work. The owner asked whether Lullshift can offer activities "in an intelligent way", and chose: more local signals with a readable rule and a "Why this?" line, and, as a separate opt-in, asking the user's own Companion model (BYOK) for an idea.

Constraints: no account, nothing uploaded by default, no surveillance or scoring of the person (PRD principle 7), no streaks or totals (ADR-026 section 4), network use only when the user starts it (PRD principle 5), every activity and marketplace is untrusted input.

## Decision

### 1. Three optional manifest fields

`reference/ACTIVITY_MANIFEST_V1.schema.json` gains three **optional** properties:

- `typicalMinutes`: integer 1 to 60, how long one round usually takes.
- `energy`: `"calm"` or `"lively"`.
- `focus`: `"light"` (can stop any moment) or `"deep"` (better finished in one go).

They are additive and optional, so every existing manifest stays valid. The schema has `additionalProperties: false`, so a host older than this change refuses manifests that use them; v1 has not shipped, so the change is made in place rather than as a schema v2. The validator, the generated types, the creator CLI (`validate`, `simulate`, templates), the `/activity-creator` skill and the creator guides are updated. All bundled activities get the fields. Third-party activities without them still work and are ranked as unknown (section 3).

The values are claims by the activity author, so they only change ranking, never permissions, and are shown to the user only through host-owned words ("about 3 min", "calm").

### 2. A local preference memory

- Lullshift remembers, per **kind** (a host-owned group such as Puzzles, Breathing or Step away, from the ADR-026 chip map) and never per session or per day: how often a suggestion of that kind was started, finished or dismissed with **Not this kind**.
- Stored in the local database as small counters with a decay (older choices count less; nothing older than 60 days counts). A migration adds one table, with migration and rollback tests (CLAUDE.md rule 8).
- Never shown as numbers, charts or streaks. Settings > Activities shows the kinds it currently leans towards and away from in words, with **Reset** (clears the table) and an off switch **Learn from what I pick** (default on; off means the memory is neither read nor written).
- It is about the person's taste, kept only on this computer, and is never exported, sent to a model (except as section 5 says, opt-in) or used for anything but ranking.

### 3. The ranking rule (`suggest.ts`)

A readable, deterministic score per candidate, computed locally. Candidates are the enabled, installed activities (as in ADR-026). Inputs:

1. **Saved first** (unchanged from ADR-026): resuming the newest save is the first suggestion.
2. **Fit to the wait:** with an estimate, prefer `typicalMinutes` at or under the estimate; unknown minutes rank in the middle.
3. **Interruptions:** if the working agents have asked for the user (needs-you) more than about once every 3 minutes over the last 30 minutes, prefer `focus: "light"`; `"deep"` ranks low.
4. **Time of day** (local clock): a gentle lean to `energy: "calm"` after 20:00 and before 07:00. Never a rule.
5. **Variety:** the last activity started ranks low unless it has a save; the daily puzzle at most once a day.
6. **How long the user was away** (time since the app last had focus): after a long absence prefer movement, eyes and step-away; after a few minutes, resume.
7. **Preference memory** (section 2), if on.

Shuffle walks the ranked list in order (ADR-026's date seed breaks ties). The weights live in one table in `suggest.ts` with unit tests for each input.

### 4. "Why this?"

Every suggestion shows one short line built only from host-owned phrases for the inputs that decided it, for example "Short wait, and you often finish puzzles" or "It's evening, so something calm". If no input applies, the line is "Something different". Activity-supplied text never appears in this line.

### 5. Opt-in: ask your own model for an idea

- **Off by default.** Settings > Companion gets **Let my model suggest activities**, which can only be turned on when a Companion model is set up. Turning it on shows the exact text that would be sent, and asks for consent.
- **Only when the user asks.** When on, the suggestion card gains **Ask my model**. Each press sends one request. Nothing is sent when Home opens, on a timer, or from the tray window (it opens the main window instead).
- **What is sent:**
  - Up to 30 candidates, each as host-owned data only: an opaque index, kind, `typicalMinutes`, `energy` and `focus`.
  - Coarse signals: the wait bucket ("under 3 min", "about 5 min", "10 min or more"), the time-of-day bucket, interruptions high or low, and the leaning kinds from section 2 if that is on.
  - **Never** activity names, descriptions or tags (untrusted: they could carry instructions to the model), session titles, task text, code, file paths or tool names.
- **What is accepted:** the reply must be JSON naming one index from the list and a reason. Anything else, an index not offered, or a reply over 2 KB is ignored. The reason is shown as plain text, stripped of invisible characters, cut to 140 characters, and labelled "Your model says". It never replaces the host's "Why this?" line.
- **Failure:** offline, no key, a timeout (15 s), a provider error or a bad reply: the local suggestion stays, with a quiet note ("Your model didn't answer; here's Lullshift's pick"). Usage is counted like any other Companion request (ADR-020).
- The request uses the Companion profile and model the user chose, goes only to that provider, and is not stored in a Companion thread.

## Consequences

- Suggestions get better without any network use, and every suggestion can explain itself in plain words.
- One new table with migration tests; a manifest schema change that ripples to the validator, CLI, types, skill and creator docs.
- The opt-in model path adds one network call type, inside the existing PRD rule (user-initiated BYOK calls). The PRD's network list wording is updated to name it.
- The tray window (ADR-026 section 6) shows the local suggestion and its "Why this?" line, never the model path.

## Alternatives considered

- **A learned model on device:** opaque, hard to explain, needs far more data than one person produces. A readable rule with "Why this?" is better here.
- **Let the model see names and descriptions:** better ideas, but third-party text could steer the model and the user's provider would learn what they have installed. Rejected.
- **Call the model automatically on Home:** breaks the "network only when the user starts it" rule. Rejected.
