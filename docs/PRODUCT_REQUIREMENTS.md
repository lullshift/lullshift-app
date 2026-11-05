# Lullshift - Product Requirements & Strategy

**Version:** 1.1 draft | **Date:** 2026-11-04 | **Status:** Implementation handoff

> **Vision:** The human attention layer for asynchronous AI work. While AI agents work, Lullshift gives people a safe choice to disconnect or stay in flow and helps them return with context.

## 1. Executive summary

Lullshift is a cross-platform, local-first desktop companion for people who delegate work to AI agents. It observes supported agent lifecycle events, lets users choose interruptible activities, captures context and decisions, and brings them back when human attention is needed. The initial audience is developers using coding agents; a generalized adapter protocol will allow a later expansion to research, operations, sales, marketing, analytics and other business agents.

**Non-negotiable commercial decision:** **A Lullshift account is mandatory for every user, including Community/Open Source**. Users may register with verified email, Google, GitHub or Apple. Authentication is hosted even though Community features operate locally. There is no fully anonymous first-run mode. After successful initial sign-in, local features continue during temporary outages through a bounded offline entitlement lease. This makes the product *local-first, not account-free or cloud-independent at activation*.

The open-source desktop software, agent protocol, local capabilities and activity SDK use **Apache-2.0**. Commercial hosting adds sync, mobile pairing/push, managed AI, organization controls and billing. Commercial modules must never degrade essential local agent awareness or hold users' local data hostage.

## 2. Problem, principles, and users

### Problem

Agent execution creates unpredictable waiting intervals. People either watch progress, repeatedly check for input, context-switch into unrelated work, take a break, or prepare next decisions. The difficulty is not merely boredom: **uncertainty about when attention will be needed** and **the cost of reconstructing the task afterward**.

### Product principles

1. **Human attention first:** No obligation to maximize every idle minute; play, rest and thinking are equally valid.
2. **Accurate signals:** Never imply a whole task is done merely because one model turn stopped.
3. **Interruptibility by design:** Activities can pause, persist or terminate cleanly when an agent needs input.
4. **Local data ownership:** Task context, logs, prompts and files remain local by default; upload requires explicit opt-in.
5. **Required account, graceful offline behavior:** Initial sign-in and account verification required; core features are usable offline after activation for a defined lease.
6. **Agent-neutral:** Adapters supply a common event contract and declare capability limitations.
7. **No surveillance:** Do not build individual time-on-task scoring, employee ranking or hidden employer monitoring.
8. **Accessible and lightweight:** Tray-first interaction; keyboards, screen readers, reduced motion and clear notifications.

### Personas

| Persona | Job to be done | Initial priority |
|---|---|---|
| Coding-agent developer | Delegate implementation, notice approvals, resume reviews | P0 |
| Technical lead | Track multiple agents, preserve intent, prepare next decisions | P0 |
| AI power user | Delegate long-running research/analysis tasks | P1 |
| General business professional | Monitor supported asynchronous enterprise agents | P2 |
| IT administrator | Manage deployment, identity, safety and policies | Commercial P2 |

**Positioning:** Lullshift manages the *human experience around agent work*, not the agent's execution, authorization or orchestration.

## 3. Experience model

Two visible top-level modes:

- **Disconnect**: Arcade (interruptible mini-games and puzzles), Break (step away, stretching/rest, gentle notices).
- **Stay in Flow**: Next Move (prepare follow-up actions and questions), Companion (separate conversational workspace).

**Context Keeper** operates across both modes with user-approved task metadata and captures user notes, last intent, assumptions and review checklist. Users may disable individual activities. Academy/structured education is **out of scope** and may be reconsidered as a future community activity.

### Agent states

`unknown -> idle -> working -> needs_attention | completed | failed | cancelled` with explicit event source, timestamps and confidence. Multiple simultaneous sessions can have different states. Completion is classified by adapter semantics; transient tool events are not task completion. Notifications distinguish `needs_attention` from `completed`.

### Core journey

1. User installs Lullshift and signs in or creates an account via email, Google, GitHub or Apple.
2. User explicitly approves the addition of agent hooks/integrations; Lullshift backs up existing hook configuration and performs a reversible install.
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

