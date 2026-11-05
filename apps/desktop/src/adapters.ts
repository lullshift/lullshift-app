// View-side model for agent integrations. The Rust core decides status; this labels it.

export type AdapterStatus =
  "not_installed" | "installed" | "needs_update" | "partial" | "settings_invalid";

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
};

/** Hook event names in plain words, for the consent dialog. Unknown names pass through. */
export function hookEventText(events: readonly string[]): string[] {
  return events.map((e) => HOOK_EVENTS[e] ?? e);
}

export function adapterStatusText(status: AdapterStatus): string {
  return STATUS[status];
}

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

export function adapterErrorText(code: string): string {
  return ERRORS[code] ?? "Something went wrong. Nothing was changed.";
}
