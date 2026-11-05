# Sprint 03 - Notifications and Activity Host

**Goal:** Attention-aware notifications and a sandboxed activity host that the host itself can pause, with Break and one reference game.

**Depends on:** Sprint 02.

## Work items

- [x] S03-T01: Attention-priority native notifications, quiet hours and preferences
- [x] S03-T02: Activity host: isolated WebView/origin, CSP, no Tauri IPC/network/filesystem, nonce-bound lifecycle bridge, namespaced storage quota
- [x] S03-T03: Host-enforced pause/freeze on `needs_attention`, snapshot persistence and crash recovery
- [x] S03-T04: Break activity (away mode, gentle return) and one original reference puzzle game built on the bridge
- [x] S03-T05: Sandbox denial test suite; record per-OS results in an ADR (activity sandbox strategy)
- [x] S03-T06: Give activities their own origin with an explicit `frame-src` in the app CSP (never `'self'`) (Sprint 00 security review)
- [x] S03-T07 (Sprint 02 review follow-up): confirm Claude Code hook install natively from Rust (a dialog naming the settings file) so a script in a webview cannot install hooks without the user; add a test that only the `main` window holds the `claude-code-install`/`uninstall` permissions (ADR-010)

## Acceptance criteria

- [x] S03-A01: Agent attention pauses the running activity without its cooperation and without losing progress
- [x] S03-A02: Turn-stop is never shown as task completion
- [x] S03-A03: Sandbox denial tests pass on the dev OS; other OS results recorded or flagged blocked (Windows 42/42 with one known gap, WebRTC; macOS and Linux are manual test M03-5)
- [x] S03-A04: Keyboard and screen-reader smoke checks documented (keyboard checked automatically; the screen-reader pass is manual test M03-3)

## Guardrails

- No accounts, hosted services, default telemetry or stubs for `docs/future/` items (ADR-001).
- Treat agent hooks, marketplace repos and activity code as sensitive or untrusted input.
- Represent unknown agent status honestly; never lose local user data.
- Keep Windows, macOS and Linux compatibility in native code.

## Closeout

```text
Sprint: 03
Completed tasks (commits): uncommitted at closeout; to be committed after approval.
  - T01: notify.rs (only on a change into needs-you/completed/failed; turn end never notifies;
    quiet hours in local time), Settings > Notifications, preferences table.
  - T02/T06: lullact scheme with a strict CSP (now including `sandbox allow-scripts`), an
    opaque-origin iframe, app CSP frame-src for lullact only, bridge protocol v1 (nonce, source
    and origin checks, rate limit, size limits), storage quota enforced in Rust.
  - T03: freeze = save, then unload (800 ms cap); autosave every 15 s; agent-attention event;
    snapshots in SQLite (migration 0003); resume and crash recovery from the last snapshot.
  - T04: Step Away (break) and Lantern Pond (original puzzle) on the bridge.
  - T05: debug-only sandbox probe, tests/sandbox/run-denial-tests.mjs, ADR-012.
  - T07: native "Connect Claude Code?" dialog from Rust naming the settings file; tests that
    only main holds install/uninstall and that no capability exposes dialogs.
Commands run and results:
  - pnpm check: all steps passed (secret scan, schemas, types, typecheck, lint, format, TS
    tests 78 + 80, cargo fmt, clippy, cargo test 94 passed, desktop build).
  - node tests/sandbox/run-denial-tests.mjs --attention against the debug build (Windows 11,
    WebView2): 42/42 passed, 1 known gap (WebRTC). The new "keep playing" check failed on the
    pre-fix build, which reproduced the review's finding.
  - Activities checked over the debug port in light and dark: start, keyboard play, pause,
    resume with an identical board, focus on the paused card, no console errors.
Tests not run and why:
  - macOS and Linux sandbox and UI runs: no machine (M03-5).
  - Native install dialog "Connect" path and real notifications: they change real settings
    or show OS UI (M03-1, M03-2).
  - Screen reader pass (M03-3) and a real Claude Code attention pause (M03-4): manual.
Security/privacy review: security-qa-reviewer, adversarial.
  - Fixed: Keep playing re-paused at once (high); oversized messages; out-of-order saves;
    state accepted before init; null snapshot accepted by Rust; CSP sandbox header added.
  - Recorded in ADR-012 as Sprint 08 prerequisites: cross-activity load in its own frame,
    quota row overhead and Rust write budget, "nothing to save" reply, native confirmation
    for uninstall and notification changes.
  - No raw task data logged and no secrets stored.
Remaining defects and risks:
  - WebRTC is not governed by CSP (an exfiltration path). It blocks third-party code
    activities until closed.
  - Tauri injects its globals into frames. IPC from the frame is denied today, but that is
    Tauri behaviour; re-run the denial tests on every Tauri or wry upgrade.
  - WebKit has no out-of-process iframes, so a busy activity may slow the app on macOS and
    Linux (pause still works).
Acceptance checks passed/failed: A01 passed (automated, including a real hook event);
  A02 passed (unit tests: idle never notifies or shows as finished); A03 passed on Windows,
  other OSes flagged as manual; A04 keyboard passed, screen reader documented as M03-3.
Recommended next action: commit Sprint 03 as logical commits, then Sprint 04.
```