### 4.2 Context Keeper (P0/P1)

- Local task bookmark including task title, intent, workspace, session link, last meaningful notes and decisions.
- Explicit user consent before reading task content; metadata-only default.
- Return-to-work card at agent attention/completion with editable notes and review checklist.
- Per-session deletion, retention settings and export; optional encrypted commercial sync.
- No default repository indexing, transcript scraping or source-code upload.

### 4.3 Next Move (P1)

- Draft follow-up questions, acceptance criteria, test ideas and decision notes while agent works.
- Queue text for **user-reviewed copy/paste** into agent; never inject commands into active agent by default.
- User-controlled prioritization and link to originating agent session.

### 4.4 Arcade (P0/P1)

- 1-2 bundled lightweight games or puzzles for MVP.
- Pause/resume/save-state lifecycle; time-bounded activities and accessible keyboard input.
- Open activity SDK for community contributions with capability-scoped access.

### 4.5 Break (P0)

- Focus-friendly break suggestions, away mode, notification routing, opt-in reminders.
- Accessibility options; no medical advice, behavior manipulation or productivity guilt.

### 4.6 Companion (P1)

- Local transcript storage, BYOK integration (provider-neutral) and optional hosted AI later.
- Explicit context sharing scopes: no context / session summary / selected notes.
- Never grant agent execution privileges from a Companion conversation without explicit separate approval.
- Respect provider policies, tokens, rate limits and local credential storage.

### 4.7 Authentication and account (P0 for all editions)

- **Mandatory hosted account** even for free/open-source desktop binaries.
- Sign-up/sign-in via **verified email** (passwordless email link or OTP preferred) and **Google, GitHub, Apple** using supported OIDC/OAuth mechanisms; exact provider capabilities to be validated during integration.
- Link multiple identity providers to one Lullshift account only through explicit authenticated linking; prevent email-only automatic merging and account takeover.
- Desktop and mobile use external-browser Authorization Code + **PKCE**; never embed shared client secrets.
- OIDC identity broker/server issues Lullshift tokens; provider access tokens are not application sessions.
- Store refresh credentials in OS keychain/secure store; short-lived access tokens; rotation, revocation, per-device session management and secure logout.
- Profile minimalism: user ID, verified email, display name, linked providers, devices, plan and basic consent metadata.
- Account deletion/export and privacy notice; preserve local data unless user chooses to erase it, and clearly distinguish account closure from local data deletion.
- **Offline policy (proposed):** first activation requires Internet; successful sign-in caches a signed time-limited offline lease for **30 days** (configurable final policy). Local features remain functional during outages. On expiry, prompt for reauthentication; do not silently erase local user data. Local notifications continue while lease is valid. Entitlement check never requires a connection on every agent event.
- Required login is a conscious adoption/privacy tradeoff: open-source app remains inspectable and modifiable; hosted identity availability is a service dependency and should be documented prominently.

## 5. Packaging and entitlements

| Capability | Community - Apache 2.0 client | Pro hosted | Business hosted |
|---|---|---|---|
| Mandatory Lullshift login | Yes | Yes | Yes |
| Local agent monitor, sessions, notifications | Yes | Yes | Yes |
| Context Keeper, Next Move, Arcade, Break | Yes | Yes | Yes |
| Local Companion with BYOK | Yes | Yes | Yes |
| Offline use after initial sign-in | Time-limited lease | Time-limited lease | Time-limited lease |
| Open adapter/activity SDK | Yes | Yes | Yes |
| Multi-device sync, encrypted backup | No | Yes | Yes |
| Mobile paired notifications | No | Yes | Yes |
| Hosted AI usage allowance | No | Optional | Optional |
| Organization identity/policy/deployment | No | No | Yes |
| Enterprise support/SLAs | No | No | Optional paid contract |

**Illustrative pricing hypothesis:** Community $0, Pro ~$6/user/month, Business ~$12/user/month; subject to interview and willingness-to-pay tests. Mobile app can be distributed freely but paired commercial cloud services are paid. Confirm platform-store and provider requirements before launch.

## 6. Expansion beyond developers

