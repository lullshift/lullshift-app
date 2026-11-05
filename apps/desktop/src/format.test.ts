import { describe, expect, it } from "vitest";
import {
  filterSessions,
  groupSessions,
  providerIcon,
  providerName,
  relativeTime,
  shortId,
  sortHistory,
  summarize,
  toolChoices,
} from "./format";
import type { SessionState, SessionView } from "./sessions";

const NOW = Date.parse("2026-11-05T12:00:00Z");
const ago = (seconds: number) => new Date(NOW - seconds * 1000).toISOString();

function session(
  state: SessionState,
  id: string = state,
  reason: SessionView["reason"] = "observed",
) {
  return {
    provider: "claude-code",
    providerSessionId: id,
    state,
    reason,
    lastKnownState: state,
    lastSeenUtc: ago(10),
    lastReceivedUtc: ago(10),
  } satisfies SessionView;
}

describe("names", () => {
  it("names known agents and falls back to the provider ID", () => {
    expect(providerName("claude-code")).toBe("Claude Code");
    expect(providerName("cursor")).toBe("Cursor");
    expect(providerName("other-agent")).toBe("other-agent");
  });

  it("gives each known tool its own glyph and others a generic one", () => {
    expect(providerIcon("claude-code")).toBe("terminal");
    expect(providerIcon("cursor")).toBe("pointer");
    expect(providerIcon("other-agent")).toBe("agent");
  });

  it("shortens long session IDs only", () => {
    expect(shortId("3f2a9c1b-0000-4000-8000-000000000000")).toBe("3f2a9c1b");
    expect(shortId("abc")).toBe("abc");
  });
});

describe("relativeTime", () => {
  it.each([
    [5, "just now"],
    [90, "2 min ago"],
    [59 * 60, "59 min ago"],
    [2 * 3600, "2 hr ago"],
    [26 * 3600, "yesterday"],
    [4 * 86400, "4 days ago"],
    [-30, "just now"],
  ])("%i s ago reads %s", (s, text) => {
    expect(relativeTime(ago(s), NOW)).toBe(text);
  });

  it("is honest about unreadable times", () => {
    expect(relativeTime("nope", NOW)).toBe("unknown time");
  });
});

describe("summarize", () => {
  it("puts the session that needs you first, by agent name", () => {
    const s = summarize([session("working", "w"), session("needs_attention")]);
    expect(s).toEqual({
      tone: "attention",
      title: "Claude Code needs you",
      detail: "1 more is still working.",
    });
  });

  it("counts several sessions that need you", () => {
    expect(
      summarize([session("needs_attention", "a"), session("needs_attention", "b")]).title,
    ).toBe("2 sessions need you");
  });

  it("is calm while agents work", () => {
    const s = summarize([session("working", "a"), session("working", "b"), session("idle")]);
    expect(s.tone).toBe("active");
    expect(s.title).toBe("2 agents working");
  });

  it("says all quiet, and mentions sessions that went silent", () => {
    expect(summarize([]).title).toBe("All quiet");
    const s = summarize([session("unknown", "x", "stale")]);
    expect(s.tone).toBe("calm");
    expect(s.detail).toMatch(/One session hasn't reported recently/);
  });
});

describe("groupSessions", () => {
  it("splits into needs you, working and recent", () => {
    const g = groupSessions([
      session("needs_attention"),
      session("working"),
      session("ended"),
      session("idle"),
    ]);
    expect(g.needsYou).toHaveLength(1);
    expect(g.working).toHaveLength(1);
    expect(g.recent.map((s) => s.state)).toEqual(["ended", "idle"]);
  });
});

describe("filters", () => {
  const cursor = { ...session("working", "A81C40DE"), provider: "cursor" };
  const all = [
    session("working", "7c41e2d9"),
    cursor,
    { ...session("idle", "x1"), provider: "zed" },
  ];

  it("offers every known tool, then other tools that appear", () => {
    expect(toolChoices([])).toEqual(["claude-code", "cursor"]);
    expect(toolChoices(all)).toEqual(["claude-code", "cursor", "zed"]);
  });

  it("filters by tool and by ID or tool name, ignoring case and spaces", () => {
    expect(filterSessions(all, { query: "", tool: "all" })).toHaveLength(3);
    expect(filterSessions(all, { query: "", tool: "cursor" })).toEqual([cursor]);
    expect(filterSessions(all, { query: " a81c ", tool: "all" })).toEqual([cursor]);
    expect(filterSessions(all, { query: "claude", tool: "all" })).toHaveLength(1);
    expect(filterSessions(all, { query: "claude", tool: "cursor" })).toEqual([]);
  });
});

describe("sortHistory", () => {
  const a = { ...session("ended", "bbb"), lastReceivedUtc: ago(60) };
  const b = { ...session("idle", "aaa"), provider: "cursor", lastReceivedUtc: ago(10) };
  const bad = { ...session("ended", "ccc"), lastReceivedUtc: "not a time" };

  it("sorts by last seen, newest first, with unknown times last", () => {
    expect(sortHistory([a, bad, b], { key: "lastSeen", descending: true })).toEqual([b, a, bad]);
  });

  it("sorts by tool, session and state, either way", () => {
    expect(sortHistory([b, a], { key: "tool", descending: false })).toEqual([a, b]);
    expect(sortHistory([a, b], { key: "session", descending: false })).toEqual([b, a]);
    expect(sortHistory([a, b], { key: "state", descending: true })).toEqual([b, a]);
  });

  it("keeps the incoming order for ties", () => {
    const c = { ...a, providerSessionId: "zzz" };
    expect(sortHistory([c, a], { key: "tool", descending: false })).toEqual([c, a]);
  });
});
