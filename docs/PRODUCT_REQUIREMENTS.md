# Lullshift - Product Requirements & Strategy

**Version:** 2.0 | **Date:** 2026-11-05 | **Status:** Implementation handoff | **Decisions:** ADR-001, ADR-002, ADR-003

> **Vision:** The human attention layer for asynchronous AI work. While AI agents work, Lullshift gives people a safe choice to disconnect or stay in flow and helps them return with context.

## 1. Executive summary

Lullshift is a cross-platform, local-first desktop companion for people who delegate work to AI agents. It observes supported agent lifecycle events, lets users choose interruptible activities, captures context and decisions, and brings them back when human attention is needed. The initial audience is developers using coding agents; a generalized adapter protocol will allow a later expansion to research, operations, sales, marketing, analytics and other business agents.

**v1 is fully open source and account-free (ADR-001).** The desktop app, Rust core, agent protocol, activity SDK, CLI, kits, engines and all bundled games and activities are **Apache-2.0**. There is no sign-in, registration, device enrollment or offline lease: the app is fully functional on first launch. No Lullshift-operated server is required for any v1 feature, and there is no telemetry by default.

Activities are distributed through **Git-repository marketplaces (ADR-002)**: any public or private Git repo with a marketplace index can be added, using the user's own Git access. This repository is the built-in marketplace and bundles about **20 games and ~100 activities** as a release target (ADR-003).

## 2. Problem, principles, and users

### Problem

Agent execution creates unpredictable waiting intervals. People either watch progress, repeatedly check for input, context-switch into unrelated work, take a break, or prepare next decisions. The difficulty is not merely boredom: **uncertainty about when attention will be needed** and **the cost of reconstructing the task afterward**.

### Product principles

1. **Human attention first:** No obligation to maximize every idle minute; play, rest and thinking are equally valid.
2. **Accurate signals:** Never imply a whole task is done merely because one model turn stopped.
3. **Interruptibility by design:** Activities can pause, persist or terminate cleanly when an agent needs input.
4. **Local data ownership:** Task context, logs, prompts and files remain local; nothing is uploaded by Lullshift.
5. **No account, works offline:** Everything runs and is stored locally. The only network activity is user-initiated: Git fetches of marketplaces the user added, app update checks, and BYOK Companion calls to the provider the user configured.
6. **Agent-neutral:** Adapters supply a common event contract and declare capability limitations.
7. **No surveillance:** Do not build individual time-on-task scoring, employee ranking or hidden employer monitoring.
8. **Accessible and lightweight:** Tray-first interaction; keyboards, screen readers, reduced motion and clear notifications.
9. **Calm, beautiful and effortless:** A polished, calm and soft visual design and a UX where the next step is always obvious (product owner, 2026-11-05; `docs/DESIGN_SYSTEM.md`, ADR-011).

### Personas

| Persona | Job to be done | Initial priority |
|---|---|---|
| Coding-agent developer | Delegate implementation, notice approvals, resume reviews | P0 |
| Technical lead | Track multiple agents, preserve intent, prepare next decisions | P0 |
| Activity creator | Build and share games/activities, often AI-assisted | P1 |
| AI power user | Delegate long-running research/analysis tasks | P1 |
| General business professional | Monitor supported asynchronous enterprise agents | P2 |

**Positioning:** Lullshift manages the *human experience around agent work*, not the agent's execution, authorization or orchestration.

## 3. Experience model

Two visible top-level modes:

- **Disconnect**: Arcade (interruptible games and puzzles), Break (step away, movement, breathing, rest, gentle notices).
- **Stay in Flow**: Next Move (prepare follow-up actions and questions), Companion (separate conversational workspace).

**Context Keeper** operates across both modes with user-approved task metadata and captures user notes, last intent, assumptions and review checklist. Users may disable individual activities. Academy/structured education is **out of scope**; quizzes and teasers are play, not curriculum.

### Agent states

