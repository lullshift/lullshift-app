# Companion commands (Sprint 09)

This is the contract between the Tauri backend and the React UI for Companion (ADR-020). All JSON uses camelCase. Every command returns `Result<T, CommandError>`, where `CommandError` is `{ kind, message }`. `message` is a short, friendly sentence. It never contains a key, a request header or a raw provider body.

`kind` is one of:

- `offline`
- `timeout`
- `rateLimited`
- `keyRejected`
- `noKey`
- `keychainUnavailable`
- `budgetReached`
- `providerError`
- `invalidResponse`
- `invalidInput`
- `notFound`
- `cancelled`
- `busy`
- `internal`

## Types

```ts
type ProviderKind = "anthropic" | "openai" | "openaiCompatible";

type ProviderProfile = {
  id: string; // generated
  kind: ProviderKind;
  label: string;
  baseUrl: string; // fixed for anthropic/openai; user-entered for openaiCompatible
  host: string; // shown as "Sends to <host>"
  model: string;
  monthlyTokenBudget: number | null;
  isDefault: boolean;
  hasKey: boolean; // asked of the keychain; false when the keychain can't be read
  keychainOk: boolean;
};

type Scope =
  | { kind: "none" }
  | { kind: "sessionSummary"; provider: string; sessionId: string }
  | { kind: "selectedNotes"; provider: string; sessionId: string; notes: boolean; itemIds: number[] };

type ContextPreview = { scope: Scope; text: string; chars: number; sha256: string | null }; // exactly what will be attached

type Message = {
  id: number;
  threadId: number;
  role: "user" | "assistant";
  text: string;
  attachedContext: string | null; // the text that was sent with it
  scopeKind: "none" | "sessionSummary" | "selectedNotes";
  host: string | null; // host[:port] it was sent to (user message) or came from (reply)
  status: "sent" | "failed" | "pending";
  error: { kind: string; message: string } | null;
  inputTokens: number | null;
  outputTokens: number | null;
  createdAt: string;
};

type Thread = {
  id: number;
  title: string;
  profileId: string | null;
  model: string | null;
  createdAt: string;
  updatedAt: string;
};

type Usage = { profileId: string; month: string; inputTokens: number; outputTokens: number; budget: number | null };
```

## Commands

- `companion_profiles() -> ProviderProfile[]`
- `companion_save_profile({ profile: { id?, kind, label, baseUrl?, model, monthlyTokenBudget?, isDefault } }) -> ProviderProfile`
  - Validates the base URL: https, or http for loopback only, with no userinfo, query or fragment.
- `companion_delete_profile({ id }) -> void`
  - Deletes the keychain entry too.
- `companion_set_key({ profileId, key }) -> void`
  - Writes to the keychain.
  - The key is trimmed, must be 8 to 512 printable ASCII characters, and is never echoed.
- `companion_clear_key({ profileId }) -> void`
- `companion_test({ profileId }) -> { ok: true, model: string }`
  - Makes one minimal request. It is only run when the user presses the button.
- `companion_threads() -> Thread[]`
- `companion_new_thread({ title? }) -> Thread`
- `companion_rename_thread({ id, title }) -> void`
- `companion_delete_thread({ id }) -> void`
- `companion_messages({ threadId }) -> Message[]`
- `companion_preview_context({ scope }) -> ContextPreview`
- `companion_send({ threadId, profileId, text, scope, expectedContextSha256, requestId, onEvent }) -> Message`
  - Sprint 12: the reply streams. `requestId` is chosen by the UI (8 to 64 characters of `A-Za-z0-9_-`) and `onEvent` is a Tauri `Channel`. Events (all carry `requestId`, and the UI ignores other ids): `started`, `round { round }`, `delta { text }` (already cleaned), `toolRequest { callId, tool, label, arguments, resultText, host }`, `toolSkipped { tool, text }`, `toolResolved { callId, approved }`. Also `toolNotice { reason, tool? }` (owner decision 27.6): `reason` is `unknown`, `malformed` or `limit`; `tool` only if a short safe identifier; never the model's arguments. Each message in `companion_messages` history carries `toolNotices` (`id, round, reason, tool, createdAt`) beside `toolCalls`. The promise still ends with the stored assistant message, or an error.
  - `expectedContextSha256` is the `sha256` of the preview the user confirmed: lowercase hex SHA-256 of the exact attached text, or `null` for scope `none`. The backend rebuilds the context and compares. On any mismatch (including a missing hash for a scope that attaches text, or a hash given for `none`) it returns `invalidInput` with "The context changed since you previewed it. Review it again.", stores nothing and makes no request. The UI then shows a new preview.
  - Only earlier messages that were sent to the same host (`host[:port]`) as this send are repeated as history. Messages that went to another host are never replayed to this one.
  - Returns the assistant message.
  - The user message is stored first, as `pending`, then marked `sent` or `failed`.
  - The context attached is built again on the backend from the scope. The text must match what the preview would show.
