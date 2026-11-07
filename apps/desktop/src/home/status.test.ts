import { describe, expect, it } from "vitest";
import type { SessionState, SessionView } from "../sessions";
import { homeStatus, liveSessions, type StatusInput } from "./status";

const session = (
  state: SessionState,
  extra: Partial<SessionView> = {},
  provider = "claude-code",
): SessionView => ({
  provider,
  providerSessionId: `${provider}-${state}`,
  state,
  reason: "observed",
  lastKnownState: state,
  lastSeenUtc: "2026-11-07T10:00:00Z",
  lastReceivedUtc: "2026-11-07T10:00:00Z",
  needsReview: false,
  userTitle: null,
  agentTitle: null,
  intentLine: null,
  disconnectedUtc: null,
  ...extra,
});

const base: StatusInput = {
  loading: false,
  coreError: false,
  hasDashboard: true,
  problems: [],
  sessions: [],
};

describe("homeStatus", () => {
  it("says it is checking while loading, never claiming anything", () => {
    const s = homeStatus({ ...base, loading: true, hasDashboard: false });
    expect(s.title).toBe("Checking your agents");
    expect(s.tone).toBe("unknown");
  });

  it("says status is unknown when the data can't be read and there is no last good copy", () => {
    const s = homeStatus({ ...base, coreError: true, hasDashboard: false });
    expect(s.title).toBe("Agent status is unknown");
    expect(s.line).toContain("Try again");
  });

  it("puts a session that needs you first, naming the tool", () => {
    const s = homeStatus({
      ...base,
      sessions: [session("needs_attention"), session("working", {}, "codex")],
    });
    expect(s).toMatchObject({ tone: "attention", face: "awake", title: "Claude Code needs you" });
    expect(s.line).toBe("1 more is still working.");
    const many = homeStatus({
      ...base,
      sessions: [session("needs_attention"), session("needs_attention", {}, "codex")],
    });
    expect(many.title).toBe("2 sessions need you");
    expect(many.line).toBe("Everything else is quiet.");
  });

  it("calls working agents working and never says a task is done", () => {
    const s = homeStatus({
      ...base,
      sessions: [session("working"), session("working", {}, "codex")],
    });
    expect(s.title).toBe("Your agents are working");
    expect(homeStatus({ ...base, sessions: [session("working")] }).title).toBe(
      "Your agent is working",
    );
    const quiet = homeStatus({ ...base, sessions: [session("completed")] });
    expect(quiet.title).toBe("All quiet");
    expect(`${quiet.title} ${quiet.line}`.toLowerCase()).not.toMatch(/done|finished|complete/);
  });

  it("says No agents yet when nothing has reported", () => {
    expect(homeStatus(base).title).toBe("No agents yet");
  });

  it("is honest about stale sessions and health problems", () => {
    const stale = homeStatus({
      ...base,
      sessions: [session("working", { reason: "stale" })],
    });
    expect(stale.line).toContain("One session hasn't reported recently");
    const broken = homeStatus({ ...base, problems: ["x"], sessions: [session("idle")] });
    expect(broken.title).toBe("Agent status is unknown");
  });

  it("uses no exclamation marks", () => {
    for (const sessions of [[], [session("working")], [session("needs_attention")]]) {
      const s = homeStatus({ ...base, sessions });
      expect(`${s.title}${s.line}`).not.toContain("!");
    }
  });
});

describe("liveSessions", () => {
  it("keeps tracked, unfinished sessions and drops finished, untracked and imported ones", () => {
    const list = [
      session("working"),
      session("idle"),
      session("completed"),
      session("ended"),
      session("working", { disconnectedUtc: "2026-11-07T09:00:00Z" }),
      session("working", { importedUtc: "2026-11-07T09:00:00Z" }),
    ];
    expect(liveSessions(list).map((s) => s.state)).toEqual(["working", "idle"]);
  });
});
