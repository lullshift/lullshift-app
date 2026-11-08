import { describe, expect, it } from "vitest";
import type { NotesOverview, NotesSession, SessionItem } from "../context/api";
import type { SessionView } from "../sessions";
import { buildGroups, filterGroups, resultsLine, stateFilterOf } from "./notesLogic";

const item = (id: number, text: string, done = false): SessionItem => ({
  id,
  kind: "checklist",
  category: null,
  text,
  done,
  position: id,
  createdUtc: "2026-11-05T12:00:00Z",
});

const entry = (id: string, more: Partial<NotesSession> = {}): NotesSession => ({
  provider: "claude-code",
  sessionId: id,
  lastReceivedUtc: "2026-11-05T12:00:00Z",
  userTitle: null,
  agentTitle: null,
  intent: null,
  notes: "",
  checklist: [],
  ...more,
});

const view = (
  id: string,
  state: SessionView["state"],
  more: Partial<SessionView> = {},
): SessionView => ({
  provider: "claude-code",
  providerSessionId: id,
  state,
  reason: "observed",
  lastKnownState: state,
  lastSeenUtc: "2026-11-05T12:00:00Z",
  lastReceivedUtc: "2026-11-05T12:00:00Z",
  needsReview: false,
  userTitle: null,
  agentTitle: null,
  intentLine: null,
  disconnectedUtc: null,
  ...more,
});

const overview: NotesOverview = {
  truncated: false,
  sessions: [
    entry("a", {
      userTitle: "Token refresh",
      intent: "Refresh tokens quietly",
      notes: "Check the two-tab case",
      checklist: [item(1, "Refresh path has a test", true), item(2, "Sign-out clears the profile")],
    }),
    entry("b", { notes: "Old JSON files stay as backup" }),
    entry("c", { checklist: [item(3, "Run the suite 20 times")] }),
  ],
};
const sessions = [
  view("a", "needs_attention"),
  view("b", "ended"),
  view("c", "working", { provider: "codex" }),
];
// "c" is a Claude Code note but the live session is Codex's: it is not in the list.
const groups = buildGroups(overview, sessions);
const ids = (v: ReturnType<typeof filterGroups>) => v.map((x) => x.group.key.sessionId);
const all = { text: "", kind: "all", state: "all" } as const;

describe("grouping", () => {
  it("keeps the overview's order and joins live sessions", () => {
    expect(groups.map((g) => g.key.sessionId)).toEqual(["a", "b", "c"]);
    expect(groups[0]!.title).toBe("Token refresh");
    expect(groups[0]!.state.label).toBe("Needs you");
    expect(groups[1]!.state.label).toBe("Ended");
  });

  it("says unknown, not a guess, for a session the dashboard no longer lists", () => {
    expect(groups[2]!.session).toBeNull();
    expect(groups[2]!.state).toEqual({ label: "Unknown", tone: "neutral" });
    expect(stateFilterOf(null)).toBe("unknown");
  });
});

describe("kind chips", () => {
  it("All shows everything", () => {
    expect(ids(filterGroups(groups, all))).toEqual(["a", "b", "c"]);
  });
  it("Open items shows only unfinished items", () => {
    const v = filterGroups(groups, { ...all, kind: "open" });
    expect(ids(v)).toEqual(["a", "c"]);
    expect(v[0]!.itemIds).toEqual([2]);
    expect(v[0]!.showNotes || v[0]!.showIntent).toBe(false);
  });
  it("Done items shows only finished items", () => {
    const v = filterGroups(groups, { ...all, kind: "done" });
    expect(ids(v)).toEqual(["a"]);
    expect(v[0]!.itemIds).toEqual([1]);
  });
  it("Notes and Intents show just that text", () => {
    expect(ids(filterGroups(groups, { ...all, kind: "notes" }))).toEqual(["a", "b"]);
    const intents = filterGroups(groups, { ...all, kind: "intents" });
    expect(ids(intents)).toEqual(["a"]);
    expect(intents[0]!.showIntent && !intents[0]!.showNotes).toBe(true);
  });
});

describe("search", () => {
  it("is case-insensitive across intent, items, notes and session names", () => {
    expect(ids(filterGroups(groups, { ...all, text: "QUIETLY" }))).toEqual(["a"]);
    expect(ids(filterGroups(groups, { ...all, text: "20 times" }))).toEqual(["c"]);
    expect(ids(filterGroups(groups, { ...all, text: "backup" }))).toEqual(["b"]);
    expect(ids(filterGroups(groups, { ...all, text: "token refresh" }))).toEqual(["a"]);
  });

  it("matches the tool name and keeps the whole session visible", () => {
    const v = filterGroups(groups, { ...all, text: "claude code" });
    expect(ids(v)).toEqual(["a", "b", "c"]);
    expect(v[0]!.itemIds).toEqual([1, 2]);
  });

  it("a text match shows only the text that matched", () => {
    const v = filterGroups(groups, { ...all, text: "two-tab" });
    expect(v).toHaveLength(1);
    expect(v[0]!.showNotes).toBe(true);
    expect(v[0]!.showIntent).toBe(false);
    expect(v[0]!.itemIds).toEqual([]);
  });

  it("finds nothing for words nobody wrote", () => {
    expect(filterGroups(groups, { ...all, text: "zebra" })).toEqual([]);
  });

  it("keeps the box being typed in visible after an edit stops matching", () => {
    const v = filterGroups(
      groups,
      { ...all, text: "zebra" },
      { key: "claude-code:b", field: "notes" },
    );
    expect(ids(v)).toEqual(["b"]);
  });
});

describe("state filter", () => {
  it("narrows by the session's state", () => {
    expect(ids(filterGroups(groups, { ...all, state: "needs" }))).toEqual(["a"]);
    expect(ids(filterGroups(groups, { ...all, state: "ended" }))).toEqual(["b"]);
    expect(ids(filterGroups(groups, { ...all, state: "working" }))).toEqual([]);
  });
  it("an unlisted session only shows under Any state", () => {
    expect(ids(filterGroups(groups, all))).toContain("c");
    for (const state of ["needs", "working", "idle", "ended"] as const)
      expect(ids(filterGroups(groups, { ...all, state }))).not.toContain("c");
  });
});

describe("results line", () => {
  it("counts sessions, with and without filters", () => {
    expect(resultsLine(3, 3, false)).toBe("3 sessions with notes");
    expect(resultsLine(1, 1, false)).toBe("1 session with notes");
    expect(resultsLine(1, 3, true)).toBe("1 of 3 sessions");
    expect(resultsLine(0, 0, false)).toBe("");
  });
});
