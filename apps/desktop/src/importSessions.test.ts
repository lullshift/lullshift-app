import { describe, expect, it } from "vitest";
import {
  agentNotes,
  countText,
  filterFound,
  groupByFolder,
  groupTick,
  keyOf,
  picksFrom,
  resultText,
  rowLabel,
  sortFound,
  toggleAllShown,
  toggleGroup,
  toggleOne,
  type FoundSession,
} from "./importSessions";
import { liveState, sessionStateView, reasonText, type SessionView } from "./sessions";
import { summarize } from "./format";

const NOW = Date.parse("2026-11-07T12:00:00Z");

const found = (more: Partial<FoundSession>): FoundSession => ({
  provider: "claude-code",
  sessionId: "11111111-2222-4333-8444-555555555555",
  project: "shop/api",
  projectPath: "D:\\work\\shop\\api",
  title: "Fix the login bug",
  lastActivityUtc: "2026-11-07T11:00:00Z",
  guess: "waiting",
  tracked: false,
  ...more,
});

const A = found({
  sessionId: "aaaaaaaa-0000-4000-8000-000000000001",
  lastActivityUtc: "2026-11-07T11:58:00Z",
});
const B = found({
  provider: "cursor",
  sessionId: "bbbbbbbb-0000-4000-8000-000000000002",
  project: "d-work-docs",
  projectPath: "d-work-docs",
  title: "Add dark mode",
  lastActivityUtc: "2026-11-07T10:00:00Z",
});
const C = found({
  provider: "codex",
  sessionId: "cccccccc-0000-4000-8000-000000000003",
  title: null,
  project: "infra/tf",
  projectPath: "/srv/infra/tf",
  lastActivityUtc: "2026-11-07T11:30:00Z",
  tracked: true,
});

describe("sort and filter", () => {
  it("orders by last activity, newest first", () => {
    expect(sortFound([B, A, C]).map((s) => s.sessionId)).toEqual([A, C, B].map((s) => s.sessionId));
  });

  it("matches agent name, folder, title and short id, every word", () => {
    const all = [A, B, C];
    expect(filterFound(all, "")).toHaveLength(3);
    expect(filterFound(all, "cursor").map((s) => s.provider)).toEqual(["cursor"]);
    expect(filterFound(all, "Claude").map((s) => s.provider)).toEqual(["claude-code"]);
    expect(filterFound(all, "infra")).toEqual([C]);
    expect(filterFound(all, "work\\shop")).toEqual([A]);
    expect(filterFound(all, "DARK mode")).toEqual([B]);
    expect(filterFound(all, "bbbbbbbb")).toEqual([B]);
    expect(filterFound(all, "dark login")).toEqual([]);
  });
});

describe("selection", () => {
  it("ticks none, one or many, never a tracked session", () => {
    let sel = new Set<string>();
    expect(picksFrom([A, B, C], sel)).toEqual([]);
    sel = toggleOne(sel, A);
    expect(picksFrom([A, B, C], sel)).toEqual([{ provider: A.provider, sessionId: A.sessionId }]);
    sel = toggleOne(sel, C);
    expect(sel.has(keyOf(C))).toBe(false);
    sel = toggleOne(sel, B);
    expect(picksFrom([A, B, C], sel)).toHaveLength(2);
    sel = toggleOne(sel, A);
    expect(picksFrom([A, B, C], sel)).toEqual([{ provider: B.provider, sessionId: B.sessionId }]);
  });

  it("select all shown ticks the visible pickable ones and keeps hidden ticks", () => {
    let sel = toggleOne(new Set(), B);
    sel = toggleAllShown(sel, [A, C]);
    expect([...sel].sort()).toEqual([keyOf(A), keyOf(B)].sort());
    // All shown already ticked: a second press unticks only those.
    sel = toggleAllShown(sel, [A, C]);
    expect([...sel]).toEqual([keyOf(B)]);
    expect(toggleAllShown(new Set(), [C]).size).toBe(0);
  });
});