- `companion_retry({ messageId, requestId, onEvent }) -> Message`
  - Resends a failed user message with the same attached context. If the profile's host differs from the host stored on the message, it returns `invalidInput` ("This provider's address changed. Send the message again to review it.") and sends nothing.
- `companion_cancel({ threadId }) -> void`
  - Returns at once and frees the thread at once (Sprint 12, decision 20.1): a new send in the same thread can start immediately. The old worker is abandoned and anything it still receives is discarded (never stored, never sent to the UI). The cancelled call is still counted in usage when its worker finishes: the provider's numbers if reported, otherwise an estimate from the request and the text received. The cancelled message says the request may already have reached the provider.
- `companion_tool_decision({ requestId, callId, approve }) -> void`
  - Answers the lookup that a `toolRequest` event announced. Nothing is sent to the provider before this. `notFound` if nothing matching is waiting (already answered, cancelled, other call or request).
- `companion_prefs() -> { toolsEnabled, lastScope }`
  - `toolsEnabled` is false until the user turns lookups on. `lastScope` is the scope of the last message that passed validation, or null.
- `companion_set_tools_enabled({ enabled }) -> { toolsEnabled, lastScope }`
- `companion_usage() -> Usage[]`
  - Covers the current month.
- `companion_model_catalog({ mode: "cached" | "load" | "refresh" }) -> ModelCatalog | null` (decision 32)
  - `cached`: memory only. Returns the list loaded earlier in this app run, or `null`. Never contacts the network; the UI calls it when the form opens.
  - `load`: the list from memory, or one GET of `https://models.dev/api.json` if it isn't loaded yet (`fromMemory` says which).
  - `refresh`: one GET with `If-None-Match` (the stored ETag, if any). A 304 keeps the list and sets `unchanged: true`. A failed refresh keeps the old list in memory.
  - Concurrent `load`/`refresh` calls share one request (single-flight).
  - `ModelCatalog = { fetchedAt: string /* RFC 3339 UTC, when models.dev last sent or confirmed the list */, unchanged: boolean, fromMemory: boolean, providers: { id, name, modelCount }[] }`.
  - Errors: `offline` ("Can't reach models.dev…"), `timeout`, `invalidResponse` (not JSON, or larger than 16 MiB), `providerError` (HTTP status, named; the body is never included).
- `companion_catalog_models({ providerId }) -> CatalogModel[]` (decision 32)
  - Memory only. `notFound` when the list isn't loaded ("The model list isn't loaded…") or the provider id isn't in it.
  - `CatalogModel = { id, name, family: string | null, toolCall, reasoning, releaseDate: string | null /* YYYY, YYYY-MM or YYYY-MM-DD */, contextTokens: number | null, outputTokens: number | null, costInput: number | null, costOutput: number | null /* USD per 1M tokens */ }`.
  - Every `id` passes the profile model rule, so a chosen model always saves.

## Not exposed

No command returns key material. No command lets the frontend choose arbitrary headers, endpoints beyond the profile's base URL, tools or system prompts. The models.dev address is a constant in `crates/companion/src/models_dev.rs`; no command can change it. Whether the three read-only lookups are offered is decided by the stored `toolsEnabled` setting, never by an argument of `companion_send`.

## Implementation notes and deviations (backend, Sprint 09)

These are the places where the backend adds detail to the contract above or differs from it. The UI should rely on this section.

