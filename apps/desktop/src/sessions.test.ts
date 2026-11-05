import { describe, expect, it } from "vitest";
import { sortSessions, stateView, type SessionSnapshot } from "./sessions";

const s = (id: string, state: SessionSnapshot["state"], at: string | null): SessionSnapshot => ({
  provider: "claude-code",
  providerSessionId: id,
  title: id,
  state,
  lastEventUtc: at,
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

describe("sortSessions", () => {
  it("puts sessions needing attention first, then working, newest first", () => {
    const sorted = sortSessions([
      s("done", "completed", "2026-11-05T10:00:00Z"),
      s("busy-old", "working", "2026-11-05T09:00:00Z"),
      s("ask", "needs_attention", "2026-11-05T08:00:00Z"),
      s("busy-new", "working", "2026-11-05T10:30:00Z"),
      s("never", "unknown", null),
    ]);
    expect(sorted.map((x) => x.providerSessionId)).toEqual([
      "ask",
      "busy-new",
      "busy-old",
      "done",
      "never",
    ]);
  });

  it("does not mutate its input", () => {
    const input = [s("a", "completed", null), s("b", "needs_attention", null)];
    sortSessions(input);
    expect(input[0]?.providerSessionId).toBe("a");
  });
});
