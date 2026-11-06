// Lullshift-owned file: lullshift-observer, plugin version 1 (experimental, OpenCode 2.x only).
// Written by Lullshift when you connect OpenCode. Disconnect in Lullshift removes it. Do not edit:
// Lullshift replaces this file when you repair the connection.
//
// Observe only. It listens to OpenCode's event stream and tells the Lullshift app that a session
// started, is working, needs you, finished or ended. It sends session ids and event kinds. It
// never changes OpenCode, never returns a hook, and never sends message text, file paths, tool
// inputs or outputs. (Your first prompt is sent only if you turned on Task context for OpenCode
// in Lullshift.) Every failure is swallowed, so Lullshift being closed or missing cannot affect
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
let inFlight = 0;

function remember(map, key, value) {
  if (map.size >= MAX_TRACKED && !map.has(key)) map.delete(map.keys().next().value);
  if (map instanceof Set) map.add(key);
  else map.set(key, value);
}

function send(message) {
  if (inFlight >= MAX_IN_FLIGHT) return;
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
  } catch {
    // Observing must never disturb OpenCode.
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
  if (typeof prompt === "string" && prompt.length > 0) message.prompt = prompt.slice(0, MAX_PROMPT);
  send(message);
}

function working(kind, sessionId, hint, prompt) {
  const now = Date.now();
  const entry = sessions.get(sessionId) || { at: 0, last: "" };
  // Coalesce: at most one "working" per second per session, unless it carries the prompt.
  if (!prompt && entry.last === "working" && now - entry.at < WORKING_EVERY_MS) return;
  remember(sessions, sessionId, { at: now, last: "working" });
  post(kind, sessionId, hint, prompt);
}

function once(kind, sessionId, hint) {
  const entry = sessions.get(sessionId) || { at: 0, last: "" };
  // A turn that ended after a failure keeps showing the failure.
  if (entry.last === kind || (kind === "turn_ended" && entry.last === "failed")) return;
  remember(sessions, sessionId, { at: entry.at, last: kind });
  post(kind, sessionId, hint);
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
      working(type, id, undefined, captureOn() && typeof text === "string" ? text : undefined);
      return;
    }
    case "permission.asked": {
      // Real signal: OpenCode is waiting for the user. Not coalesced, and the answer that
      // follows is a new "working".
      const entry = sessions.get(id) || { at: 0, last: "" };
      remember(sessions, id, { at: entry.at, last: "attention" });
      post(type, id);
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