`unknown -> idle -> working -> needs_attention | completed | failed | cancelled` with explicit event source, timestamps and confidence. Multiple simultaneous sessions can have different states. Completion is classified by adapter semantics; transient tool events are not task completion. Notifications distinguish `needs_attention` from `completed`.

### Core journey

1. User installs Lullshift; first launch opens directly to optional agent-hook setup. No sign-in.
2. User explicitly approves adding agent hooks/integrations; Lullshift backs up existing hook configuration and performs a reversible, idempotent install. Skipping is allowed; the app remains usable.
3. Agent begins a task; local adapter emits normalized session events.
4. Lullshift shows agent state and optional choice: Disconnect or Stay in Flow.
5. Activity launches with a pause/save contract; local context snapshot is stored if approved.
6. Agent requests input; critical notification appears and activity pauses or is saved; user returns to agent.
7. When completion is confirmed, Lullshift displays a concise return card: original intent, outcome summary if known, open decisions, notes and recommended review actions.

## 4. Functional scope by component

### 4.1 Agent Awareness Engine (P0)

- Discover, register, enable and disable local adapters; Claude Code first, then Codex and Cursor subject to documented APIs.
- Capture lifecycle events through hooks/IPC; deduplicate, order and persist normalized events.
- Multi-session dashboard; precise `needs_attention`, terminal state and stale/unknown handling.
- Native desktop notifications, focus-aware quiet hours and user-level priority settings.
- Tray operation independent of UI window visibility; optional future daemon separation.
- Agent-specific capability reporting; avoid claiming unsupported remote control/approval.

### 4.2 Context Keeper (P0/P1, host feature)

- Local task bookmark including task title, intent, workspace, session link, last meaningful notes and decisions.
- Explicit user consent before reading task content; metadata-only default.
- Return-to-work card at agent attention/completion with editable notes and review checklist.
- Per-session deletion, retention settings and local export.
- No default repository indexing, transcript scraping or source-code upload.

### 4.3 Next Move (P1, host feature)

- Draft follow-up questions, acceptance criteria, test ideas and decision notes while agent works.
- Queue text for **user-reviewed copy/paste** into agent; never inject commands into active agent by default.
- User-controlled prioritization and link to originating agent session.

### 4.4 Arcade and Break activities (P0/P1)

- Bundled games and activities run in the sandboxed activity host (section 5) and follow the pause/resume/save-state lifecycle; time-bounded, keyboard-accessible, reduced-motion aware.
- Break offers focus-friendly break suggestions, away mode, notification routing and opt-in reminders, using movement, breathing, guided-pause and away-prompt content.
- No medical claims, behavior manipulation or productivity guilt.
- Puzzle games may offer a **daily seeded puzzle**, generated locally.

### 4.5 Activity platform and marketplaces (P0/P1)

See section 5.

### 4.6 Companion (P1, host feature)

- Local transcript storage and BYOK integration (provider-neutral); provider keys stored only in the OS keychain.
- Explicit context sharing scopes: no context / session summary / selected notes.
- Never grant agent execution privileges from a Companion conversation without explicit separate approval.
- Respect provider policies, tokens and rate limits; clearly show which provider receives data.

Context Keeper, Next Move and Companion need task context and/or AI access, so they are **host features, not SDK activities** (ADR-003).

## 5. Activity platform

### 5.1 Activity model (ADR-003)

Creators choose one of three tiers; kits and engines are optional conveniences, never required.

| Tier | What the author ships | Runs code? |
|---|---|---|
| **Code activity (custom)** | Any static web app (HTML/CSS/bundled JS, any framework) implementing the SDK lifecycle | Yes, in sandbox |
| **Code activity on a kit** | Game rules built on a first-party kit (grid, card, real-time arcade) | Yes, in sandbox |
| **Content pack** | JSON data (plus optional static media) for a named engine | **No** |