describe("words", () => {
  it("counts for the polite live line", () => {
    expect(countText(0, 3, 3)).toBe("3 sessions found, 0 selected.");
    expect(countText(1, 1, 3)).toBe("1 of 3 shown, 1 selected.");
    expect(countText(2, 1, 1)).toBe("1 session found, 2 selected.");
  });

  it("names everything a row shows for screen readers", () => {
    const label = rowLabel(C, NOW);
    expect(label).toContain("Codex");
    expect(label).toContain("Untitled session");
    expect(label).toContain("in infra/tf");
    expect(label).toContain("already in Lullshift");
    expect(rowLabel(A, NOW)).toContain("Waiting for you");
  });

  it("says honestly which agents had no logs", () => {
    expect(
      agentNotes([
        { provider: "claude-code", state: "read", sessions: 2, incomplete: false },
        { provider: "opencode", state: "not_found", sessions: 0, incomplete: false },
        { provider: "cursor", state: "unreadable", sessions: 0, incomplete: false },
        { provider: "codex", state: "not_local", sessions: 0, incomplete: false },
        { provider: "claude-code", state: "read", sessions: 9, incomplete: true },
      ]),
    ).toEqual([
      "No OpenCode logs found on this computer.",
      "Cursor logs are there but couldn't be read.",
      "Codex logs are not on a local drive, so Lullshift doesn't read them.",
      "Only the newest Claude Code sessions were read; older ones may be missing.",
    ]);
  });

  it("confirms what the import did", () => {
    expect(resultText({ imported: 2, alreadyTracked: 1, notFound: 1 })).toBe(
      "Imported 2 sessions. 1 was already in Lullshift. 1 is no longer in the agent's logs and was skipped.",
    );
  });
});

describe("imported sessions on the board", () => {
  const view = (more: Partial<SessionView>): SessionView => ({
    provider: "claude-code",
    providerSessionId: "s",
    state: "needs_attention",
    reason: "observed",
    lastKnownState: "needs_attention",
    lastSeenUtc: "2026-11-07T11:59:00Z",
    lastReceivedUtc: "2026-11-07T11:59:00Z",
    needsReview: false,
    userTitle: null,
    agentTitle: null,
    intentLine: null,
    disconnectedUtc: null,
    ...more,
  });

  it("never count as needs you or working, and say they are a guess", () => {
    const imported = view({ importedUtc: "2026-11-07T12:00:00Z" });
    expect(liveState(imported)).toBe("unknown");
    expect(sessionStateView(imported)).toEqual({ label: "May need you", tone: "neutral" });
    expect(reasonText(imported, 600)).toMatch(/guess/);
    expect(summarize([imported]).tone).toBe("calm");
    expect(
      sessionStateView(view({ importedUtc: "x", state: "unknown", lastKnownState: "idle" })).label,
    ).toBe("Waiting for you");
    // Once a hook event arrives the mark is gone and it is a normal session again.
    expect(liveState(view({ importedUtc: null }))).toBe("needs_attention");
  });
});

describe("grouping by project folder", () => {
  const x1 = found({ sessionId: "x1", lastActivityUtc: "2026-11-07T08:00:00Z" });
  const x2 = found({ sessionId: "x2", lastActivityUtc: "2026-11-07T09:00:00Z", tracked: true });
  const y1 = found({
    sessionId: "y1",
    project: "docs/site",
    projectPath: "/home/u/docs/site",
    lastActivityUtc: "2026-11-07T10:00:00Z",
  });
  const none = found({
    sessionId: "n1",
    project: null,
    projectPath: null,
    lastActivityUtc: "2026-11-07T11:00:00Z",
  });

  it("orders groups by their most recent session and sessions newest first", () => {
    const groups = groupByFolder([x1, y1, x2, none]);
    expect(groups.map((g) => g.label)).toEqual(["Unknown folder", "docs/site", "shop/api"]);
    expect(groups[2]!.sessions.map((s) => s.sessionId)).toEqual(["x2", "x1"]);
    expect(groups[2]!.latest).toBe(Date.parse("2026-11-07T09:00:00Z"));
  });

  it("keeps same-named folders at different paths apart", () => {
    const a = found({ sessionId: "p1", projectPath: "/a/shop/api" });
    const b = found({ sessionId: "p2", projectPath: "/b/shop/api" });
    expect(groupByFolder([a, b])).toHaveLength(2);
  });

  it("has a tri-state over the sessions that can be ticked", () => {
    const [g] = groupByFolder([x1, x2, found({ sessionId: "x3" })]);
    expect(groupTick(g!, new Set())).toBe("none");
    expect(groupTick(g!, new Set([keyOf(x1)]))).toBe("some");
    expect(groupTick(g!, new Set([keyOf(x1), keyOf(found({ sessionId: "x3" }))]))).toBe("all");
    const [onlyTracked] = groupByFolder([x2]);
    expect(groupTick(onlyTracked!, new Set())).toBe("none");
  });

  it("toggles a whole group, skipping tracked sessions and keeping other ticks", () => {
    const [g] = groupByFolder([x1, x2]);
    const other = keyOf(y1);
    const on = toggleGroup(new Set([other]), g!);
    expect([...on].sort()).toEqual([keyOf(x1), other].sort());
    const off = toggleGroup(on, g!);
    expect([...off]).toEqual([other]);
  });
});
