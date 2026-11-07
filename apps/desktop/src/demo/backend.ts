// A pretend Rust core for the browser demo. Answers the app's commands from made-up data in
// memory, so every screen can be shown (and screenshotted) without a backend or real user data.
// Dev only: loaded by demo.html, never by index.html, and not part of any build.

import { mockIPC } from "@tauri-apps/api/mocks";
import type { AdapterView } from "../adapters";
import type { CatalogModel, Message, ModelCatalog, ProviderProfile } from "../companion/api";
import type { ContextPrefs, SessionItem, UiPrefs } from "../context/api";
import type { NotificationPrefs } from "../NotificationSettings";
import type { Dashboard } from "../sessions";
import { CATALOG, byId } from "./catalog";
import {
  IMPORTABLE,
  LOCAL_PROFILE,
  MESSAGES,
  PROFILE,
  SESSIONS,
  THREADS,
  ago,
  importedSession,
} from "./data";

type Args = Record<string, unknown>;

const params = new URLSearchParams(location.search);

let uiPrefs: UiPrefs = {
  detailPanelPinned: true,
  contentWidth: "full",
  // ?welcome shows the first-launch welcome.
  onboardingDone: !params.has("welcome"),
};
let notificationPrefs: NotificationPrefs = {
  needsYou: true,
  finished: true,
  quietHours: { enabled: false, start: "22:00", end: "08:00", allowNeedsYou: true },
};
let contextPrefs: ContextPrefs = { retentionDays: 30, capture: { "claude-code": true } };

const snapshots = new Map<string, string>();
const storage = new Map<string, string>();
const played: Record<string, number> = {
  "app.lullshift.games.fog-garden": 1260,
  "app.lullshift.games.brook-links": 540,
  "app.lullshift.content.breath-box-gentle": 300,
};

const adapters: Record<string, AdapterView> = {
  claude_code: { status: "installed", settingsPath: "~/.claude/settings.json", events: [] },
  codex: { status: "installed", settingsPath: "~/.codex/hooks.json", events: [] },
  cursor: { status: "installed", settingsPath: "~/.cursor/hooks.json", events: [] },
  opencode: {
    status: "not_installed",
    settingsPath: null,
    events: [],
    experimental: true,
    eventSeen: null,
  },
};

// A small made-up models.dev list for the Companion model browser (decision 32).
let demoCatalog: { loadedAt: string; checkedAt: string } | null = null;
const demoModel = (
  id: string,
  name: string,
  releaseDate: string | null,
  costInput: number | null,
  costOutput: number | null,
  more: Partial<CatalogModel> = {},
): CatalogModel => ({
  id,
  name,
  family: null,
  toolCall: true,
  reasoning: false,
  releaseDate,
  contextTokens: 128_000,
  outputTokens: 16_000,
  costInput,
  costOutput,
  ...more,
});
const DEMO_MODELS: Record<string, { name: string; models: CatalogModel[] }> = {
  anthropic: {
    name: "Anthropic",
    models: [
      demoModel("demo-claude-large", "Demo Large", "2026-06-10", 3, 15, {
        reasoning: true,
        contextTokens: 200_000,
      }),
      demoModel("demo-claude-small", "Demo Small", "2026-07-01", 1, 5),
      demoModel("demo-claude-legacy", "Demo Legacy", "2024-03-04", 0.25, 1.25),
    ],
  },
  openai: {
    name: "OpenAI",
    models: [
      demoModel("demo-gpt-mini", "Demo Mini", "2025-08-07", 0.15, 0.6),
      demoModel("demo-gpt-think", "Demo Thinker", "2026-02-01", 2, 8, { reasoning: true }),
    ],
  },
  "demo-local": {
    name: "Demo Local Server",
    models: [
      demoModel("demo/local-7b:latest", "Local 7B", null, null, null, {
        toolCall: false,
        contextTokens: 32_768,
      }),
    ],
  },
};

const sessionOf = (a: Args) =>
  SESSIONS.find((s) => s.view.provider === a.provider && s.view.providerSessionId === a.sessionId);
