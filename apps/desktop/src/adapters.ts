// View-side model for agent integrations. The Rust core decides status; this labels it.

export type AdapterStatus =
  | "not_installed"
  | "installed"
  | "needs_update"
  | "partial"
  | "settings_invalid"
  | "hooks_disabled";

/** Agents with an adapter. */
export type AgentId = "claude-code" | "codex" | "cursor" | "opencode";

/** Every supported agent with a hook adapter, in the order Integrations lists them. */
export const SUPPORTED_AGENTS: readonly AgentId[] = ["claude-code", "codex", "cursor"];

/**
 * Agents whose adapter is experimental (ADR-022). Listed after the supported ones. They are not
 * part of the welcome's "everything is connected" check: an experimental agent must never keep
 * the Connect button on, or hide it, because of a tool the user may not use or that may not work
 * for their version.
 */
export const EXPERIMENTAL_AGENTS: readonly AgentId[] = ["opencode"];

/** The Tauri command that reports an agent's adapter status. */
export function statusCommand(agent: AgentId): string {
  return agent === "claude-code" ? "claude_code_status" : `${agent}_status`;
}

/**
 * True only when every supported agent is known to be connected. A status that is missing
 * (null: unreadable or still unknown) counts as not connected, so the welcome keeps offering the
 * button rather than hiding it on a guess.
 */
export function allAgentsConnected(statuses: readonly (AdapterStatus | null)[]): boolean {
  return statuses.length === SUPPORTED_AGENTS.length && statuses.every((s) => s === "installed");
}

export interface AdapterView {
  status: AdapterStatus;
  settingsPath: string | null;
  events: string[];
  /** The adapter is experimental (OpenCode). Missing in older builds. */
  experimental?: boolean;
  /**
   * Experimental adapters, when installed: whether an event has arrived since the file was
   * written. Only `true` counts as connected; false and null are unknown.
   */
  eventSeen?: boolean | null;
}

export interface UninstallView {
  outcome: "restored" | "removed" | "nothing_to_remove";
  view: AdapterView;
  /** Open sessions that stopped being tracked because of the disconnect (S05-T10). */
  sessionsDisconnected: number;
}

const STATUS: Record<AdapterStatus, string> = {
  not_installed: "Not connected. Lullshift can't see Claude Code sessions yet.",
  installed: "Connected. Claude Code reports its sessions to Lullshift.",
  needs_update:
    "Needs repair: the hooks point to an older or moved copy of Lullshift, so events may be lost.",
  partial: "Partly connected: some Lullshift hooks were removed, so some events are missed.",
  settings_invalid:
    "Your Claude Code settings file couldn't be read as JSON. Lullshift won't change it. Fix the file, then check again.",
  hooks_disabled: "Hooks are turned off in Claude Code.",
};

const HOOK_EVENTS: Record<string, string> = {
  SessionStart: "a session starts",
  UserPromptSubmit: "you send a prompt",
  PostToolUse: "it finishes a step",
  PermissionRequest: "it asks for permission",
  Notification: "it's waiting for you",
  Stop: "it finishes a turn",
  StopFailure: "a turn fails",
  SessionEnd: "a session ends",
  Interrupt: "you stop a turn",
  sessionStart: "a session starts",
  beforeSubmitPrompt: "you send a prompt",
  "session.created": "a session starts",
  "session.execution.started": "it starts working",
  "session.tool.called": "it runs a tool",
  "session.text.started": "it replies",
  "session.status": "it is busy",
  "session.inbox.enqueued": "you send a prompt",
  "permission.asked": "it asks for permission",
  "permission.replied": "you answer a permission request",
  turn_ended: "it finishes a turn",
  failed: "a turn fails",
  "session.deleted": "a session is deleted",
  postToolUse: "it finishes a step",
  afterAgentResponse: "it replies",
  postToolUseFailure: "a step fails",
  subagentStop: "a helper agent finishes",
  preCompact: "it tidies its context",
  stop: "it finishes a turn",
  sessionEnd: "a session ends",
};

/** Hook event names in plain words, for the consent dialog. Unknown names pass through. */
export function hookEventText(events: readonly string[]): string[] {
  return events.map((e) => HOOK_EVENTS[e] ?? e);
}

const AGENT_NAME: Record<AgentId, string> = {
  "claude-code": "Claude Code",
  codex: "Codex",
  cursor: "Cursor",
  opencode: "OpenCode",
};

/**
 * The status of an experimental agent as the user sees it: installed is not connected. It stays
 * unknown until a session has actually reported, so a plugin that OpenCode didn't load, or loaded
 * but never ran, never reads as working.
 */
export function experimentalState(
  view: Pick<AdapterView, "status" | "eventSeen">,
): "connected" | "unknown" | AdapterStatus {
  if (view.status !== "installed") return view.status;
  return view.eventSeen === true ? "connected" : "unknown";
}

const OPENCODE_STATUS: Record<AdapterStatus, string> = {
  not_installed: "Not connected. Lullshift can't see OpenCode sessions yet.",
  installed: "Connected. OpenCode 2.x reports its sessions to Lullshift.",
  needs_update:
    "Needs repair: the plugin file points to an older or moved copy of Lullshift, or was edited, so events may be lost.",
  partial: "Partly connected: the plugin file is out of date.",
  settings_invalid:
    "A file with Lullshift's plugin name is already there and Lullshift didn't write it, so it won't change it. Move or rename that file, then check again.",
  hooks_disabled: "OpenCode plugins are turned off.",
};

const OPENCODE_UNKNOWN =
  "Status unknown. The plugin file is in place, but no OpenCode event has reached Lullshift yet. Restart OpenCode (it needs version 2.x) and use a session; this turns to Connected when the first event arrives.";

