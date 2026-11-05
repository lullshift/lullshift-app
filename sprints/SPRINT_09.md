# Sprint 09 - Companion (BYOK)

**Goal:** A separate conversational workspace using the user's own AI provider key.

**Depends on:** Sprints 01 and 04.

## Work items

- [x] S09-T01: Provider-neutral BYOK settings; keys in OS keychain only
- [x] S09-T02: Companion threads stored locally; explicit context scopes (none / session summary / selected notes)
- [x] S09-T03: Network failure, rate limit and budget handling
- [x] S09-T04: Tests proving keys never reach SQLite, logs or error traces

## Acceptance criteria

- [x] S09-A01: BYOK key never appears in SQLite, logs or UI errors
- [x] S09-A02: Context sharing requires an explicit scope choice
- [x] S09-A03: Companion never gains agent execution privileges

## Guardrails

- No accounts, hosted services, default telemetry or stubs for `docs/future/` items (ADR-001).
- Treat agent hooks, marketplace repos and activity code as sensitive or untrusted input.
- Represent unknown agent status honestly; never lose local user data.
- Keep Windows, macOS and Linux compatibility in native code.

## Closeout

```text
Sprint: 09
Completed tasks (commits): local commits "Add the BYOK Companion core...", "Store Companion
  profiles, threads and usage...", "Add Companion to the app..." and this closeout. Local only,
  not pushed.
Commands run and results:
  pnpm check (full, Windows 11): 11 of 14 steps passed on the first run; secret scan (a fake
    credential in a refusal test, now allowed on that line), lint (a setState-in-effect in
    App.tsx, replaced by state adjusted during render) and format (capabilities/default.json)
    were fixed and re-run: pass.
  cargo test --workspace: all pass (desktop lib 116, companion 26 + 15 HTTP mock-server tests,
    local-store incl. migrations 8 and 9 upgrade/backup/rollback). Real-keychain test (ignored
    by default) run once on Windows: pass, no entry left behind.
  TS tests: desktop 740 (41 Companion), agent-protocol 177, kits 82, SDK 18, CLI 43: pass.
  Debug app against a local fake OpenAI-compatible provider: profile, key save (input cleared),
    Test connection, two sends ("Also sends 2 earlier messages" matched the request), session
    summary preview matched the text sent byte for byte, scope reset to No context, a reply with
    a URL and HTML rendered as plain text, the key only in the Authorization header and found
    nowhere in the app data folders (database, WAL, backups, WebView storage); test profile,
    conversation and keychain entries removed afterwards. The real database upgraded 8 -> 9
    with a backup and all data intact.
Tests not run and why:
  A real provider with a real key, including the OpenAI wire format, which was written without
    the official page to hand (M09-1). Context with real notes and items (M09-2). Screen reader
    and offline (M09-3). macOS and Linux keychains (M09-4).
Security/privacy review: security-qa-reviewer, no critical or high. Fixed M1 (cancel freed the
  busy lock and skipped usage while the request ran; now held until the worker ends, usage
  booked or estimated), L1 (send carries the previewed context hash; a change is refused before
  anything is stored or sent), L2 (bidi/invisible characters stripped from replies), L3 (history
  only replayed to the same host, shown in the composer; migration 9), L4 (key and auth header
  in zeroizing strings), I1 (keychain not read at launch; Companion mounts on first visit), I2
  (retry refused after a provider address change). Each with a regression test.
Remaining defects and risks:
  hasKey reads the secret (keyring 4.2 has no metadata check); ureq keeps its own header copy.
  Usage estimated for a cancelled call is lost if the app quits before the next Companion
    command writes it.
  Product choices made while the owner was away: DECISIONS_FOR_REVIEW.md items 19-20.
Acceptance checks passed/failed: A01 key never in SQLite, logs or UI errors (automated scans
  and the app check); A02 context needs an explicit scope with an exact preview; A03 no tools,
  plain-text replies, no path to agents, adapters or the marketplace. Manual checks pending.
Recommended next action: run M09-1 with a real key; review decisions 19-20; then Sprint 10.
```
