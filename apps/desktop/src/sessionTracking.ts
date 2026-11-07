// UI contract for src-tauri/src/settle.rs (decision 35): the session-tracking setting and the
// outcome of the last log catch-up. Change both together.

/** Mirrors `TrackingPrefs`. Everything is off until the user turns it on. */
export interface TrackingPrefs {
  catchUpFromLogs: boolean;
}

/** Mirrors `CatchUpReport`. */
export interface CatchUpStatus {
  outcome: "not_run" | "done" | "busy" | "cancelled" | "store_unavailable";
  finishedUtc: string | null;
  updated: number;
  /** Agent ids whose logs exist but couldn't be read (or aren't on a local drive). */
  unreadable: string[];
  /** A cap or the deadline cut the scan short. */
  incomplete: boolean;
}

export const CATCH_UP_LABEL = "Catch up from agent logs at startup";

const AGENTS: Record<string, string> = {
  "claude-code": "Claude Code",
  cursor: "Cursor",
  codex: "Codex",
  opencode: "OpenCode",
};

function agentName(id: string): string {
  return AGENTS[id] ?? "an agent";
}

function list(names: string[]): string {
  if (names.length <= 1) return names[0] ?? "";
  return `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`;
}

/**
 * One line about what the last startup catch-up did, or null when there is nothing to say
 * (the setting is off). Honest about failures, partial scans and the fact that the changed
 * states are guesses.
 */
export function catchUpSummary(enabled: boolean, status: CatchUpStatus | null): string | null {
  if (!enabled) return null;
  if (!status || status.outcome === "not_run") {
    return "Turned on. It runs the next time Lullshift starts.";
  }
  switch (status.outcome) {
    case "busy":
      return "Skipped at startup because an import was running. It tries again at the next start.";
    case "cancelled":
      return "Stopped before it finished. Nothing was changed.";
    case "store_unavailable":
      return "Couldn't update sessions: local data is unavailable.";
    case "done": {
      const parts: string[] = [
        status.updated === 0
          ? "Last catch-up: nothing to update."
          : status.updated === 1
            ? "Last catch-up: updated 1 session from the logs (shown as a guess until the agent reports)."
            : `Last catch-up: updated ${status.updated} sessions from the logs (shown as guesses until the agents report).`,
      ];
      if (status.unreadable.length > 0) {
        parts.push(
          `Couldn't read the logs of ${list([...new Set(status.unreadable)].map(agentName))}.`,
        );
      }
      if (status.incomplete) parts.push("Older sessions may not have been checked.");
      return parts.join(" ");
    }
  }
}