export function adapterStatusText(
  status: AdapterStatus,
  agent: AgentId = "claude-code",
  eventSeen?: boolean | null,
): string {
  if (agent === "opencode") {
    return status === "installed" && eventSeen !== true
      ? OPENCODE_UNKNOWN
      : OPENCODE_STATUS[status];
  }
  if (agent === "claude-code" && status !== "hooks_disabled") return STATUS[status];
  const n = AGENT_NAME[agent];
  switch (status) {
    case "not_installed":
      return `Not connected. Lullshift can't see ${n} sessions yet.`;
    case "installed":
      return agent === "codex"
        ? "Connected. Once you've trusted the hooks in Codex (/hooks), it reports its sessions to Lullshift."
        : `Connected. ${n} reports its sessions to Lullshift.`;
    case "needs_update":
      return agent === "codex"
        ? "Needs repair: the hooks point to an older or moved copy of Lullshift, so events may be lost. After repairing, review and trust the changed hooks in Codex (/hooks) again."
        : "Needs repair: the hooks point to an older or moved copy of Lullshift, so events may be lost.";
    case "partial":
      return "Partly connected: some Lullshift hooks were removed, so some events are missed.";
    case "settings_invalid":
      return `Your ${n} hooks file couldn't be read as JSON, or has a format Lullshift doesn't know. Lullshift won't change it. Fix the file, then check again.`;
    case "hooks_disabled":
      return `Hooks are turned off in ${n}. Lullshift sees nothing until you turn them back on in your ${n} config.`;
  }
}

/** What Lullshift can and can't see for each agent (docs/ADAPTER_CAPABILITIES.md). */
export const CAPABILITY_SUMMARY: Record<"codex" | "cursor" | "opencode", string[]> = {
  opencode: [
    "Experimental: it works only with OpenCode 2.x. It does nothing on OpenCode 1.x, and an old version may show a plugin error in OpenCode until you disconnect.",
    "Sees: when a session starts, when you send a prompt, when OpenCode works, when a turn ends or fails, and when a session is deleted.",
    "Sees when OpenCode asks to approve an action (a permission request). It doesn't report being idle or asking a question, and it doesn't see a session that is closed without being deleted.",
    "Helper-agent sessions are not shown as sessions of their own.",
  ],
  codex: [
    "Sees: when a session starts, when you send a prompt, when a step finishes, when a turn ends or you stop it, and when a session ends.",
    "Sees only part of when Codex needs you: only when it asks to approve a command. It doesn't report being idle or asking a question.",
    "Session end can also fire after 30 minutes idle, and has no reason.",
  ],
  cursor: [
    "Sees: when a session starts, when you send a prompt, when Cursor works and replies, when a turn ends, and when a session ends.",
    "Cursor doesn't tell Lullshift when it is waiting for you, so Lullshift never shows Cursor as needing you.",
    "The Cursor command-line agent reports only some of these events.",
  ],
};

/** Shown for Codex: new or changed hooks don't run until the user approves them in Codex. */
export const CODEX_TRUST_NOTE =
  "Codex asks you to review and trust new or changed hooks in /hooks before they run. Do this after connecting, and again after a repair.";

const ERRORS: Record<string, string> = {
  settings_invalid:
    "Your Claude Code settings file isn't valid JSON, so Lullshift didn't change it.",
  changed_concurrently:
    "The settings file changed while Lullshift was updating it. Nothing was written; try again.",
  store_unavailable:
    "Lullshift's local data is unavailable, so it can't record how to undo the change. Nothing was written.",
  file_error: "Lullshift couldn't read or write the settings file. Nothing was changed.",
  settings_not_writable:
    "Your Claude Code settings file is a link or read-only, so Lullshift didn't change it.",
  unsupported_path:
    "Lullshift is installed in a folder whose name can't be written into Claude Code settings. Nothing was changed.",
  rollback_failed:
    "Lullshift added the hooks but couldn't record how to undo them, and couldn't take them back out. Use Disconnect, or remove the Lullshift entries from the file by hand.",
};

export function adapterErrorText(code: string, agent: AgentId = "claude-code"): string {
  const text = ERRORS[code] ?? "Something went wrong. Nothing was changed.";
  if (agent === "claude-code") return text;
  const n = AGENT_NAME[agent];
  if (agent === "opencode") {
    return (
      {
        settings_invalid:
          "A file with Lullshift's plugin name is already in OpenCode's plugins folder and Lullshift didn't write it, so it was left alone.",
        settings_not_writable:
          "The OpenCode plugin file is a link or read-only, so Lullshift didn't change it.",
        file_error:
          "Lullshift couldn't read or write the OpenCode plugin file. Nothing was changed.",
        rollback_failed:
          "Lullshift added the plugin but couldn't record how to undo it, and couldn't take it back out. Use Disconnect, or delete lullshift-observer.js from OpenCode's plugins folder by hand.",
        changed_concurrently:
          "The plugin file changed while Lullshift was updating it. Nothing was written; try again.",
      }[code] ?? text.replace("Claude Code settings file", "OpenCode plugin file")
    );
  }
  if (code === "unsupported_path") {
    return "Lullshift can't write a hook command for this folder name: the Lullshift program folder or your user data folder has a character that can't be written safely (a control character such as a line break, or on Windows a letter that an older code page could turn into a curly quote, such as the Cyrillic letter Ve or O-grave). Nothing was changed. Claude Code is not affected.";
  }
  return text
    .replace("Claude Code settings file", `${n} hooks file`)
    .replace("settings file", "hooks file");
}