Target developers in first two releases for strong distribution and accessible hook integrations. Build a domain-neutral event protocol from day one for supported long-running research, analytics, sales and operations agents later. General business users become relevant only where agents expose meaningful asynchronous events. Avoid manufacturing a waiting-time use case for rapid chatbot replies.

## 7. Privacy, security and trust requirements

- Default collection: adapter name, session ID, timestamps, status and user-approved task label. Minimize all transmitted telemetry.
- No background clipboard reading, screen recording, raw transcript upload or automatic repository browsing.
- Allow disabling diagnostics; no personal activity feeds for employers.
- Cloud sync must be end-to-end/client-side encrypted for sensitive notes; document the exception that hosted AI cannot process encrypted content unless user explicitly decrypts/submits selected content.
- Separate local agent privileges from account identity and payment state; cloud compromise must not lead to local code execution.
- Signed desktop releases/updates, supply-chain scanning, dependency pinning and repeatable builds.
- Users must consent to integrations and be able to uninstall hooks safely.

## 8. Nonfunctional requirements and initial quality bars

| Dimension | Initial release bar |
|---|---|
| Idle footprint | Target <150 MB steady-state resident memory on reference platforms (measure, do not promise) |
| Event processing | 95th percentile <1 second from local hook receipt to state update in lab tests |
| Reliability | No duplicate critical notifications after event replay; recover from process restart |
| Offline | Existing valid offline lease preserves free local features without cloud |
| Accessibility | Keyboard-only operation; screen-reader labels; reduced motion |
| Platform coverage | Windows, macOS, Linux; publish exact tested matrix |
| Privacy | No source files or raw prompts leave device without explicit permission |

## 9. Release scope and acceptance criteria

- **v0.1 (developer foundation):** mandatory account and all four login methods; device registration; Claude Code hooks; tray, state engine, local SQLite; notifications; Break; one game; signed offline lease.
- **v0.2:** multi-session hardening; Context Keeper, Next Move, return card, local exports/deletion.
- **v0.3:** Companion BYOK; adapter/activity SDK; Codex/Cursor capability-based integrations; plugin security model.
- **v1.0:** stable supported-platform releases, signed updater, accessibility, security audits, public documentation and onboarding.
- **Commercial later:** cloud sync/backup, Flutter mobile, billing, organization administration, managed AI and enterprise policy.

**Definition of done:** code, automated tests, smoke tests on supported OSes, threat-model review for changed boundaries, documentation, migration/recovery paths and acceptance demo. No sprint is complete on code generation alone.

## 10. Risks and decisions requiring validation

1. **Mandatory account friction:** test drop-off during installation; consider transparent offline-lease policy. Do not reintroduce an anonymous edition without product-owner approval.
2. **Identity provider differences:** Apple developer enrollment, GitHub email privacy and provider linking must be tested.
3. **Agent hook reliability:** plugin state only as accurate as provider events; unknown/stale status is valid.
4. **Desktop plugin security:** arbitrary JS/native plugins are deferred until permissions and sandbox exist.
5. **Business readiness:** do not promise integrations without supported lifecycle APIs.
6. **Hosted economics:** hosted AI and mobile push costs need usage caps and abuse controls.
7. **Name:** Lullshift is a proposed working name; trademark and domain clearance outstanding.

## 11. Metrics and validation

- Activation: install -> registration -> login -> adapter configured -> first agent observed.
- Meaningful weekly active use: sessions with notifications/activities and successful return.
- Agent-to-user attention latency, false-positive notices, session state precision.
- Context recovery self-rating; how often users open context card; opt-in rates.
- Return retention by 1/4 weeks; permission revocation; identity failure rate.
- Commercial conversion and net infrastructure cost per paying customer.

**Do not optimize for:** hours spent in Lullshift, activity surveillance or making leisure artificially productive.

## 12. References

- Claude Code skills/subagents: https://claude.com/blog/steering-claude-code-skills-hooks-rules-subagents-and-more
- OAuth 2.0 native app security: https://www.rfc-editor.org/rfc/rfc8252
- Tauri 2 documentation: https://v2.tauri.app/
- Tauri signed updater: https://v2.tauri.app/plugin/updater/

