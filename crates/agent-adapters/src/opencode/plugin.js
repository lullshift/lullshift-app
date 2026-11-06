// Lullshift-owned file: lullshift-observer, plugin version 2 (experimental, OpenCode 2.x only).
// Written by Lullshift when you connect OpenCode. Disconnect in Lullshift removes it. Do not edit:
// Lullshift replaces this file when you repair the connection.
//
// Observe only. It listens to OpenCode's event stream and tells the Lullshift app that a session
// started, is working, needs you, finished or ended. It sends session ids and event kinds. It
// never changes OpenCode, never returns a hook, and never sends message text, file paths, tool
// inputs or outputs. (Your first prompt is sent only if you turned on Task context for OpenCode
// in Lullshift, and only the first prompt of each session.) Every failure is swallowed, so Lullshift being closed or missing cannot affect
// OpenCode.
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { join } from "node:path";

const EXE = "__LULLSHIFT_EXE__";
const DATA_DIR = "__LULLSHIFT_DATA_DIR__";

const ADAPTER = "opencode";
const WORKING_EVERY_MS = 1000;
const MAX_TRACKED = 256;
const MAX_IN_FLIGHT = 8;
const MAX_ID = 200;
const MAX_PROMPT = 4000;

// session id -> { at: last "working" sent (ms), last: last kind sent }
const sessions = new Map();
// Sessions started by another session (subagents) are not tracked as their own.
const children = new Set();
// Sessions whose first prompt has already been sent: later prompts never leave OpenCode.
const prompted = new Set();
let inFlight = 0;

function remember(map, key, value) {
  if (map.size >= MAX_TRACKED && !map.has(key)) map.delete(map.keys().next().value);
  if (map instanceof Set) map.add(key);
  else map.set(key, value);
}

// Returns whether a forwarder was started. A dropped message (too many in flight, or a spawn
// error) returns false so the caller does not remember it as sent.
function send(message) {
  if (inFlight >= MAX_IN_FLIGHT) return false;
  try {
    // Argument array, no shell. The child is never awaited and is detached from OpenCode.
    const child = spawn(EXE, ["hook", ADAPTER, "--data-dir", DATA_DIR], {
      stdio: ["pipe", "ignore", "ignore"],
      detached: true,
      windowsHide: true,
    });
    inFlight += 1;
    let done = false;
    const finish = () => {
      if (!done) {
        done = true;
        inFlight -= 1;
      }
    };
    child.on("error", finish);
    child.on("exit", finish);
    if (child.stdin) {
      child.stdin.on("error", () => {});
      child.stdin.end(JSON.stringify(message));
    }
    child.unref();
    return true;
  } catch {
    // Observing must never disturb OpenCode.
    return false;
  }
}

function sessionIdOf(event) {
  const id = event && event.data && event.data.sessionID;
  return typeof id === "string" && id.length > 0 && id.length <= MAX_ID ? id : undefined;
}

function captureOn() {
  try {
    return existsSync(join(DATA_DIR, "context-capture", ADAPTER));
  } catch {
    return false;
  }
}

// The message is built field by field from this list; nothing else is ever copied.
function post(kind, sessionId, hint, prompt) {
  const message = { v: 1, event: kind, sessionId };
  if (typeof hint === "string" && /^[a-z_]{1,32}$/.test(hint)) message.hint = hint;
  if (typeof prompt === "string" && prompt.length > 0) message.prompt = cutPrompt(prompt);
  return send(message);
}

// At most MAX_PROMPT code points; a surrogate pair is never split. The first slice only bounds
// the work: MAX_PROMPT * 2 code units always hold at least MAX_PROMPT whole code points.
function cutPrompt(text) {
  return Array.from(text.slice(0, MAX_PROMPT * 2))
    .slice(0, MAX_PROMPT)
    .join("");
}

function working(kind, sessionId, hint, prompt) {
  const now = Date.now();
  const entry = sessions.get(sessionId) || { at: 0, last: "" };
  // Coalesce: at most one "working" per second per session, unless it carries the prompt.
  if (!prompt && entry.last === "working" && now - entry.at < WORKING_EVERY_MS) return;
  // Remembered only if the message was really started: a dropped one is retried next time.
  if (post(kind, sessionId, hint, prompt)) {
    remember(sessions, sessionId, { at: now, last: "working" });
    if (prompt) remember(prompted, sessionId);
  }
}

function once(kind, sessionId, hint) {
  const entry = sessions.get(sessionId) || { at: 0, last: "" };
  // A turn that ended after a failure keeps showing the failure.
  if (entry.last === kind || (kind === "turn_ended" && entry.last === "failed")) return;
  if (post(kind, sessionId, hint)) remember(sessions, sessionId, { at: entry.at, last: kind });
}

function handle(event) {
  if (!event || typeof event.type !== "string") return;
  const id = sessionIdOf(event);
  if (!id) return;
  const type = event.type;
  if (type === "session.created") {
    const parent = event.data.parentID;
    if (typeof parent === "string" && parent.length > 0) {
      remember(children, id);
      return;
    }
    remember(sessions, id, { at: 0, last: "" });
    post("session.created", id);
    return;
  }
  if (children.has(id)) return;
  switch (type) {
    case "session.execution.started":
    case "session.tool.called":
    case "session.text.started":
      working(type, id);
      return;
    case "session.status": {
      const status = event.data.status && event.data.status.type;
      if (status === "busy") working(type, id, "busy");
      return;
    }
    case "session.inbox.enqueued": {
      const item = event.data.item;
      if (!item || item.type !== "user") return;
      const text = item.payload && item.payload.text;
      // Only the first prompt of a session is ever sent, and only with the consent marker.
      const first = !prompted.has(id) && typeof text === "string" && captureOn();
      working(type, id, undefined, first ? text : undefined);
      return;
    }
    case "permission.asked": {
      // Real signal: OpenCode is waiting for the user. Not coalesced, and the answer that
      // follows is a new "working".
      const entry = sessions.get(id) || { at: 0, last: "" };
      if (post(type, id)) remember(sessions, id, { at: entry.at, last: "attention" });
      return;
    }
    case "permission.replied": {
      const reply = event.data.reply;
      working(
        type,
        id,
        reply === "once" || reply === "always" || reply === "reject" ? reply : undefined,
      );
      return;
    }
    case "session.idle":
    case "session.execution.succeeded":
      once("turn_ended", id);
      return;
    case "session.execution.interrupted": {
      const reason = event.data.reason;
      once("turn_ended", id, typeof reason === "string" ? reason : undefined);
      return;
    }
    case "session.execution.failed":
      once("failed", id);
      return;
    case "session.deleted":
      sessions.delete(id);
      children.delete(id);
      prompted.delete(id);
      post(type, id);
      return;
    default:
      return;
  }
}

export default {
  id: "lullshift-observer",
  setup(ctx) {
    const abort = new AbortController();
    try {
      const events = ctx.event.subscribe({ signal: abort.signal });
      // Not awaited: OpenCode waits for setup to return before it starts sessions.
      void (async () => {
        try {
          for await (const event of events) {
            try {
              handle(event);
            } catch {
              // Skip this event.
            }
          }
        } catch {
          // The stream ended or failed; nothing to do.
        }
      })();
    } catch {
      // Not an OpenCode 2.x host: do nothing.
    }
    return () => {
      try {
        abort.abort();
      } catch {
        // Already stopped.
      }
    };
  },
};
