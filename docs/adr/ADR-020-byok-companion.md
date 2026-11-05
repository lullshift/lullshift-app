# ADR-020: BYOK Companion: providers, keys and the context boundary

- **Status:** Accepted (Sprint 09), pending product-owner review (see `sprints/DECISIONS_FOR_REVIEW.md`)
- **Date:** 2026-11-05
- **Owner:** Implementation lead
- **Related:** ADR-001 (no accounts or hosted services), ADR-013 (Context Keeper and opt-in capture), PRD §4.6, TECHNICAL_ARCHITECTURE §5 and §6

## Context

Companion is a separate conversational workspace that talks to an AI provider using the user's own key. It is the only feature that sends user-written text off the machine, and the key is the only secret the app holds. Requirements: keys only in the OS keychain; explicit context scopes (none, session summary, selected notes); honest network, rate-limit and budget handling; and Companion never gains agent execution privileges.

## Decision

### Providers

- Three provider kinds in v1:
  - `anthropic`: the Messages API at `https://api.anthropic.com`.
  - `openai`: Chat Completions at `https://api.openai.com`.
  - `openaiCompatible`: Chat Completions at a base URL the user enters. This is for other hosted providers and for local servers. The URL must be `https://`, except that `http://` is allowed for loopback only (`127.0.0.1`, `::1`, `localhost`), so local models work without TLS.
- A provider profile has an id, kind, label, base URL, model name and an optional monthly budget. The user types the model name; the UI offers suggestions but never assumes a model exists. There can be several profiles, and one is the default.
- Requests are made by the Rust core with a TLS-verifying HTTP client. The rules are:
  - redirects are not followed;
  - a 60-second timeout and a 2 MiB response cap apply;
  - requests are not streaming in v1;
  - a request can be cancelled from the UI;
  - there are no automatic retries.
- The WebView never talks to a provider and never sees a key.

### Keys

- Keys live only in the OS keychain: Windows Credential Manager, macOS Keychain, or the Secret Service on Linux. They are stored under service `app.lullshift.companion` and account `<profile id>`. SQLite stores no key material, not even a hash or a "last four" display hint.
- The UI sends a key once, through `companion_set_key`, which writes it straight to the keychain. No command ever returns a key. The UI only learns whether a key is set, by asking the keychain.
- In Rust, keys are held in a `Secret` type. Its `Debug` and `Display` are redacted, and it is zeroised on drop where the crate allows. Provider error bodies and transport errors are mapped to fixed, friendly messages. Any text that could echo a request header is scrubbed of the key before it reaches a log, an error or the UI.
- If the keychain is unavailable (for example, Linux without a Secret Service), Companion says so and does not fall back to any other storage.

### Threads and context scopes

- Threads and messages are stored locally in SQLite (new migration), with the provider profile id and the model used, and can be deleted.
- Each user message carries an explicit scope:
  - `none`, which is the default;
  - `sessionSummary` for one chosen agent session: its title, status, the user's intent and the latest ask, as captured by Context Keeper;
  - `selectedNotes`: notes and checklist items the user ticks from one session.
- Before sending, the UI shows the exact text that will go to the provider, the provider name and host, and the scope. Nothing is attached without that choice.
- What was attached is stored with the message, so the user can see later what was shared. The scope does not persist from one message to the next.
- Companion only reads Context Keeper data the user picked. It never reads captured prompts unless they are part of a chosen scope, and it never reads raw transcripts or files.

### Budgets and limits

- Usage (input and output tokens, as reported by the provider) is recorded per message.
- A profile may have a monthly limit in tokens. When it is reached, sending is blocked with a clear message until the month changes or the limit is raised.
- HTTP 429 shows a "rate limited" message with the provider's retry-after time if one is given. The user retries by hand.
- 401 and 403 mean the key was rejected. Offline, timeout, 5xx and invalid-response conditions each get their own honest message. A failed message stays in the thread, marked as not sent, with a "Try again" button.

### No execution privileges

- Requests carry no tools, functions or system prompts that claim agent access. Replies are rendered as plain text, with Markdown-like line breaks only: no HTML, no clickable links, and no buttons that act on agents, files or the shell. A test asserts that every request body has no tool definitions.
- Companion has no access to the agent IPC, the adapter installers or the marketplace.

### Network

- Network calls happen only when the user sends a message or presses "Test connection". Nothing calls a provider on launch.

## Consequences

- The keychain dependency (`keyring` crate or equivalent, chosen and pinned during implementation after checking its docs) is new native code on three OSes. Real keychain behaviour on macOS and Linux needs manual tests.
- Non-streaming replies feel slower. Streaming can be added later behind the same boundary.
- Keeping provider calls in Rust costs more code than calling from the WebView, but it keeps the key out of the WebView entirely.

## Implementation notes (Sprint 09)

- **Keychain:** `keyring` 4.2.0 (default features: Windows Credential Manager, macOS Keychain, Linux Secret Service; no file or mock backend in release builds). Tests use an in-memory store; one ignored test exercises the real keychain and was run on Windows.
- **HTTP:** `ureq` 3.5.1 with rustls and the platform certificate verifier (so corporate root certificates in the OS store work), instead of `reqwest`, whose copy in the lockfile comes from Tauri without TLS. Redirects off, 60 s timeout, 2 MiB cap; loopback `http` bypasses any proxy from the environment.
- **Wire formats:** Anthropic Messages with `anthropic-version: 2023-06-01`; OpenAI Chat Completions. The OpenAI shapes were written from knowledge of the API; the official page could not be fetched during implementation, so they need a check against a live provider (manual test).
- Further command-level details are in `docs/COMPANION_COMMANDS.md`, "Implementation notes and deviations".
