// Host emulator for `lullshift dev` and `lullshift simulate` (ADR-017). It behaves like the
// desktop app's ActivityHost (ADR-012): the same protocol code, limits, timeouts and pause rules,
// with an in-memory store instead of SQLite. `simulate` drives it through `window.sim`; `dev`
// adds buttons for the same actions.

import {
  RateLimiter,
  hostEnvelope,
  jsonSize,
  newNonce,
  parseActivityMessage,
  type HostMessage,
  type SummaryItem,
} from "@lullshift/activity-sdk/host";

interface Config {
  mode: "dev" | "simulate";
  activityOrigin: string;
  name: string;
  storage: boolean;
  /** Pack data for engines; null otherwise. */
  content: unknown;
}

type Phase = "idle" | "loading" | "running" | "pausing" | "paused" | "failed";

interface LogEntry {
  at: number;
  from: "host" | "activity";
  type: string;
  note?: string;
}

interface AgentEvent {
  id: string;
  session: string;
  type: "working" | "needs_attention" | "completed" | "failed" | "cancelled";
  ts: number;
}

const SAVE_TIMEOUT_MS = 800;
const READY_TIMEOUT_MS = 8000;
const AUTOSAVE_MS = 15_000;
const MAX_MISSED = 3;
const SNAPSHOT_MAX = 256 * 1024;
const STORAGE_QUOTA = 256 * 1024;
const VALUE_MAX = 64 * 1024;

const FRAME_ALLOW = [
  "camera",
  "microphone",
  "geolocation",
  "clipboard-read",
  "clipboard-write",
  "display-capture",
  "fullscreen",
  "payment",
  "usb",
  "serial",
  "hid",
  "bluetooth",
]
  .map((f) => `${f} 'none'`)
  .join("; ");

const config = (await (await fetch("/config.json")).json()) as Config;
const stage = document.getElementById("stage") as HTMLElement;

let phase: Phase = "idle";
let nonce = "";
let frame: HTMLIFrameElement | null = null;
let limiter: RateLimiter | null = null;
let counter = 0;
let missed = 0;
let launchedAt = 0;
let autosaveTimer = 0;
let snapshot: unknown = null;
let summary: SummaryItem[] | null = null;
let readyWaiter: ((ok: boolean) => void) | null = null;
const pending = new Map<string, (state: unknown) => void>();
const storage = new Map<string, string>();
const log: LogEntry[] = [];
const dropped: Record<string, number> = {};
const savedStates: unknown[] = [];
const agentSeen = new Set<string>();
const agentLatest = new Map<string, { ts: number; type: string }>();
let pauses = 0;

const now = () => performance.now();
const note = (from: LogEntry["from"], type: string, extra?: string) => {
  log.push({ at: Math.round(now()), from, type, note: extra });
  if (log.length > 2000) log.shift();
  renderLog();
};

function setPhase(next: Phase) {
  phase = next;
  const el = document.getElementById("phase");
  if (el) el.textContent = next;
  renderControls();
}

function post(message: HostMessage) {
  frame?.contentWindow?.postMessage(hostEnvelope(nonce, message), "*");
  note("host", message.type);
}

/** Saves a state like the app does: JSON, never null, at most 256 KiB. */
function persist(state: unknown): boolean {
  const size = jsonSize(state);
  if (state === null || state === undefined || size > SNAPSHOT_MAX) return false;
  snapshot = JSON.parse(JSON.stringify(state)) as unknown;
  savedStates.push(snapshot);
  renderSnapshot();
  return true;
}

function requestSave(): Promise<{ ok: boolean; state: unknown }> {
  counter += 1;
  const requestId = `s${counter}`;
  return new Promise((resolve) => {
    const timer = window.setTimeout(() => {
      pending.delete(requestId);
      resolve({ ok: false, state: undefined });
    }, SAVE_TIMEOUT_MS);
    pending.set(requestId, (state) => {
      window.clearTimeout(timer);
      resolve({ ok: persist(state), state });
    });
    post({ type: "save", requestId });
  });
}

function removeFrame() {
  frame?.remove();
  frame = null;
  window.clearInterval(autosaveTimer);
}

async function freeze(reason: string): Promise<{ saved: boolean; state: unknown; ms: number }> {
  const started = now();
  if (phase === "paused" || phase === "failed" || phase === "idle" || phase === "pausing") {
    return { saved: false, state: undefined, ms: 0 };
  }
  let saved = false;
  let state: unknown;
  if (phase === "running") {
    setPhase("pausing");
    post({ type: "pause" });
    ({ ok: saved, state } = await requestSave());
  }
  removeFrame();
  pauses += 1;
  note("host", "unloaded", reason);
  setPhase("paused");
  return { saved, state, ms: Math.round(now() - started) };
}

