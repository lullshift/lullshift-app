import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import type { SessionView } from "../sessions";
import * as api from "./api";
import type { SessionItem } from "./api";
import {
  askFallback,
  checklistText,
  decideLayout,
  groupedList,
  itemsSummary,
  panelMode,
  showsReviewPill,
  timelineLabel,
} from "./panelLogic";

const view = (id: string, state: SessionView["state"], needsReview = false): SessionView => ({
  provider: "claude-code",
  providerSessionId: id,
  state,
  reason: "observed",
  lastKnownState: state,
  lastSeenUtc: "2026-11-05T12:00:00Z",
  lastReceivedUtc: "2026-11-05T12:00:00Z",
  needsReview,
});

const item = (id: number, kind: SessionItem["kind"], text: string, done = false): SessionItem => ({
  id,
  kind,
  category: kind === "next_move" ? "question" : null,
  text,
  done,
  position: id,
  createdUtc: "2026-11-05T12:00:00Z",
});

describe("decideLayout", () => {
  it("shows no panel when nothing is open", () => {
    expect(decideLayout(false, true, 1400)).toEqual({ panel: null, board: "columns" });
  });

  it("keeps two columns beside a pinned panel in a wide window", () => {
    expect(decideLayout(true, true, 1150)).toEqual({ panel: "docked", board: "columns" });
    expect(decideLayout(true, true, 1600)).toEqual({ panel: "docked", board: "columns" });
  });

  it("switches to the grouped list beside a pinned panel in a medium window", () => {
    expect(decideLayout(true, true, 1149)).toEqual({ panel: "docked", board: "grouped-list" });
    expect(decideLayout(true, true, 720)).toEqual({ panel: "docked", board: "grouped-list" });
  });

  it("floats over an unchanged board when the user chose floating", () => {
    for (const width of [800, 1000, 1400]) {
      expect(decideLayout(true, false, width)).toEqual({ panel: "floating", board: "columns" });
    }
  });

  it("always floats under 720 px, whatever the saved preference", () => {
    expect(decideLayout(true, true, 719)).toEqual({ panel: "floating", board: "columns" });
    expect(decideLayout(true, false, 719)).toEqual({ panel: "floating", board: "columns" });
  });
});

describe("grouping and review pill", () => {
  it("pins Needs you and leaves Working to scroll", () => {
    const sorted = [view("a", "needs_attention"), view("b", "working"), view("c", "working")];
    const g = groupedList(sorted);
    expect(g.pinned.map((s) => s.providerSessionId)).toEqual(["a"]);
    expect(g.scrolling.map((s) => s.providerSessionId)).toEqual(["b", "c"]);
  });

  it("leaves finished sessions out of the live list", () => {
    const g = groupedList([view("a", "ended"), view("b", "idle")]);
    expect(g.pinned).toHaveLength(0);
    expect(g.scrolling).toHaveLength(0);
  });

  it("shows the Review pill only when the session needs a review", () => {
    expect(showsReviewPill(view("a", "ended", true))).toBe(true);
    expect(showsReviewPill(view("b", "ended", false))).toBe(false);
  });

  it("picks the panel mode from the state", () => {
    expect(panelMode("needs_attention")).toBe("needs_you");
    expect(panelMode("working")).toBe("working");
    for (const s of ["unknown", "idle", "completed", "failed", "cancelled", "ended"] as const) {
      expect(panelMode(s)).toBe("history");
    }
  });
});

describe("timeline labels", () => {
  it("uses plain words and never claims a turn end means the task is done", () => {
    expect(timelineLabel({ eventType: "turn_ended", reason: null })).toMatch(/not necessarily/);
    expect(timelineLabel({ eventType: "session_ended", reason: null })).toBe("Session ended");
  });

  it("explains why it needed you, and shows unknown reasons honestly", () => {
    expect(timelineLabel({ eventType: "needs_attention", reason: "permission" })).toBe(
      "Needed you (asked for permission)",
    );
    expect(timelineLabel({ eventType: "needs_attention", reason: "some_new_hint" })).toBe(
      "Needed you (some new hint)",
    );
    expect(timelineLabel({ eventType: "needs_attention", reason: null })).toBe("Needed you");
  });

  it("does not hide unknown event types", () => {
    expect(timelineLabel({ eventType: "brand_new", reason: null })).toBe("Event: brand new");
  });

  it("describes the ask from the latest needs_attention entry", () => {
    const t = (reason: string | null) => ({
      timestampUtc: "2026-11-05T12:00:00Z",
      eventType: "needs_attention",
      reason,
    });
    expect(askFallback([t("prompt_input_exit"), t("permission")])).toMatch(/permission/);
    expect(askFallback([])).toMatch(/didn't say why/);
  });
});

describe("clipboard text", () => {
  it("copies the checklist as one line per item with done state", () => {
    const items = [
      item(2, "checklist", "Run the tests", true),
      item(1, "checklist", "Read the diff"),
      item(3, "decision", "Keep the old API?"),
      item(4, "next_move", "Not part of the checklist"),
    ];
    expect(checklistText(items)).toBe("- [ ] Read the diff\n- [x] Run the tests");
  });

  it("summarises items without zero counts", () => {
    expect(itemsSummary([])).toBe("No items yet.");
    expect(
      itemsSummary([
        item(1, "decision", "a"),
        item(2, "next_move", "b"),
        item(3, "next_move", "c"),
      ]),
    ).toBe("1 decision, 2 Next Move items.");
  });
});

describe("nothing is sent to an agent", () => {
  beforeEach(() => invoke.mockReset().mockResolvedValue(null));

  it("exposes no function that talks to an agent", () => {
    const names = Object.keys(api).filter(
      (n) => typeof (api as Record<string, unknown>)[n] === "function",
    );
    expect(names.length).toBeGreaterThan(10);
    for (const n of names)
      expect(n).not.toMatch(/send|inject|paste|answer|reply|respond|deliver|type/i);
  });

  it("calls only local context commands", async () => {
    const key = { provider: "claude-code", sessionId: "s1" };
    await api.sessionDetail(key);
    await api.setSessionNotes(key, "n");
    await api.setSessionIntent(key, "i");
    await api.addSessionItem(key, "next_move", "question", "q");
    await api.updateSessionItem(1, { done: true });
    await api.deleteSessionItem(1);
    await api.moveSessionItem(1, 0);
    await api.linkSessions(key, { provider: "claude-code", sessionId: "s2" });
    await api.unlinkSessions(key, { provider: "claude-code", sessionId: "s2" });
    await api.markReviewed(key);
    await api.clearCapturedContext(key);
    await api.clearAllCapturedContext();
    await api.exportSession(key, "json");
    await api.deleteSession(key);
    await api.contextPrefs();
    await api.setContextPrefs({ retentionDays: 30, capture: {} });
    await api.uiPrefs();
    await api.setUiPrefs({ detailPanelPinned: true, contentWidth: "full" });

    const commands = invoke.mock.calls.map((c) => c[0] as string);
    expect(commands).toHaveLength(18);
    for (const c of commands) expect(c).not.toMatch(/send|inject|agent|paste|answer|reply|type/i);
  });
});