- **Kit:** first-party TypeScript library compiled into the activity bundle (grid: cells, selection, keyboard navigation, undo, seeded generators; card: deck, moves, undo; arcade: game loop, forced pause, resume countdown).
- **Engine:** first-party code activity that renders a family of experiences from data, versioned and referenced as `id@major`. Engine data schemas are public, versioned contracts; breaking changes need a new major and old majors keep running installed packs.
- **Content pack:** manifest with `kind: "content"` naming an engine and supplying data validated against its schema; contains no executable files.
- Manifests (`reference/ACTIVITY_MANIFEST_V1.schema.json`) declare canonical ID, semver version, author, `kind` (`app` requires entrypoint and runtime; `content` requires engine and data), host compatibility, minimum SDK version, permissions, category, offline behavior, interruptibility and asset licenses. Validation is strict.
- Every code activity implements `initialize/start/pause/resume/saveState/restoreState/dispose`. **The host, not the activity, owns interruption**: it enforces pause/freeze on `needs_attention` without relying on the activity's cooperation.

### 5.2 Sandbox

- Activities run in a security-reviewed isolated WebView/origin or equivalent. No privileged Tauri IPC, filesystem, agent credentials, raw transcripts or unrestricted network. Host APIs are capability-scoped, deny by default.
- No native modules, shell access or Node.js runtime in any activity.
- The sandbox must be proven on Windows, macOS and Linux before third-party **code** activities can be installed; content packs can be enabled earlier.

### 5.3 Git-repository marketplaces (ADR-002)

- A marketplace is any Git repository with `.lullshift/marketplace.json`; entries point to a folder in the same repo or to another Git repo. Any Git URL works; `owner/repo` is GitHub shorthand.
- Public and private repos are supported through the user's existing Git access (system `git`, credential manager, SSH keys, `gh`). Lullshift never asks for, stores or transmits Git tokens.
- Transport is plain `git clone`/`fetch` with a hardened configuration (no hooks, no `core.fsmonitor`, https/ssh/file only, no submodules/LFS) into an app-owned cache. Works offline after clone.
- **Installs are pinned to a commit SHA.** Updates are explicit user actions showing version change, source and any new permission requests. Branch heads are never executed directly.
- Users can add/remove marketplaces and install, update, disable and uninstall activities; retention of saved progress on update/uninstall is a clear user option.
- Install UI states that third-party marketplaces are **unverified** and shows source repo, commit, kind (content vs code) and permissions before install.
- Repository content is untrusted input: reject path traversal, escaping symlinks, oversized files, unknown permissions and invalid manifests.
- **Built-in marketplace:** this `lullshift-app` repository (`lullshift`). Its activities live in `activities/`, are bundled at the app release tag, always available offline, updated with the app, and can be disabled but not removed. It is the only reviewed marketplace (by pull request).
- Package signing is deferred past v1; commit pinning plus the sandbox is the v1 trust baseline.

### 5.4 Creator journey

**Describe -> Generate -> Preview -> Simulate -> Validate -> Pack -> Publish to a Git repo (public or private) -> users add the repo.**

- Official Claude Code `activity-creator` skill with reproducible templates and prompts, plus model-agnostic Markdown prompts (and Codex/Cursor-compatible assets where practical). Output must be a complete, testable activity or content pack, not just UI.
- Open-source `lullshift` CLI: `create`, `dev`, `simulate`, `validate`, `pack`, and marketplace-index helpers; all local, no account.
- The simulator generates `working`, `needs_attention`, `completed`, `failed`, `cancelled`, duplicate/out-of-order events, forced pause, window close and resume; deterministic save/restore tests are expected before publishing.
- There is no upload and no central review; contributing to the built-in marketplace is by pull request to this repo.
- **AI-generated code is untrusted.** Generated activities get no unrestricted filesystem, network or Tauri IPC; the sandbox, not the generator, is the security boundary.
- **Licensing:** the SDK, kits and engines are Apache-2.0, but creators choose the license of their own activities. Creators must hold the rights they publish and declare third-party asset licenses; no claims of exclusivity over purely AI-generated content. Bundled items are Apache-2.0 with compatible asset licenses.