- **Argument names.** Tauri maps camelCase JS arguments to the Rust parameters: `{ profile }`, `{ id }`, `{ profileId, key }`, `{ threadId, profileId, text, scope }`, `{ messageId }`, `{ scope }`. `companion_new_thread` takes `{ title? }`.
- **Base URLs.** `anthropic` is fixed to `https://api.anthropic.com` (the backend adds `/v1/messages`). `openai` is fixed to `https://api.openai.com/v1` (adds `/chat/completions`). For `openaiCompatible` the user's base URL should include the version segment, such as `https://openrouter.ai/api/v1` or `http://localhost:11434/v1`; the backend adds `/chat/completions`. A `baseUrl` sent with `anthropic` or `openai` is ignored. A trailing slash is removed. Only `localhost`, `127.0.0.1` and `::1` may use `http`.
- **`companion_save_profile`.** If `id` is given it must exist (`notFound` otherwise). The first profile becomes the default whatever `isDefault` says. Asking for `isDefault: true` clears it on the others. A profile cannot be un-defaulted directly; make another one the default. Model names may use letters, digits and `. _ : / @ + -` only. `monthlyTokenBudget` must be 1 or more, or `null` for no limit. Up to 20 profiles.
- **Keys.** 8 to 512 characters, visible ASCII only, **no spaces** (stricter than "printable").
- **`companion_delete_profile`.** The keychain entry is deleted first. If the keychain refuses, the call fails with `keychainUnavailable` and the profile is kept. Usage rows are deleted. Threads and messages are kept, with their profile link cleared (`profileId: null`).
- **`hasKey` / `keychainOk`.** Read from the keychain on every `companion_profiles` call, no network. The UI calls it only after the Companion view is first opened, never at app launch (the keyring API in use has no metadata-only check, so each call reads the stored secret). If the keychain cannot be read, `hasKey` is `false` and `keychainOk` is `false`.
- **`companion_send` failure behaviour.**
  - Checks that fail *before* anything is stored return an error and store nothing: `invalidInput`, `notFound`, `noKey`, `keychainUnavailable`, `budgetReached`, `busy`.
  - Once the user message is stored, any failure (`offline`, `timeout`, `rateLimited`, `keyRejected`, `providerError`, `invalidResponse`, `cancelled`) returns the error **and** leaves the user message as `failed` with `error` set. The UI should call `companion_messages` again after an error to show it.
  - A send for a thread that is already sending returns `busy` before anything else.
- **`companion_retry`.** Only a `failed` user message that is the **latest message in its thread** can be retried (`invalidInput` otherwise), and its profile must still exist (`notFound` if it was deleted). Key and budget are checked again. The same attached context is resent; it is not rebuilt.
- **`companion_cancel`.** Resolves at once. The pending `companion_send` then returns `cancelled`, and the message stays `failed` with kind `cancelled`, so it can be retried. Cancelling a thread that is not sending does nothing. The thread is free as soon as this returns.
- **Messages carry `toolCalls`.** A user message has `toolCalls: { id, round, tool, label, arguments, resultText, status: approved|declined, card: { activityId, name, minutes } | null, host, createdAt }[]`: the lookups made while answering it, with exactly what was sent. A card is only for an approved `suggest_break`; starting the activity is the UI's explicit user action, never the lookup's.
- **Limits (Sprint 12).** 60 s to the first byte, 30 s of silence once started, 300 s per provider call, 128 KiB of reply text, 64 KiB per event, 4 MiB per stream; at most 3 rounds and 3 honoured lookups per message.
- **Restart.** Messages left `pending` by a quit or crash are changed to `failed` / `cancelled` at the next launch.
- **History sent to the provider.** The newest 20 delivered messages (at most 60,000 characters), as plain text. Failed messages are not included. Context attached to earlier messages is **not** repeated; it goes only with the message it was chosen for. The attached text is placed before the user's text: `<context>\n\n---\n\n<text>`.
- **System prompt.** One fixed sentence in `crates/companion/src/provider.rs` (`SYSTEM_PROMPT`): a chat companion that can only reply with text. With lookups on, `SYSTEM_PROMPT_WITH_TOOLS` instead. Output is capped at 1024 tokens per reply (16 for the connection test).
- **Scopes.**
  - `sessionSummary` text: a fixed header line, then Title (user title, else the agent's), Status (the last reported state and when it was reported, with a note that it may have changed), Intent (the user's, else the captured one) and Latest ask.
  - `selectedNotes` needs `notes: true` or at least one item id, otherwise `invalidInput`. Item ids that do not belong to the session give `invalidInput`. An unknown session gives `notFound`.
  - Text over 30,000 characters is cut with a visible marker. The preview shows the cut text.
- **`companion_test`.** Sends one request with the text "Reply with the single word: ok" and 16 output tokens. Its tokens count toward the budget, and it is refused when the budget is used up.
- **`Usage`.** One entry per profile for the current UTC month (profiles with no use show zeros). If a provider reports no usage, tokens are estimated from the text length so the budget still moves.
- **Budget rule.** Sending is blocked when this month's input plus output tokens are at or over the limit. A call that starts under the limit may end over it.
- **Reply text.** Bidi controls (U+202A-202E, U+2066-2069, U+200E-200F, U+061C) and invisible formatting characters (zero-width, soft hyphen, BOM, U+2028, U+2029 and the rest of `agent_core::text::is_invisible`) are removed. Control characters other than line breaks and tabs are removed, and replies are cut at 100,000 characters. Render as plain text.
- **Permissions.** Each command has its own `allow-companion-*` permission in `capabilities/default.json` (main window only).