function crash() {
  removeFrame();
  note("host", "crash");
  setPhase("paused");
}

interface LaunchOptions {
  reducedMotion?: boolean;
  theme?: "light" | "dark";
  /** Fire a needs_attention event right after the frame is created, before `ready`. */
  attentionBeforeReady?: boolean;
}

function launch(opts: LaunchOptions = {}): Promise<{ ok: boolean; readyMs: number }> {
  removeFrame();
  nonce = newNonce();
  missed = 0;
  pending.clear();
  limiter = null;
  setPhase("loading");
  launchedAt = now();
  frame = document.createElement("iframe");
  frame.className = "frame";
  frame.title = config.name;
  frame.setAttribute("sandbox", "allow-scripts");
  frame.setAttribute("allow", FRAME_ALLOW);
  frame.referrerPolicy = "no-referrer";
  frame.src = `${config.activityOrigin}/index.html#n=${nonce}`;
  stage.replaceChildren(frame);
  const restore = snapshot;
  const ready = new Promise<boolean>((resolve) => {
    readyWaiter = resolve;
    window.setTimeout(() => resolve(false), READY_TIMEOUT_MS);
  }).then((ok) => {
    readyWaiter = null;
    if (!ok && phase === "loading") setPhase("failed");
    return { ok: ok && phase === "running", readyMs: Math.round(now() - launchedAt) };
  });
  pendingInit = {
    restore,
    reducedMotion: opts.reducedMotion ?? false,
    theme: opts.theme ?? "light",
  };
  if (opts.attentionBeforeReady) {
    void agent({ id: `early-${nonce}`, session: "sim", type: "needs_attention", ts: Date.now() });
  }
  return ready;
}

let pendingInit: { restore: unknown; reducedMotion: boolean; theme: "light" | "dark" } | null =
  null;

window.addEventListener("message", (event: MessageEvent) => {
  const win = frame?.contentWindow;
  if (!win || event.source !== win || event.origin !== "null") return;
  const clock = now();
  limiter ??= new RateLimiter(30, 120, clock);
  if (!limiter.take(clock)) {
    dropped.rate = (dropped.rate ?? 0) + 1;
    return;
  }
  const parsed = parseActivityMessage(event.data, nonce);
  if (!parsed.ok) {
    dropped[parsed.reason] = (dropped[parsed.reason] ?? 0) + 1;
    note("activity", "dropped", parsed.reason);
    if (parsed.reason === "size") void freeze("unresponsive");
    const data = event.data as { type?: unknown; requestId?: unknown } | null;
    if (
      parsed.reason === "field" &&
      data?.type === "storage" &&
      typeof data.requestId === "string"
    ) {
      post({
        type: "storage-result",
        requestId: data.requestId,
        ok: false,
        error: "invalid_request",
      });
    }
    return;
  }
  const m = parsed.message;
  note("activity", m.type);
  if (m.type === "ready") {
    if (phase !== "loading" || !pendingInit) return;
    post({ type: "init", ...pendingInit, content: config.content ?? null });
    setPhase("running");
    frame?.focus();
    readyWaiter?.(true);
    if (config.mode === "dev") startAutosave();
  } else if (m.type === "state") {
    if (phase !== "running" && phase !== "pausing") return;
    if (m.requestId) {
      missed = 0;
      pending.get(m.requestId)?.(m.state);
      pending.delete(m.requestId);
    } else {
      persist(m.state);
    }
  } else if (m.type === "summary") {
    if (phase === "running" || phase === "pausing") {
      summary = m.items;
      renderSummary();
    }
  } else if (m.type === "storage") {
    const reply = (ok: boolean, value?: string | null, error?: string) =>
      post({ type: "storage-result", requestId: m.requestId, ok, value, error });
    if (!config.storage) return reply(false, undefined, "not_permitted");
    if (m.op === "get") return reply(true, storage.get(m.key) ?? null);
    if (m.op === "remove") {
      storage.delete(m.key);
      return reply(true, null);
    }
    if (m.op !== "set") return;
    if (m.value.length > VALUE_MAX) return reply(false, undefined, "value_too_large");
    let used = 0;
    for (const [k, v] of storage) if (k !== m.key) used += k.length + v.length;
    if (used + m.key.length + m.value.length > STORAGE_QUOTA) {
      return reply(false, undefined, "quota_exceeded");
    }
    storage.set(m.key, m.value);
    reply(true, null);
  }
});

