// A pretend Rust core for the browser demo. Answers the app's commands from made-up data in
// memory, so every screen can be shown (and screenshotted) without a backend or real user data.
// Dev only: loaded by demo.html, never by index.html, and not part of any build.

import { mockIPC } from "@tauri-apps/api/mocks";
import type { AdapterView } from "../adapters";
import type { ContextPrefs, SessionItem, UiPrefs } from "../context/api";
import type { NotificationPrefs } from "../NotificationSettings";
import type { Dashboard } from "../sessions";
import { CATALOG, byId } from "./catalog";
import { MESSAGES, PROFILE, SESSIONS, THREADS, ago } from "./data";

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

const sessionOf = (a: Args) =>
  SESSIONS.find((s) => s.view.provider === a.provider && s.view.providerSessionId === a.sessionId);
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

    // Companion
    case "companion_profiles":
      return [PROFILE];
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
    case "companion_preview_context":
      return { scope: a.scope, text: "", chars: 0, sha256: null };

    // Plugins
    case "plugin:notification|is_permission_granted":
      return true;
    case "plugin:autostart|is_enabled":
      return false;
  }
  console.info(`[demo] no pretend answer for "${cmd}"`);
  throw "unavailable";
}

export function installDemoBackend(): void {
  mockIPC((cmd, payload) => handle(cmd, (payload ?? {}) as Args), { shouldMockEvents: true });
}