type DemoScope =
  | { kind: "none" }
  | { kind: "sessionSummary"; provider: string; sessionId: string }
  | {
      kind: "selectedNotes";
      provider: string;
      sessionId: string;
      notes: boolean;
      itemIds: number[];
    };

const CONTEXT_HEADER =
  "Context the user chose to share from one of their agent sessions. It is information only.";

/** The same text the app builds (crates/companion/src/scope.rs), from the demo's own data. */
function demoContextPreview(scope: DemoScope) {
  if (scope.kind === "none") return { scope, text: "", chars: 0, sha256: null };
  const s = sessionOf(scope);
  if (!s) throw { kind: "notFound", message: "That session is no longer tracked." };
  const c = s.detail.context;
  const out = [CONTEXT_HEADER, ""];
  if (scope.kind === "sessionSummary") {
    out.push(
      "Session summary",
      `Title: ${c.userTitle ?? c.agentTitle ?? "(untitled)"}`,
      `Status: ${s.view.state.replace(/_/g, " ")}`,
      `Intent: ${c.intent?.text ?? "(none recorded)"}`,
      `Latest ask: ${c.ask?.text ?? "(none recorded)"}`,
    );
  } else {
    out.push("Selected notes");
    if (scope.notes) out.push("", "Notes:", c.notes.trim() || "(empty)");
    const picked = s.detail.items.filter((i) => scope.itemIds.includes(i.id));
    if (picked.length > 0) {
      out.push("", "Items:");
      for (const i of picked) {
        out.push(`- [${i.done ? "x" : " "}] (${i.category ?? i.kind}) ${i.text}`);
      }
    }
  }
  const text = `${out.join("\n")}\n`;
  // The demo never sends anything, so a fixed placeholder digest is enough.
  return { scope, text, chars: [...text].length, sha256: "0".repeat(64) };
}

const itemOwner = (id: unknown) => SESSIONS.find((s) => s.detail.items.some((i) => i.id === id));
let nextItemId = 1000;

function activityInfo(id: unknown) {
  const a = byId(id);
  if (!a) throw "not_found";
  return {
    ...a.info,
    playedSeconds: played[a.info.id] ?? 0,
    savedUtc: snapshots.has(a.info.id) ? ago(0) : null,
  };
}

