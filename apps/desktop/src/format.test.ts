import { describe, expect, it } from "vitest";
import {
  disconnectNotice,
  ellipsize,
  filterSessions,
  groupSessions,
  providerIcon,
  providerName,
  relativeTime,
  sessionName,
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
    needsReview: false,
    userTitle: null,
    agentTitle: null,
    intentLine: null,
    disconnectedUtc: null,
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
    expect(toolChoices([])).toEqual(["claude-code", "codex", "cursor", "opencode"]);
    expect(toolChoices(all)).toEqual(["claude-code", "codex", "cursor", "opencode", "zed"]);
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

describe("sessionName", () => {
  const named = (over: Partial<SessionView>) => ({
    ...session("working", "0b1c2d3e-1111"),
    ...over,
  });

  it("prefers the user's title, then the agent's, then the intent, then the tool", () => {
    const both = named({ userTitle: "Mine", agentTitle: "Agent", intentLine: "Intent" });
    expect(sessionName(both)).toEqual({
      primary: "Mine",
      secondary: "Claude Code · 0b1c2d3e",
      source: "user",
    });
    expect(sessionName(named({ agentTitle: "Agent", intentLine: "Intent" })).primary).toBe("Agent");
    expect(sessionName(named({ intentLine: "Fix the login bug" }))).toEqual({
      primary: "Fix the login bug",
      secondary: "Claude Code · 0b1c2d3e",
      source: "intent",
    });
    expect(sessionName(named({}))).toEqual({
      primary: "Claude Code",
      secondary: null,
      source: "tool",
    });
  });

  it("skips empty and blank titles and flattens whitespace", () => {
    expect(sessionName(named({ userTitle: "   ", agentTitle: "A  \n b" })).primary).toBe("A b");
    expect(sessionName(named({ userTitle: "", agentTitle: "", intentLine: " " })).source).toBe(
      "tool",
    );
  });

  it("cuts an intent title to 80 characters with an ellipsis, but not a real title", () => {
    const long = "word ".repeat(40).trim();
    const intent = sessionName(named({ intentLine: long })).primary;
    expect([...intent].length).toBeLessThanOrEqual(80);
    expect(intent.endsWith("…")).toBe(true);
    expect(sessionName(named({ agentTitle: long })).primary).toBe(long);
    expect(ellipsize("short", 80)).toBe("short");
    expect(ellipsize("abcdef", 4)).toBe("abc…");
  });

  it("is searchable by title", () => {
    const list = [named({ agentTitle: "Billing refactor", providerSessionId: "x1" }), named({})];
    expect(filterSessions(list, { query: "billing", tool: "all" })).toHaveLength(1);
  });

  it("sorts named sessions by the name shown", () => {
    const list = [
      named({ providerSessionId: "z", userTitle: "Alpha" }),
      named({ providerSessionId: "a", userTitle: "Zulu" }),
    ];
    const sorted = sortHistory(list, { key: "session", descending: false });
    expect(sorted.map((s) => s.userTitle)).toEqual(["Alpha", "Zulu"]);
  });
});

describe("disconnected sessions in the dashboard", () => {
  const gone = (state: SessionState, id: string): SessionView => ({
    ...session(state, id),
    disconnectedUtc: "2026-11-05T11:59:00Z",
  });

  it("leave Needs you and Working and go to History", () => {
    const g = groupSessions([
      gone("needs_attention", "a"),
      gone("working", "b"),
      session("working", "c"),
    ]);
    expect(g.needsYou).toEqual([]);
    expect(g.working.map((s) => s.providerSessionId)).toEqual(["c"]);
    expect(g.recent.map((s) => s.providerSessionId)).toEqual(["a", "b"]);
    // The raw last state is still there for the History label to build on.
    expect(g.recent[0]?.lastKnownState).toBe("needs_attention");
  });

  it("never count toward the summary", () => {
    expect(summarize([gone("needs_attention", "a"), gone("working", "b")]).title).toBe("All quiet");
    const s = summarize([gone("working", "a"), session("working", "b")]);
    expect(s.title).toBe("1 agent working");
    // A silent, untracked session is not reported as "hasn't reported recently".
    expect(summarize([{ ...gone("working", "x"), reason: "stale" as const }]).detail).not.toMatch(
      /reported recently/,
    );
  });
});

describe("disconnectNotice", () => {
  it("is calm, singular or plural, and empty for none", () => {
    expect(disconnectNotice("claude-code", 2)).toBe(
      "Claude Code disconnected. 2 sessions are no longer tracked.",
    );
    expect(disconnectNotice("claude-code", 1)).toBe(
      "Claude Code disconnected. 1 session is no longer tracked.",
    );
    expect(disconnectNotice("claude-code", 0)).toBeNull();
    expect(disconnectNotice("cursor", 3)).toBe(
      "Cursor disconnected. 3 sessions are no longer tracked.",
    );
  });
});
