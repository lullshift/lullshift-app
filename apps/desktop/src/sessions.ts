// View-side model for the Sprint 00 mock agent feed. The Rust core is authoritative;
// this file only describes the shape it returns and how the UI labels it.

export type SessionState =
  "unknown" | "idle" | "working" | "needs_attention" | "completed" | "failed" | "cancelled";

export interface SessionSnapshot {
  provider: string;
  providerSessionId: string;
  title: string;
  state: SessionState;
  lastEventUtc: string | null;
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
};

export function stateView(state: SessionState): StateView {
  return VIEWS[state];
}

/** Sessions needing attention first, then working, then the rest; newest first within a group. */
export function sortSessions(sessions: readonly SessionSnapshot[]): SessionSnapshot[] {
  const rank = (s: SessionState) => (s === "needs_attention" ? 0 : s === "working" ? 1 : 2);
  return [...sessions].sort(
    (a, b) =>
      rank(a.state) - rank(b.state) || (b.lastEventUtc ?? "").localeCompare(a.lastEventUtc ?? ""),
  );
}