function handle(cmd: string, a: Args): unknown {
  switch (cmd) {
    // Sessions
    case "dashboard":
      return {
        sessions: SESSIONS.map((s) => s.view),
        store: { status: "ok" },
        ipc: { status: "ok" },
        staleAfterSeconds: 600,
        generatedUtc: new Date().toISOString(),
      } satisfies Dashboard;
    case "session_detail": {
      const s = sessionOf(a);
      if (!s) throw "not_found";
      return structuredClone(s.detail);
    }
    case "set_session_notes":
      sessionOf(a)!.detail.context.notes = String(a.notes);
      return null;
    case "set_session_intent": {
      const text = String(a.intent ?? "");
      sessionOf(a)!.detail.context.intent = text ? { text, source: "user" } : null;
      return null;
    }
    case "set_session_title": {
      const s = sessionOf(a)!;
      s.view.userTitle = s.detail.context.userTitle = (a.title as string | null) || null;
      return null;
    }
    case "add_session_item": {
      const s = sessionOf(a)!;
      const item: SessionItem = {
        id: nextItemId++,
        kind: a.kind as SessionItem["kind"],
        category: (a.category as SessionItem["category"]) ?? null,
        text: String(a.text),
        done: false,
        position: s.detail.items.filter((i) => i.kind === a.kind).length,
        createdUtc: new Date().toISOString(),
      };
      s.detail.items.push(item);
      return item;
    }
    case "update_session_item": {
      const item = itemOwner(a.id)?.detail.items.find((i) => i.id === a.id);
      if (!item) throw "not_found";
      if (typeof a.text === "string") item.text = a.text;
      if (typeof a.done === "boolean") item.done = a.done;
      return null;
    }
    case "delete_session_item": {
      const s = itemOwner(a.id);
      if (s) s.detail.items = s.detail.items.filter((i) => i.id !== a.id);
      return null;
    }
    case "mark_reviewed": {
      const s = sessionOf(a)!;
      s.view.needsReview = false;
      s.detail.context.reviewedUtc = new Date().toISOString();
      return null;
    }
    case "move_session_item":
    case "link_sessions":
    case "unlink_sessions":
    case "clear_captured_context":
    case "clear_all_captured_context":
    case "delete_session":
      return null;
    case "export_session":
      return "cancelled";
    // Import sessions (decision 34): made-up logs; ?importfail shows the error state.
    case "import_scan":
      if (params.has("importfail")) throw "internal";
      return {
        sessions: IMPORTABLE.map((f) => ({
          ...f,
          tracked: f.tracked || SESSIONS.some((s) => s.view.providerSessionId === f.sessionId),
        })),
        agents: [
          { provider: "claude-code", state: "read", sessions: 2, incomplete: false },
          { provider: "cursor", state: "read", sessions: 1, incomplete: false },
          { provider: "codex", state: "read", sessions: 1, incomplete: false },
          { provider: "opencode", state: "not_found", sessions: 0, incomplete: false },
        ],
        incomplete: false,
      };
    case "import_cancel":
      return null;
    case "import_sessions": {
      const picks = (a.picks ?? []) as { provider: string; sessionId: string }[];
      let imported = 0;
      let alreadyTracked = 0;
      for (const p of picks) {
        const f = IMPORTABLE.find((x) => x.provider === p.provider && x.sessionId === p.sessionId);
        if (!f) continue;
        if (f.tracked || SESSIONS.some((s) => s.view.providerSessionId === f.sessionId)) {
          alreadyTracked += 1;
          continue;
        }
        SESSIONS.push(importedSession(f));
        imported += 1;
      }
      return { imported, alreadyTracked, notFound: 0 };
    }

    // Preferences
    case "ui_prefs":
      return uiPrefs;
    case "set_ui_prefs":
      uiPrefs = a.prefs as UiPrefs;
      return null;
    case "context_prefs":
      return contextPrefs;
    case "set_context_prefs":
      contextPrefs = a.prefs as ContextPrefs;
      return null;
    case "notification_prefs":
      return notificationPrefs;
    case "set_notification_prefs":
      notificationPrefs = a.prefs as NotificationPrefs;
      return notificationPrefs;

    // Activities
    case "activities_list":
      return CATALOG.map((x) => activityInfo(x.info.id));
    case "activity_info":
      return activityInfo(a.id);
    case "activity_launch":
      return byId(a.id)?.runUrl ?? null;
    case "activity_launch_end":
      return null;
    case "activity_preview":
      return byId(a.id)?.previewUrl ?? null;
    case "activity_content":
      return byId(a.id)?.content ?? null;
    case "activity_snapshot": {
      const s = snapshots.get(String(a.id));
      return s ? { snapshot: s } : null;
    }
    case "activity_save_snapshot":
      snapshots.set(String(a.id), String(a.snapshot));
      return null;
    case "activity_clear_snapshot":
      snapshots.delete(String(a.id));
      return null;
    case "activity_storage_get":
      return storage.get(`${a.id}/${a.key}`) ?? null;
    case "activity_storage_set":
      storage.set(`${a.id}/${a.key}`, String(a.value));
      return null;
    case "activity_storage_remove":
      storage.delete(`${a.id}/${a.key}`);
      return null;
    case "activity_add_play":
      played[String(a.id)] = (played[String(a.id)] ?? 0) + Number(a.seconds ?? 0);
      return null;
    case "activities_manage_list":
      return CATALOG.map((x) => ({
        activityId: x.info.id,
        name: x.info.name,
        kind: x.info.engine ? "content" : "app",
        builtin: true,
        marketplaceId: "lullshift",
        enabled: true,
        commit: null,
        version: x.info.version,
        problem: null,
        sourceNote: null,
      }));

    // Marketplaces
    case "marketplace_list":
      return [
        {
          id: "lullshift",
          url: "Built in",
          builtin: true,
          description: "Activities bundled with Lullshift.",
          owner: "Lullshift contributors",
          lastFetchedCommit: null,
          lastFetchedAt: null,
          privateHost: false,
        },
      ];
    case "marketplace_entries":
      return [];
    case "marketplace_git_backend":
      return { backend: "system" };

    // Integrations
    case "claude_code_status":
    case "codex_status":
    case "cursor_status":
    case "opencode_status":
      return adapters[cmd.replace(/_status$/, "")];
    // Packaged-app checks (decision 33): the demo runs in a browser, never inside a package.
    case "packaged_context":
      return { package: null };
    case "connection_health":
      return { package: null, scan: "skipped", stale: [], failures: [] };
    case "connection_fix_stale":
      throw "unsupported";

    // Companion
    case "companion_profiles":
      return demoProfiles;
    case "companion_add_model":
    case "companion_remove_model":
    case "companion_set_default_model":
      return changeDemoModels(cmd, a);
    case "companion_open_link":
      // The demo never opens anything; the dialog flow is the same as in the app.
      console.info("[demo] would open a link after confirmation");
      return null;
    case "companion_threads":
      return THREADS;
    case "companion_messages":
      return MESSAGES.filter((m) => m.threadId === a.threadId);
    case "companion_prefs":
      return { toolsEnabled: false, lastScope: null };
    case "companion_usage":
      return [
        {
          profileId: PROFILE.id,
          month: new Date().toISOString().slice(0, 7),
          inputTokens: 18240,
          outputTokens: 9310,
          budget: 500_000,
        },
      ];
    case "companion_send":
      return demoSend(a);
    case "companion_cancel":
      demoCancelled.add(Number(a.threadId));
      return null;
    case "companion_tool_decision":
      return null;
    case "companion_preview_context":
      return demoContextPreview(a.scope as DemoScope);
    case "companion_model_catalog": {
      // Made-up list, no network. "cached" answers from memory like the real core.
      // A refresh of the demo list always finds no change, so it moves only the check time.
      if (a.mode === "cached" && !demoCatalog) return null;
      const fromMemory = a.mode !== "refresh" && demoCatalog !== null;
      const unchanged = a.mode === "refresh" && demoCatalog !== null;
      const now = new Date().toISOString();
      if (!demoCatalog) demoCatalog = { loadedAt: now, checkedAt: now };
      else if (unchanged) demoCatalog = { ...demoCatalog, checkedAt: now };
      return {
        ...demoCatalog,
        unchanged,
        fromMemory,
        providers: Object.entries(DEMO_MODELS).map(([id, p]) => ({
          id,
          name: p.name,
          modelCount: p.models.length,
        })),
      } satisfies ModelCatalog;
    }
    case "companion_catalog_models": {
      if (!demoCatalog) throw { kind: "notFound", message: "The model list isn't loaded." };
      const p = DEMO_MODELS[String(a.providerId)];
      if (!p) throw { kind: "notFound", message: "The loaded model list has no such provider." };
      return p.models;
    }

    // Plugins
    case "plugin:notification|is_permission_granted":
      return true;
    case "plugin:autostart|is_enabled":
      return false;
  }
  console.info(`[demo] no pretend answer for "${cmd}"`);
  throw "unavailable";
}

