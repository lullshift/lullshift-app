// View-side model for the dashboard. The Rust core is authoritative (ADR-008): it derives each
// session's displayed state, including unknown/stale. This file only labels what it returns.

export type SessionState =
  | "unknown"
  | "idle"
  | "working"
  | "needs_attention"
  | "completed"
  | "failed"
  | "cancelled"
  | "ended";

export type ViewReason = "observed" | "unconfirmed_after_restart" | "stale" | "not_reported";

export interface SessionView {
  provider: string;
  providerSessionId: string;
  state: SessionState;
  reason: ViewReason;
  lastKnownState: SessionState;
  lastSeenUtc: string;
  lastReceivedUtc: string;
  /** True when the session ended and the user hasn't reviewed it yet (return card, ADR-013). */
  needsReview: boolean;
}

export type Health = { status: "ok" } | { status: "error"; code: string };

export interface Dashboard {
  sessions: SessionView[];
  store: Health;
  ipc: Health;
  staleAfterSeconds: number;
  generatedUtc: string;
}

export interface StateView {
  label: string;
  tone: "neutral" | "active" | "attention" | "success" | "danger";
}

const VIEWS: Record<SessionState, StateView> = {
  unknown: { label: "Unknown", tone: "neutral" },
  idle: { label: "Idle", tone: "neutral" },
  working: { label: "Working", tone: "active" },
  needs_attention: { label: "Needs you", tone: "attention" },
  completed: { label: "Completed", tone: "success" },
  failed: { label: "Failed", tone: "danger" },
  cancelled: { label: "Cancelled", tone: "neutral" },
  ended: { label: "Ended", tone: "neutral" },
};

export function stateView(state: SessionState): StateView {
  return VIEWS[state];
}

/** Why a session shows what it shows, or null when the state is simply observed. */
export function reasonText(s: SessionView, staleAfterSeconds: number): string | null {
  const lastKnown = `Last known: ${VIEWS[s.lastKnownState].label}.`;
  switch (s.reason) {
    case "observed":
      return null;
    case "unconfirmed_after_restart":
      return `Not confirmed since Lullshift started. ${lastKnown}`;
    case "stale":
      return `No update for over ${Math.round(staleAfterSeconds / 60)} minutes. ${lastKnown}`;
    case "not_reported":
      return "The agent is connected but hasn't reported its state yet.";
  }
}

/** Sessions needing attention first, then working, then the rest; most recent first within a group. */
export function sortSessions(sessions: readonly SessionView[]): SessionView[] {
  const rank = (s: SessionState) => (s === "needs_attention" ? 0 : s === "working" ? 1 : 2);
  return [...sessions].sort(
    (a, b) =>
      rank(a.state) - rank(b.state) ||
      Date.parse(b.lastReceivedUtc) - Date.parse(a.lastReceivedUtc),
  );
}

const STORE_ERRORS: Record<string, string> = {
  too_new:
    "Your local data was saved by a newer version of Lullshift. It hasn't been changed. Update Lullshift to see your sessions.",
  migration_failed:
    "Lullshift couldn't upgrade your local data. It was left at its previous version and a backup was kept.",
};

const IPC_ERRORS: Record<string, string> = {
  address_in_use:
    "Another program is using Lullshift's agent connection, so agent updates can't be received.",
  store_unavailable: "Agent updates are paused because local data is unavailable.",
  stopped: "Lullshift stopped receiving agent updates. Restart Lullshift to reconnect.",
};

/** User-facing problems, most important first. Empty when everything is healthy. */
export function healthProblems(d: Pick<Dashboard, "store" | "ipc">): string[] {
  const out: string[] = [];
  if (d.store.status === "error") {
    out.push(STORE_ERRORS[d.store.code] ?? "Local data is unavailable.");
  }
  if (d.ipc.status === "error") {
    out.push(
      `${IPC_ERRORS[d.ipc.code] ?? "The agent connection couldn't start."} Agent status is unknown.`,
    );
  }
  return out;
}
