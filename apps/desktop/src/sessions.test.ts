import { describe, expect, it } from "vitest";
import { adapterErrorText, adapterStatusText } from "./adapters";
import { createDashboardStore } from "./dashboardStore";
import {
  ENDED_PROCESS_GONE_LABEL,
  NOT_TRACKED_LABEL,
  healthProblems,
  isProcessGone,
  isTracked,
  liveState,
  reasonText,
  sessionStateView,
  sortSessions,
  stateView,
  type Dashboard,
  type SessionView,
} from "./sessions";

const s = (
  id: string,
  state: SessionView["state"],
  at: string,
  extra: Partial<SessionView> = {},
): SessionView => ({
  provider: "claude-code",
  providerSessionId: id,
  state,
  reason: "observed",
  lastKnownState: state,
  lastSeenUtc: at,
  lastReceivedUtc: at,
  needsReview: false,
  userTitle: null,
  agentTitle: null,
  intentLine: null,
  disconnectedUtc: null,
  ...extra,
});

const dash = (over: Partial<Dashboard> = {}): Dashboard => ({
  sessions: [],
  store: { status: "ok" },
  ipc: { status: "ok" },
  staleAfterSeconds: 600,
  generatedUtc: "2026-11-05T10:00:00Z",
  ...over,
});

describe("stateView", () => {
  it("labels attention distinctly from completion", () => {
    expect(stateView("needs_attention")).toEqual({ label: "Needs you", tone: "attention" });
    expect(stateView("completed").tone).toBe("success");
  });

  it("keeps unknown honest", () => {
    expect(stateView("unknown").label).toBe("Unknown");
  });
});