/** Threads with a reply being written, and threads whose Cancel was pressed. */
const demoBusy = new Set<number>();
const demoCancelled = new Set<number>();

const DEMO_REPLY =
  "Happy to help. In short, keep the change small and reversible, check it against one real case before you rely on it, and write down why you chose it so the next person does not have to guess. If it still feels uncertain after that, take a short break and look at it again with fresh eyes. Most of the time the answer is simpler than it first looked.";

/**
 * Pretends to write a reply word by word, slowly (about eight seconds), so the browser demo can
 * show leaving a conversation and coming back. Like the real core it stores your message as
 * pending first, keeps going whatever the screen does, and stores the reply at the end.
 */
async function demoSend(a: Args): Promise<Message> {
  const threadId = Number(a.threadId);
  const requestId = String(a.requestId);
  const channel = a.onEvent as { onmessage?: (e: unknown) => void };
  const emit = (e: Record<string, unknown>) => channel.onmessage?.({ requestId, ...e });
  if (demoBusy.has(threadId)) {
    throw { kind: "busy", message: "This conversation is already waiting for a reply." };
  }
  demoBusy.add(threadId);
  demoCancelled.delete(threadId);
  const nextId = () => Math.max(0, ...MESSAGES.map((m) => m.id)) + 1;
  const user = userMessage(nextId(), threadId, String(a.text ?? ""), "pending");
  MESSAGES.push(user);
  try {
    emit({ type: "started" });
    const words = DEMO_REPLY.split(" ");
    let text = "";
    for (const word of words) {
      await new Promise((resolve) => setTimeout(resolve, 150));
      if (demoCancelled.has(threadId)) {
        user.status = "failed";
        user.error = { kind: "cancelled", message: "You cancelled this reply." };
        throw { kind: "cancelled", message: "You cancelled this reply." };
      }
      text += (text ? " " : "") + word;
      emit({ type: "delta", text: (text === word ? "" : " ") + word });
    }
    user.status = "sent";
    const reply: Message = {
      ...userMessage(nextId(), threadId, text, "sent"),
      role: "assistant",
      host: PROFILE.host,
      profileId: String(a.profileId ?? PROFILE.id),
      model: String(a.model ?? PROFILE.model),
      inputTokens: 120,
      outputTokens: 80,
    };
    MESSAGES.push(reply);
    return reply;
  } finally {
    demoBusy.delete(threadId);
  }
}

