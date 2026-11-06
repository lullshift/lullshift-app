# Sandbox rules

Every code activity runs in an isolated frame with an opaque origin and a strict Content Security Policy (ADR-012). The sandbox, not the author or the tool that generated the code, is the security boundary. Treat all activity code, including AI-generated code, as untrusted.

## What an activity can't do

| Not allowed | How it is enforced |
|---|---|
| Network: `fetch`, XHR, WebSocket, EventSource, `sendBeacon`, remote images, fonts, scripts or frames | CSP `connect-src 'none'`; other sources limited to the activity's own files |
| File access, Node.js, native modules, shell | Not available in the web view |
| Clipboard, notifications | Denied |
| Tauri commands or any app IPC | No privileged capability is granted to activity frames |
| Cookies, `localStorage`, `sessionStorage`, IndexedDB | Denied by the opaque origin. Use the bridge's state and `storage` |
| Navigation, popups, the parent page | Denied |
| `eval`, `new Function`, workers | Denied by CSP |
| Inline `<script>`, inline styles, `style="..."`, `type="module"`, `import` | Denied by CSP. Use classic scripts and a stylesheet |
| Agent sessions, notes, transcripts, keys | Never passed to activities |

`lullshift validate` warns when it sees code that mentions blocked APIs (such as `fetch(` or `localStorage`), inline scripts or inline styles. That scan helps you learn early; it is not the boundary. `lullshift simulate` fails an activity that makes any network request.

## What an activity can do

- Draw and run in its own frame, from its own files.
- Talk to the host only through the bridge: `ready`, state saves, `summary` and, with the `activity.storage` permission, small string values.
- Receive `init` (restore, reduced motion, theme, pack content) and pause, resume and save requests.

## The host stays in control

- The host pauses an activity without its cooperation: it can freeze or remove the frame at any time.
- An activity that stops answering is closed as unresponsive and its last good save is kept.
- Activities can't read each other's saved state.

## Known limits

Be aware of these when reviewing activities:

- **WebRTC.** CSP doesn't govern WebRTC. In the Windows test run, `RTCPeerConnection` exists in the frame and gathers local network candidates. Because of this gap, and because the sandbox tests have not yet been run on macOS and Linux, **third-party code activities can't be installed from marketplaces in this release**. Bundled activities are reviewed for any use of `RTCPeerConnection`.
- Other items to settle before third-party code is allowed are listed under "Known gaps" in ADR-012.

## If you think you need more

Stop. Don't work around the sandbox. If an activity really needs a capability the bridge doesn't offer, open an issue to discuss it. New capabilities need an ADR and a security review.

## Reporting a sandbox escape

Follow [SECURITY.md](../../SECURITY.md). Don't open a public issue.