## 6. v1 bundled content (release target)

**Target: 20 games and ~100 activities. This is a release target, not a release blocker**: core agent awareness and the activity platform gate v1.0; unfinished content ships in later app releases. Games use original names and art on generic mechanics; no recreation of specific branded games (names, characters, art, level designs, look). Puzzle/turn-based games are prioritised; real-time games use forced pause plus resume countdown.

### Games (proposed, 20)

| # | Mechanic | Kit |
|---|---|---|
| 1 | Number-placement grid logic | grid |
| 2 | Picture logic (nonogram-style) | grid |
| 3 | Mine deduction | grid |
| 4 | Merge tiles | grid |
| 5 | Pipe connect | grid |
| 6 | Light toggle | grid |
| 7 | Word grid | grid |
| 8 | Memory pairs | grid |
| 9 | Sliding puzzle | grid |
| 10 | Flood fill | grid |
| 11 | Tile matching | grid |
| 12-15 | Card patience variants (3-4) | card |
| 16 | Paddle & bricks | arcade |
| 17 | Snake-style | arcade |
| 18 | Dodge / runner | arcade |
| 19 | Typing race | arcade |
| 20 | Reserve slot (4th patience variant or new puzzle) | grid/card |

### Activities (proposed, ~100 content packs)

| Engine | Approx. packs | Notes |
|---|---|---|
| Breathing | 8 | Paced patterns; reduced-motion variants |
| Movement routine | 20 | Stretches, eye rest, posture |
| Guided pause | 12 | Calm/grounding scripts; no medical claims |
| Quiz | 25 | General and developer trivia; play, not curriculum |
| Brain teaser | 15 | Riddles, logic, lateral thinking |
| Reflection prompts | 8 | Short private prompts, stored locally |
| Creative toy | 8 | Doodle, pixel art, generative patterns, zen garden, ambient sound mixer |
| Typing drills | 5 | Accuracy and rhythm |
| Away prompts | 5 | Step-away suggestions |

## 7. Expansion beyond developers

Target developers first for strong distribution and accessible hook integrations. Build a domain-neutral event protocol from day one for supported long-running research, analytics, sales and operations agents later. General business users become relevant only where agents expose meaningful asynchronous events. Avoid manufacturing a waiting-time use case for rapid chatbot replies.

## 8. Privacy, security and trust requirements

- Local collection only: adapter name, session ID, timestamps, status and user-approved task label. **No telemetry by default**; any future diagnostics are opt-in.
- No background clipboard reading, screen recording, transcript upload or automatic repository browsing.
- No personal activity feeds for employers.
- Network access limited to user-initiated Git fetches, app update checks and BYOK Companion calls.
- Secrets held by the app are limited to BYOK provider keys, stored in the OS keychain only. Git credentials remain with the user's Git tooling.
- Third-party activities and marketplace content are untrusted; isolation, strict validation and commit pinning per section 5.
- Signed desktop releases/updates, supply-chain scanning, dependency pinning and repeatable builds.
- Users must consent to integrations and be able to uninstall hooks safely (backups, idempotence).

## 9. Nonfunctional requirements and initial quality bars

| Dimension | Initial release bar |
|---|---|
| Idle footprint | Target <150 MB steady-state resident memory on reference platforms (measure, do not promise) |
| Event processing | 95th percentile <1 second from local hook receipt to state update in lab tests |
| Reliability | No duplicate critical notifications after event replay; recover from process restart |
| Offline | Full operation offline at all times; network needed only for adding/updating marketplaces, update checks and Companion |
| Activity safety | Host-enforced pause on `needs_attention`; no state loss across pause/resume/restart |
| Accessibility | Keyboard-only operation; screen-reader labels; reduced motion |
| Platform coverage | Windows, macOS, Linux; publish exact tested matrix |
| Privacy | No source files or raw prompts leave device without explicit permission |