function startAutosave() {
  window.clearInterval(autosaveTimer);
  autosaveTimer = window.setInterval(() => {
    if (phase !== "running") return;
    void requestSave().then(({ ok }) => {
      missed = ok ? 0 : missed + 1;
      if (missed >= MAX_MISSED) void freeze("unresponsive");
    });
  }, AUTOSAVE_MS);
}

/**
 * Agent events, reduced like the app does (ADR-008): duplicates by id and events older than the
 * session's newest are ignored; a change into needs_attention pauses the activity.
 */
async function agent(e: AgentEvent): Promise<{ applied: boolean; paused: boolean }> {
  if (agentSeen.has(e.id)) {
    note("host", `agent ${e.type}`, "duplicate, ignored");
    return { applied: false, paused: false };
  }
  agentSeen.add(e.id);
  const last = agentLatest.get(e.session);
  if (last && e.ts < last.ts) {
    note("host", `agent ${e.type}`, "older than the newest, ignored");
    return { applied: false, paused: false };
  }
  agentLatest.set(e.session, { ts: e.ts, type: e.type });
  note("host", `agent ${e.type}`);
  if (e.type === "needs_attention" && last?.type !== "needs_attention") {
    const before = pauses;
    await freeze("attention");
    return { applied: true, paused: pauses > before };
  }
  return { applied: true, paused: false };
}

const sim = {
  config,
  launch,
  save: requestSave,
  freeze,
  crash,
  agent,
  phase: () => phase,
  snapshot: () => snapshot,
  setSnapshot: (s: unknown) => {
    snapshot = s;
    renderSnapshot();
  },
  summary: () => summary,
  savedStates: () => savedStates.slice(),
  dropped: () => ({ ...dropped }),
  log: () => log.slice(),
  pauses: () => pauses,
  resetAgents: () => {
    agentSeen.clear();
    agentLatest.clear();
  },
};
(window as unknown as { sim: typeof sim }).sim = sim;

// ---------- Dev controls ----------

function renderControls() {
  for (const b of document.querySelectorAll<HTMLButtonElement>("[data-when]")) {
    const when = (b.dataset.when ?? "").split(" ");
    b.disabled = !when.includes(phase);
  }
}

function renderSummary() {
  const el = document.getElementById("summary");
  if (!el) return;
  el.textContent = summary ? summary.map((i) => `${i.label}: ${i.value}`).join(" · ") : "none yet";
}

function renderSnapshot() {
  const el = document.getElementById("snapshot");
  if (el) el.textContent = snapshot === null ? "none" : JSON.stringify(snapshot, null, 2);
}

function renderLog() {
  const el = document.getElementById("log");
  if (!el || config.mode !== "dev") return;
  el.textContent = log
    .slice(-60)
    .map(
      (l) =>
        `${String(l.at).padStart(7)} ms  ${l.from.padEnd(8)} ${l.type}${l.note ? ` (${l.note})` : ""}`,
    )
    .join("\n");
  el.scrollTop = el.scrollHeight;
}

if (config.mode === "dev") {
  document.body.classList.add("dev");
  const title = document.getElementById("title");
  if (title) title.textContent = config.name;
  const theme = () => (document.getElementById("dark") as HTMLInputElement).checked;
  const reduced = () => (document.getElementById("reduced") as HTMLInputElement).checked;
  let seq = 0;
  const actions: Record<string, () => void> = {
    start: () => void launch({ theme: theme() ? "dark" : "light", reducedMotion: reduced() }),
    pause: () => void freeze("user"),
    attention: () =>
      void agent({
        id: `dev-${(seq += 1)}`,
        session: "dev",
        type: "needs_attention",
        ts: Date.now(),
      }),
    working: () =>
      void agent({ id: `dev-${(seq += 1)}`, session: "dev", type: "working", ts: Date.now() }),
    save: () => void requestSave(),
    crash,
    forget: () => {
      snapshot = null;
      storage.clear();
      renderSnapshot();
    },
  };
  for (const b of document.querySelectorAll<HTMLButtonElement>("[data-action]")) {
    b.addEventListener("click", () => actions[b.dataset.action ?? ""]?.());
  }
  renderControls();
  renderSummary();
  renderSnapshot();
}