---
**Document owner:** Product | **Implementation source of truth:** this PRD and `TECHNICAL_ARCHITECTURE.md`; changes require a decision record.

---

## 2026-11-04 Product Revision: Creator Platform, Marketplace, and Commerce

### Product scope and commercial assumptions

Lullshift consists of three connected products: **Lullshift Desktop** (Apache-2.0 local-first host), **Lullshift Creator** (open SDK/CLI/simulator/LLM skills), and **Lullshift Marketplace** (hosted catalog and commerce). The operator is **expected to be a non-Turkish company**, but incorporation country is NOT yet decided. Do not lock legal merchant-of-record, tax, seller jurisdictions, banking arrangements, or production processor accounts until confirmed. Stripe Billing and Stripe Connect are the preferred first integrations **if the operator is eligible in a Stripe-supported jurisdiction**; they are not universal availability guarantees.

### Activity platform requirements

- Lullshift ships at least **two fully functioning bundled, original, interruptible games** and one Break activity with the installer. They must work offline after account activation within the defined offline lease.
- The platform supports **installed activity packages** independently of the desktop release cycle. A proposed `.lullpack` archive contains a strict `manifest.json`, immutable static web assets, content data, and optional media. No arbitrary native modules, shell access, or Node.js runtime in third-party packages.
- Users can discover, install, update, disable, uninstall, and pin versions of packages; installed progress must survive updates/uninstall only according to clear retention options.
- Every activity conforms to pause/resume/state serialization and accessible input requirements. The host, not the activity, owns priority interrupt events.
- Package manifests declare canonical ID, semver version, publisher, entrypoint, host compatibility, minimum SDK version, permissions, category, offline behavior, and interruptibility.
- Packages must be immutable by version, hash verified, publisher signed and platform approved before distribution. Signing does not replace malware screening, runtime sandboxing, or human moderation.
- **Activity execution** is confined to a security-reviewed isolated WebView/origin or equivalent sandbox. No privileged Tauri bridge, access to agent credentials or filesystem, unrestricted network access, or raw agent transcript access. Allowlist granular, capability-scoped host APIs; deny by default.
- Catalog search and free installs require the mandatory Lullshift account. Locally installed entitled content remains usable under the stated offline policy.
- The Activity SDK and local package tooling are community open source; the official catalog, identity, moderation and payment services are hosted.

### AI-assisted creators

**Creator journey: Describe -> Generate -> Preview -> Simulate -> Validate -> Package -> Submit -> Review -> Publish -> (optionally) Earn.**

- Provide an official Claude Code `activity-creator` skill with reproducible templates and prompts, plus Codex/agent-skills-compatible assets, Cursor rules and model-agnostic Markdown prompts. These must generate a complete, testable activity, not just game UI.
- The open-source `lullshift` CLI shall provide `create`, `dev`, `simulate`, `validate`, `pack`, and `publish` interfaces, subject to final CLI ADR. `publish` authenticates against the user's Lullshift account; the other commands work locally.
- The simulator generates sequences of `working`, `needs_attention`, `completed`, `failed`, `cancelled`, duplicate/out-of-order events, forced pause, window close, and resume. Require deterministic state save/restore tests before submission.
- Publishing metadata includes category, screenshots, description, support URL, age suitability, accessibility declaration, third-party license inventory, AI-assisted-generation disclosure, permissions rationale and owner attestation.
- AI-created submissions are treated as **untrusted code**. Require scanning, dependency inventory, sandbox tests, manual review thresholds, malware-response workflow, complaint/takedown and emergency package revocation.
- Creators retain only rights they actually own and grant a distribution license to Lullshift; open SDK licensing does NOT force creators to license their own activities as open source. No claims of copyright exclusivity for purely AI-generated content.

### Marketplace roles and listings

