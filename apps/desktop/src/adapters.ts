// View-side model for agent integrations. The Rust core decides status; this labels it.

export type AdapterStatus =
  | "not_installed"
  | "installed"
  | "needs_update"
  | "partial"
  | "settings_invalid"
  | "hooks_disabled";

/** Agents with a hook adapter. */
export type AgentId = "claude-code" | "codex" | "cursor";

/** Every agent with a hook adapter, in the order Integrations lists them. */
export const SUPPORTED_AGENTS: readonly AgentId[] = ["claude-code", "codex", "cursor"];

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
};

export function adapterStatusText(status: AdapterStatus, agent: AgentId = "claude-code"): string {
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
export const CAPABILITY_SUMMARY: Record<"codex" | "cursor", string[]> = {
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
  if (code === "unsupported_path") {
    return "Lullshift is installed in a folder whose name contains a line break or another control character, which can't be written safely into a hook command. Nothing was changed.";
  }
  return text
    .replace("Claude Code settings file", `${n} hooks file`)
    .replace("settings file", "hooks file");
}
