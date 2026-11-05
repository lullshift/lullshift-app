# ADR-009: Local IPC transport and authentication

- **Status:** Accepted (Sprint 01)
- **Date:** 2026-11-05
- **Owner:** Implementation lead
- **Related:** ADR-008, architecture §5 (trust boundaries) and §6.1

## Context

Agent hook forwarders (Sprint 02) must deliver events to the running app. Any local process could try to inject fake events, so the endpoint must only accept the current user's forwarder. The workspace forbids `unsafe` Rust, which rules out hand-written Win32 security descriptors.

## Decision

### Transport

- **Windows:** named pipe `\\.\pipe\lullshift-ipc-v1-<nonce>`, where `<nonce>` is 128 random bits chosen at each launch. Named pipes are machine-global, so a name derived from the user's profile would let another local user pre-create it and receive forwarders' tokens. Only `endpoint.json`, in the user's own data directory, reveals the name. The server creates the pipe with `first_pipe_instance(true)` and rejects remote clients. (Changed after the Sprint 01 security review; the first draft used a hash of the data directory.)
- **macOS/Linux:** Unix domain socket `ipc.sock` inside `<app data>/ipc/`, created with directory mode `0700` and socket mode `0600`. A stale socket file is removed only after a connection attempt fails.
- Implemented with `tokio` (`net::windows::named_pipe`, `net::UnixListener`); no extra IPC crate.

### Authentication

- On first start the app generates a **per-install token** (32 random bytes, hex) and stores it in `<app data>/ipc/token`. That folder sits under the user profile; on Unix the file is `0600`.
- Every request carries the token. It's compared in constant time, and a wrong or missing token is rejected and counted.
- **Rotation:** deleting the file and restarting the app makes a new token. A UI action comes later.
- The endpoint's name and protocol version are written to `<app data>/ipc/endpoint.json` for forwarders to discover. That file contains no token.

This is defence in depth: the user-only location protects the token, the token authenticates each request, and the payload is still fully validated.

### Protocol (v1)

- **One request per connection:** a single line of JSON, `{"v":1,"token":"<hex>","event":{...agent event v1...}}\n`.
- **Reply:** one line, `{"ok":true}`, `{"ok":true,"duplicate":true}` or `{"ok":false,"error":"<code>"}`. Error codes are fixed strings and never echo input: `too_large`, `timeout`, `malformed`, `unauthorized`, `invalid_event`, `rate_limited`, `internal`.
- **Limits:**
  - 64 KiB per request, enforced while reading, before parsing.
  - 2-second read timeout.
  - At most 16 concurrent connections.
  - A global rate limit of 100 events per second (token bucket).
- The forwarder must fail fast and never block the agent if the app isn't running (Sprint 02).

### Logging

Only counters and error codes are logged, never request content or tokens.

## Alternatives

- **Loopback HTTP:** any local process, including browsers, can reach it, and it invites CSRF-style attacks.
- **Explicit Windows pipe DACL via `unsafe` Win32 calls:** stronger owner-only ACL, deferred. The token already provides authentication; revisit if a review finds the default pipe ACL insufficient.

### Health

If the accept loop can't create a new pipe instance or accept a socket connection, it retries with backoff (50 ms doubling to 5 s). Until it recovers, the dashboard shows agent status as unknown.

## Consequences

- Forwarders need read access to the token file, which they have as the same user.
- Another process running as the same user can read the token. Same-user isolation is not a goal: such a process could already edit the user's agent configs.
- Forwarders must read `endpoint.json` for each send, because the pipe name changes at every launch.
- **Known residual risk (Sprint 01 review):** the pipe keeps the default DACL. Another local user who learns the name could open connections and hold slots, delaying events by up to the 2-second timeout. They still can't authenticate, and with random names they must learn the name first. An owner-only DACL, via a small audited `unsafe` Win32 crate, is to be reconsidered in Sprint 02 when the forwarders land. **Sprint 02 outcome (S02-T06):** default DACL kept for v1; see ADR-010. Pipe names can be listed by other local users, so the random name only stops guessing, not discovery.
- The event log grows without a retention limit until Context Keeper adds retention (Sprint 04).