- Roles: buyer, free publisher, verified paid publisher, moderator, marketplace admin, finance operator. Minimize privileges and maintain audit logs.
- Listings: **free** and **one-time paid** in the first monetized release. Tips, creator subscriptions, bundles and promotions are deferred pending compliance and economics.
- Registered free publishers may submit free activities subject to review. Paid sellers must complete identity/business verification and connect a supported payout account. Country and eligibility restrictions must be displayed before paid onboarding.
- Marketplace browsing, free installs, purchased-entitlement use and platform SDK remain available to Community accounts. **Pro is not required to purchase an activity**.
- The listing page shows required permissions, offline behavior, supported platforms, creator, price with relevant tax disclosure, update/support policy, ratings, version history, and a report action.
- Version releases are immutable; content is not silently swapped. Moderators may delist, block future installs, and revoke malicious content subject to documented safety and appeal policies.

### Payments, ownership, and monetization

**Three flows:** platform Pro/Business subscription, one-time creator purchase, creator settlement/payout. Hosted commerce and identity are mandatory for transactions, but entitlement checks are independent of payment-provider availability once an authorized license is issued.

- Preferred initial payments approach: **Stripe Billing + Stripe Checkout/Payments + Stripe Connect**, conditional on operator entity, merchant eligibility, supported seller geographies, and written tax/legal review. Use a provider-independent commerce interface; never hard-code Stripe IDs into domain aggregates.
- Proposed marketplace commission: **15% of an explicitly defined commission base**, provisional. Decide tax-inclusive/exclusive base and allocation of processing fees, refunds, disputes, FX and chargebacks before real-money launch. Example shares must not be represented as guaranteed net seller payouts.
- Proposed editions: Community free, Pro and Business paid; prices are hypotheses, not contractual commitments.
- At checkout snapshot seller, listing, immutable package version, currency, gross/tax/discount amounts, commission policy and merchant-of-record responsibility. Determine merchant of record by jurisdiction and legal design, not by checkout branding or API choice.
- Grant entitlements only after **verified webhook/provider confirmation**, never from browser redirect. Make webhook processing idempotent, replay-resistant, and reconciled. Support refunds and disputes that adjust entitlements and seller balances according to transparent policy.
- Prefer licensed payment-provider-managed seller onboarding, KYC/KYB and payouts. Lullshift does not directly custody pooled creator funds. Cross-border payout eligibility must be verified per creator country.
- Apply VAT/GST/sales-tax requirements based on legal merchant, sale geography and content; do not assume Stripe Tax substitutes for legal registrations or invoicing requirements.
- Initial **mobile** release is a companion, with no in-app purchase or marketplace checkout. Review Apple/Google rules before adding digital-content purchasing flows.
- Security: no card data handled or stored by Lullshift; use hosted checkout, provider tokenization, scoped webhook secrets, ledger-style accounting records, double-entry or independently reconcilable balances, integer minor units and explicit currency codes.

### Commercial launch gates

1. Incorporation and merchant-of-record model approved with qualified counsel/accountant.
2. Stripe operator-account eligibility confirmed, including Connect availability and intended charge/transfer type; verify every initial seller country.
3. Terms, privacy, publisher agreement, payout agreement, refund policy, content ownership, moderation/takedown and seller support policies signed off.
4. Payment webhooks, duplicate messages, stale events, missing settlement, refund, partial refund, dispute and chargeback simulations pass.
5. Purchase -> verified payment -> entitlement -> signed package installation -> offline access -> refund/revocation lifecycle has automated end-to-end evidence.
6. App-store billing/compliance reviewed before any paid mobile digital goods features.

### Roadmap impact

- Sprint 04: first bundled games and lifecycle reliability, **not** unreviewed external packages.
- Sprint 06: core Activity SDK and local authoring contract.
- Sprint 10: package format, sandbox and local creator toolchain.
- Sprint 11: official free catalog, upload and security-review pipeline.
- Sprint 12: Stripe subscriptions and entitlement/billing foundations.
- Sprint 13: Stripe Connect marketplace sales, seller onboarding, payout reconciliation and moderation operations.
- Sprint 14: creator growth, third-party skill templates, feedback and commercial hardening.

### Success measures

Developer return-to-task latency and self-reported interruption burden; activity launches/pauses/resumes without state loss; opt-in context use; creator submission-to-approval time; safe-install and update success; 7/30-day retention; catalog search-to-install conversion; verified purchase and payout reconciliation rates. Never expose employee monitoring dashboards.