## 10. Release scope and acceptance criteria

Milestones follow `sprints/ROADMAP.md`:

| Sprint | Scope |
|---|---|
| 00 | Foundation: monorepo, schemas, local checks, ADRs |
| 01 | Desktop shell: tray, Rust reducer, SQLite, local IPC |
| 02 | Claude Code adapter (consented, reversible hook install) |
| 03 | Notifications, sandboxed activity host, Break, one reference game |
| 04 | Context Keeper and Next Move |
| 05 | Activity SDK, creator CLI/simulator, `activity-creator` skill |
| 06 | Game kits (grid, card, arcade) and first >=8 games |
| 07 | Engines and >=40 content packs |
| 08 | Git marketplaces (add/install/pin/update, built-in marketplace) |
| 09 | Companion (BYOK) |
| 10 | Codex/Cursor adapters; content completion toward 20 games / 100 activities |
| 11 | v1.0 release: installers, signed updater, cross-platform QA, audits, documentation |

**Definition of done:** code, automated tests, smoke tests on supported OSes, threat-model review for changed boundaries, documentation, migration/recovery paths and acceptance demo. No sprint is complete on code generation alone.

## 11. Risks and decisions requiring validation

1. **Agent hook reliability:** state is only as accurate as provider events; unknown/stale status is valid.
2. **Third-party code from Git repos:** no central review; the sandbox is the security boundary and must be proven per OS before code activities are installable.
3. **Content quality and scope:** 20 games / ~100 activities risks thin or inconsistent quality; numbers are a target, not a gate.
4. **Engine data-schema compatibility:** schemas are public contracts; breaking changes require new engine majors kept alongside old ones.
5. **Git availability:** users may lack `git` or credentials; built-in content must work without Git, and the UI must explain setup clearly.
6. **Trademark and originality of games:** generic mechanics only; review names, art and level design for resemblance to branded games.
7. **Business readiness:** do not promise integrations without supported lifecycle APIs.
8. **Name:** Lullshift is a proposed working name; trademark and domain clearance outstanding.

## 12. Metrics and validation

There is no telemetry, so measurement comes from opt-in feedback, user research, GitHub issues/discussions, and local-only statistics the user can view.

- Setup success: install -> adapter configured -> first agent observed (user research and issue reports).
- Agent-to-user attention latency, false-positive notices, session state precision (lab tests and reported issues).
- Activities pause/resume/restore without state loss (automated tests and simulator).
- Context recovery self-rating; usefulness of return card (user research).
- Creator success: time from description to a valid published pack; third-party marketplaces observed in public.

**Do not optimize for:** hours spent in Lullshift, activity surveillance or making leisure artificially productive.

## 13. Future (not in v1)

Deferred ideas: accounts/identity, sync and encrypted backup, mobile companion, organization/business features, hosted AI, a hosted catalog, paid listings/payments. Prior material is kept in `docs/future/` as a starting point, not an approved design. Any of these requires a new ADR and must be **additive**: no v1 feature may move behind an account. v1 code adds no abstractions, flags or stubs for them.

## 14. References

- ADRs: `docs/adr/ADR-001-open-source-v1-no-accounts.md`, `docs/adr/ADR-002-git-repo-marketplaces.md`, `docs/adr/ADR-003-activity-model-kits-engines-content.md`, `docs/adr/ADR-004-local-quality-checks-no-hosted-ci.md`
- Claude Code skills/subagents: https://claude.com/blog/steering-claude-code-skills-hooks-rules-subagents-and-more
- Tauri 2 documentation: https://v2.tauri.app/
- Tauri signed updater: https://v2.tauri.app/plugin/updater/

---
**Document owner:** Product | **Implementation source of truth:** this PRD and `TECHNICAL_ARCHITECTURE.md`; changes require an ADR in `docs/adr/`.