describe("reasonText", () => {
  it("is silent for observed states", () => {
    expect(reasonText(s("a", "working", "2026-11-05T10:00:00Z"), 600)).toBeNull();
  });

  it("explains unknown states with the last known state", () => {
    const restart = s("a", "unknown", "2026-11-05T10:00:00Z", {
      reason: "unconfirmed_after_restart",
      lastKnownState: "working",
    });
    expect(reasonText(restart, 600)).toBe(
      "Not confirmed since Lullshift started. Last known: Working.",
    );
    const stale = {
      ...restart,
      reason: "stale" as const,
      lastKnownState: "needs_attention" as const,
    };
    expect(reasonText(stale, 600)).toBe("No update for over 10 minutes. Last known: Needs you.");
    expect(reasonText({ ...restart, reason: "not_reported" }, 600)).toMatch(/hasn't reported/);
  });
});

describe("disconnected sessions", () => {
  const gone = s("a", "working", "2026-11-05T10:00:00Z", {
    disconnectedUtc: "2026-11-05T10:05:00Z",
  });

  it("are not tracked and show the honest label, keeping the raw last state", () => {
    expect(isTracked(gone)).toBe(false);
    expect(liveState(gone)).toBe("unknown");
    expect(sessionStateView(gone)).toEqual({ label: NOT_TRACKED_LABEL, tone: "neutral" });
    expect(NOT_TRACKED_LABEL).toBe("Not tracked: disconnected");
    expect(gone.state).toBe("working");
    expect(gone.lastKnownState).toBe("working");
    // Tracked sessions are unchanged.
    const live = s("b", "needs_attention", "2026-11-05T10:00:00Z");
    expect(isTracked(live)).toBe(true);
    expect(sessionStateView(live)).toEqual(stateView("needs_attention"));
  });

  it("explain themselves with the last reported state", () => {
    expect(reasonText(gone, 600)).toBe(
      "The tool was disconnected, so this session is no longer tracked. Last known: Working.",
    );
  });

  it("sort below live sessions", () => {
    const sorted = sortSessions([
      s("gone", "needs_attention", "2026-11-05T11:00:00Z", {
        disconnectedUtc: "2026-11-05T11:01:00Z",
      }),
      s("idle", "idle", "2026-11-05T09:00:00Z"),
      s("busy", "working", "2026-11-05T08:00:00Z"),
    ]);
    expect(sorted.map((x) => x.providerSessionId)).toEqual(["busy", "gone", "idle"]);
  });
});

describe("sortSessions", () => {
  it("puts sessions needing attention first, then working, newest first", () => {
    const sorted = sortSessions([
      s("done", "completed", "2026-11-05T10:00:00Z"),
      s("busy-old", "working", "2026-11-05T09:00:00Z"),
      s("ask", "needs_attention", "2026-11-05T08:00:00Z"),
      s("busy-new", "working", "2026-11-05T10:30:00.5Z"),
      s("stale", "unknown", "2026-11-05T07:00:00Z"),
    ]);
    expect(sorted.map((x) => x.providerSessionId)).toEqual([
      "ask",
      "busy-new",
      "busy-old",
      "done",
      "stale",
    ]);
  });

  it("compares instants, not text", () => {
    const sorted = sortSessions([
      s("whole", "idle", "2026-11-05T10:00:00Z"),
      s("fraction", "idle", "2026-11-05T10:00:00.500Z"),
    ]);
    expect(sorted[0]?.providerSessionId).toBe("fraction");
  });

  it("does not mutate its input", () => {
    const input = [
      s("a", "completed", "2026-11-05T10:00:00Z"),
      s("b", "needs_attention", "2026-11-05T10:00:00Z"),
    ];
    sortSessions(input);
    expect(input[0]?.providerSessionId).toBe("a");
  });
});

describe("healthProblems", () => {
  it("is empty when healthy", () => {
    expect(healthProblems(dash())).toEqual([]);
  });

  it("says agent status is unknown when IPC is down", () => {
    const p = healthProblems(dash({ ipc: { status: "error", code: "address_in_use" } }));
    expect(p).toHaveLength(1);
    expect(p[0]).toMatch(/Agent status is unknown/);
  });

  it("falls back to a generic message for unknown codes", () => {
    expect(healthProblems(dash({ store: { status: "error", code: "whatever" } }))).toEqual([
      "Local data is unavailable.",
    ]);
  });
});

describe("dashboard store", () => {
  it("moves from loading to ready", async () => {
    const store = createDashboardStore(() => Promise.resolve(dash()));
    expect(store.getState().state.status).toBe("loading");
    await store.getState().refresh();
    expect(store.getState().state.status).toBe("ready");
  });

  it("keeps the last snapshot when the core becomes unreachable", async () => {
    let fail = false;
    const store = createDashboardStore(() =>
      fail ? Promise.reject(new Error("down")) : Promise.resolve(dash()),
    );
    await store.getState().refresh();
    fail = true;
    await store.getState().refresh();
    const st = store.getState().state;
    expect(st.status).toBe("error");
    expect(st.status === "error" && st.last).not.toBeNull();
  });

  it("ignores out-of-order responses", async () => {
    const resolvers: ((d: Dashboard) => void)[] = [];
    const store = createDashboardStore(() => new Promise<Dashboard>((r) => resolvers.push(r)));
    const first = store.getState().refresh();
    const second = store.getState().refresh();
    resolvers[1]?.(dash({ staleAfterSeconds: 2 }));
    await second;
    resolvers[0]?.(dash({ staleAfterSeconds: 1 }));
    await first;
    const st = store.getState().state;
    expect(st.status === "ready" && st.dashboard.staleAfterSeconds).toBe(2);
  });
});

describe("adapter labels", () => {
  it("is honest about partial and broken setups", () => {
    expect(adapterStatusText("partial")).toMatch(/missed/);
    expect(adapterStatusText("settings_invalid")).toMatch(/won't change it/);
  });

  it("says nothing was changed on errors, including unknown codes", () => {
    for (const code of [
      "settings_invalid",
      "changed_concurrently",
      "store_unavailable",
      "file_error",
      "???",
    ]) {
      expect(adapterErrorText(code)).toMatch(/didn't change|Nothing was (written|changed)/);
    }
  });

  it("labels ended sessions without claiming completion", () => {
    expect(stateView("ended")).toEqual({ label: "Ended", tone: "neutral" });
  });
});

describe("sessions Lullshift ended because the process is gone (decision 35)", () => {
  const gone = s("g", "ended", "2026-11-05T10:00:00Z", { endedReason: "process_gone" });

  it("says so in the pill and the reason, never as a plain Ended", () => {
    expect(isProcessGone(gone)).toBe(true);
    expect(sessionStateView(gone)).toEqual({ label: ENDED_PROCESS_GONE_LABEL, tone: "neutral" });
    expect(ENDED_PROCESS_GONE_LABEL).toBe("Ended (process gone)");
    const why = reasonText(gone, 600) ?? "";
    expect(why).toMatch(/process is no longer running/);
    expect(why).toMatch(/didn't say so itself/);
    expect(why).toMatch(/reopens/);
  });

  it("leaves an Ended the agent reported alone, and ignores the reason on other states", () => {
    const reported = s("r", "ended", "2026-11-05T10:00:00Z");
    expect(isProcessGone(reported)).toBe(false);
    expect(sessionStateView(reported)).toEqual({ label: "Ended", tone: "neutral" });
    expect(reasonText(reported, 600)).toBeNull();
    // A reopened session no longer carries the reason; a stale one would not claim it.
    const reopened = s("o", "working", "2026-11-05T10:00:00Z", { endedReason: "process_gone" });
    expect(isProcessGone(reopened)).toBe(false);
    expect(sessionStateView(reopened).label).toBe("Working");
  });

  it("is not treated as live, so it never counts as Working or Needs you", () => {
    expect(liveState(gone)).toBe("ended");
  });
});
