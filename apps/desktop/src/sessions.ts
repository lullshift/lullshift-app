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
  /** A name the user gave the session (S05-T11); null when none. */
  userTitle: string | null;
  /** The agent's own title, only present if the user opted in to capture. */
  agentTitle: string | null;
  /** First line of the captured intent (at most 200 characters), for the title fallback. */
  intentLine: string | null;
  /**
   * Set when the user disconnected the session's tool and the session sent nothing since
   * (S05-T10). `state` and `lastKnownState` still hold what it last reported; the session is no
   * longer tracked and is never shown as Working or Needs you.
   */
  disconnectedUtc: string | null;
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

export const NOT_TRACKED_LABEL = "Not tracked: disconnected";

/** False once the session's tool was disconnected and it hasn't reported since. */
export function isTracked(s: Pick<SessionView, "disconnectedUtc">): boolean {
  return !s.disconnectedUtc;
}

/**
 * The state that counts for grouping, summaries and the panel: a session that is no longer
 * tracked is "unknown", whatever it last reported.
 */
export function liveState(s: Pick<SessionView, "state" | "disconnectedUtc">): SessionState {
  return isTracked(s) ? s.state : "unknown";
}

/** The pill for a session. Untracked sessions say so instead of showing their last state. */
export function sessionStateView(s: Pick<SessionView, "state" | "disconnectedUtc">): StateView {
  return isTracked(s) ? VIEWS[s.state] : { label: NOT_TRACKED_LABEL, tone: "neutral" };
}

/** Why a session shows what it shows, or null when the state is simply observed. */
export function reasonText(s: SessionView, staleAfterSeconds: number): string | null {
  const lastKnown = `Last known: ${VIEWS[s.lastKnownState].label}.`;
  if (!isTracked(s))
    return `The tool was disconnected, so this session is no longer tracked. ${lastKnown}`;
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
      rank(liveState(a)) - rank(liveState(b)) ||
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