function userMessage(
  id: number,
  threadId: number,
  text: string,
  status: Message["status"],
): Message {
  return {
    id,
    threadId,
    role: "user",
    text,
    attachedContext: null,
    scopeKind: "none",
    host: PROFILE.host,
    profileId: PROFILE.id,
    model: PROFILE.model,
    status,
    error: null,
    inputTokens: null,
    outputTokens: null,
    createdAt: new Date().toISOString(),
    toolCalls: [],
    toolNotices: [],
  };
}

/** Providers and models of the demo, changed in memory only. */
let demoProfiles: ProviderProfile[] = [PROFILE, LOCAL_PROFILE];

function changeDemoModels(cmd: string, a: Args): ProviderProfile {
  const p = demoProfiles.find((x) => x.id === a.profileId);
  if (!p) throw { kind: "notFound", message: "That provider no longer exists." };
  const model = String(a.model ?? "");
  let models = [...p.models];
  if (cmd === "companion_add_model") {
    if (!/^[A-Za-z0-9._:/@+-]{1,200}$/.test(model)) {
      throw { kind: "invalidInput", message: "That model name isn't allowed." };
    }
    const label = typeof a.label === "string" && a.label.trim() ? a.label.trim() : null;
    if (models.some((m) => m.model === model)) {
      models = models.map((m) => (m.model === model ? { ...m, label } : m));
    } else {
      models.push({ model, label, isDefault: false });
    }
    if (a.makeDefault === true)
      models = models.map((m) => ({ ...m, isDefault: m.model === model }));
  } else if (cmd === "companion_remove_model") {
    if (models.length <= 1) {
      throw {
        kind: "invalidInput",
        message: "A provider keeps at least one model. Add another one before removing this one.",
      };
    }
    models = models.filter((m) => m.model !== model);
    const first = models[0];
    if (first && !models.some((m) => m.isDefault)) models[0] = { ...first, isDefault: true };
  } else {
    if (!models.some((m) => m.model === model)) {
      throw { kind: "notFound", message: "That item no longer exists." };
    }
    models = models.map((m) => ({ ...m, isDefault: m.model === model }));
  }
  const def = models.find((m) => m.isDefault) ?? models[0];
  const next = { ...p, models, model: def?.model ?? p.model };
  demoProfiles = demoProfiles.map((x) => (x.id === p.id ? next : x));
  return next;
}

export function installDemoBackend(): void {
  mockIPC((cmd, payload) => handle(cmd, (payload ?? {}) as Args), { shouldMockEvents: true });
}
