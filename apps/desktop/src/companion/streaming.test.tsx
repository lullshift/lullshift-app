import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(() => new Promise(() => undefined)),
  Channel: class {},
}));

import type { Scope, StreamEvent, ToolCallRecord, ToolNoticeReason } from "./api";
import { ToolsSetting } from "./CompanionSettings";
import {
  buildScope,
  draftFromScope,
  initialStream,
  reduceStream,
  restoreScope,
  streamStatus,
  TOOLS_SETTING_HELP,
  TOOLS_SETTING_LABEL,
  toolNoticeText,
  type StreamState,
} from "./logic";
import { ToolApprovalCard, ToolNotices, ToolRecords } from "./ToolCards";

const RID = "r-aaaaaaaa";

function run(events: StreamEvent[], from: StreamState = initialStream(RID)): StreamState {
  return events.reduce(reduceStream, from);
}

describe("streaming state", () => {
  it("builds the reply from deltas of its own request only", () => {
    const s = run([
      { type: "started", requestId: RID },
      { type: "delta", requestId: RID, text: "Hel" },
      { type: "delta", requestId: "r-bbbbbbbb", text: "LATE FROM A CANCELLED REQUEST" },
      { type: "delta", requestId: RID, text: "lo" },
    ]);
    expect(s.text).toBe("Hello");
  });

  it("starts a fresh text for the next provider call", () => {
    const s = run([
      { type: "delta", requestId: RID, text: "Let me check." },
      { type: "round", requestId: RID, round: 2 },
      { type: "delta", requestId: RID, text: "Done." },
    ]);
    expect(s.text).toBe("Done.");
    expect(s.round).toBe(2);
  });

  it("holds a lookup until it is resolved, then records the answer", () => {
    let s = run([
      {
        type: "toolRequest",
        requestId: RID,
        approvalId: "call_1",
        tool: "suggest_break",
        label: "Suggest a break",
        arguments: "{}",
        resultText: "Suggestion: Tea Sort",
        host: "api.example.com",
      },
    ]);
    expect(s.pending?.resultText).toBe("Suggestion: Tea Sort");
    // An answer for another call does not clear it.
    s = reduceStream(s, {
      type: "toolResolved",
      requestId: RID,
      approvalId: "other",
      approved: true,
    });
    expect(s.pending).not.toBeNull();
    s = reduceStream(s, {
      type: "toolResolved",
      requestId: RID,
      approvalId: "call_1",
      approved: false,
    });
    expect(s.pending).toBeNull();
    expect(s.answered).toEqual([{ label: "Suggest a break", approved: false }]);
  });

  it("keeps the screen-reader status to one sentence per phase, not per token", () => {
    const waiting = initialStream(RID);
    expect(streamStatus(waiting, "h")).toBe("Waiting for h…");
    const a = run([{ type: "delta", requestId: RID, text: "a" }]);
    const b = run([{ type: "delta", requestId: RID, text: "abc def ghi" }]);
    expect(streamStatus(a, "h")).toBe("Companion is replying…");
    expect(streamStatus(b, "h")).toBe(streamStatus(a, "h"));
    expect(streamStatus(a, "h")).not.toContain("abc");
    expect(streamStatus(null, "h")).toBe("");
  });

  it("notes lookups that were not possible without showing any data", () => {
    const s = run([{ type: "toolSkipped", requestId: RID, tool: "x", text: "fixed" }]);
    expect(s.skipped).toEqual(["fixed"]);
  });

  it("collects notices for lookups that were not run and keeps the safe name only", () => {
    const s = run([
      { type: "toolNotice", requestId: RID, reason: "unknown", tool: "run_shell" },
      { type: "toolNotice", requestId: RID, reason: "malformed", tool: "Run Shell!" },
      { type: "toolNotice", requestId: RID, reason: "limit" },
      { type: "toolNotice", requestId: "r-other-id", reason: "limit" },
    ]);
    expect(s.notices).toEqual([
      { reason: "unknown", tool: "run_shell" },
      { reason: "malformed", tool: null },
      { reason: "limit", tool: null },
    ]);
  });

  it("announces a notice once through the polite status, not per token", () => {
    const withNotice = run([{ type: "toolNotice", requestId: RID, reason: "unknown" }]);
    const text = streamStatus(withNotice, "h");
    expect(text).toContain("asked for something it can't have");
    // Once the reply text starts, the status goes back to the per-phase sentence.
    const replying = run([{ type: "delta", requestId: RID, text: "abc" }], withNotice);
    expect(streamStatus(replying, "h")).toBe("Companion is replying…");
  });
});

describe("lookup notices", () => {
  const cases: [ToolNoticeReason, string | null, string][] = [
    ["unknown", "run_shell", "an unknown lookup (run_shell)"],
    ["unknown", null, "an unknown lookup)"],
    ["malformed", "suggest_break", "a lookup (suggest_break) that was not valid"],
    ["limit", null, "more lookups"],
  ];
  it.each(cases)("renders the %s notice as plain text with no button", (reason, tool, part) => {
    const html = renderToStaticMarkup(<ToolNotices notices={[{ reason, tool }]} />);
    expect(html).toContain("The Companion asked for something it can&#x27;t have");
    expect(html).toContain(part.replace("'", "&#x27;"));
    expect(html).toContain("Nothing was sent.");
    expect(html).not.toContain("<button");
    expect(html).not.toContain("aria-live");
    expect(toolNoticeText(reason, tool)).toContain("Nothing was sent.");
  });

  it("drops an unsafe tool name and renders nothing for no notices", () => {
    expect(toolNoticeText("unknown", "<b>x</b>")).not.toContain("<b>");
    expect(toolNoticeText("unknown", "<b>x</b>")).toContain("an unknown lookup)");
    expect(renderToStaticMarkup(<ToolNotices notices={[]} />)).toBe("");
  });
});

describe("remembered context choice", () => {
  const summary: Scope = {
    kind: "sessionSummary",
    sessions: [{ provider: "claude-code", sessionId: "s1" }],
  };
  const notes: Scope = {
    kind: "selectedNotes",
    sessions: [{ provider: "codex", sessionId: "s2", notes: true, itemIds: [3, 1] }],
  };

  it("starts with the last choice when it is still available", () => {
    const r = restoreScope(summary, true);
    expect(r.draft.kind).toBe("sessionSummary");
    expect(r.note).toBeNull();
    expect(buildScope(r.draft)).toEqual(summary);
    expect(buildScope(restoreScope(notes, true).draft)).toEqual({
      kind: "selectedNotes",
      sessions: [{ provider: "codex", sessionId: "s2", notes: true, itemIds: [1, 3] }],
    });
  });

  it("falls back to No context and says so when it is gone", () => {
    const r = restoreScope(summary, false);
    expect(r.draft.kind).toBe("none");
    expect(r.note).toContain("isn't available any more");
    expect(r.note).toContain("No context");
  });

  it("starts with No context when nothing was remembered or the last was none", () => {
    expect(restoreScope(null, true)).toEqual({ draft: draftFromScope(null), note: null });
    expect(restoreScope({ kind: "none" }, true).draft.kind).toBe("none");
  });

  it("a remembered choice never skips the preview: scopes other than none always need one", async () => {
    const { needsPreview } = await import("./logic");
    expect(needsPreview(buildScope(restoreScope(summary, true).draft), null)).toBe(true);
  });
});

describe("lookup approval rendering", () => {
  const pending = {
    approvalId: "call_1",
    tool: "session_overview",
    label: "Session overview",
    arguments: "{}",
    resultText: "1. claude-code: working for 12 min",
    host: "api.example.com",
  };

  it("shows the exact text and both buttons, with Approve and Decline named", () => {
    const html = renderToStaticMarkup(
      <ToolApprovalCard pending={pending} busy={false} onDecide={() => undefined} />,
    );
    expect(html).toContain("1. claude-code: working for 12 min");
    expect(html).toContain("Approve");
    expect(html).toContain("Decline");
    expect(html).toContain("api.example.com");
    expect(html).toContain("This exact text will be sent");
    expect(html).toContain('aria-label="Exact text that will be sent"');
    expect(html.indexOf("Approve")).toBeLessThan(html.indexOf("Decline"));
  });

  it("disables the buttons while an answer is being sent", () => {
    const html = renderToStaticMarkup(
      <ToolApprovalCard pending={pending} busy onDecide={() => undefined} />,
    );
    expect(html.match(/disabled/g)?.length).toBe(2);
  });

  it("renders text as text, never as markup or links", () => {
    const html = renderToStaticMarkup(
      <ToolApprovalCard
        pending={{ ...pending, resultText: '<a href="https://evil">click</a>' }}
        busy={false}
        onDecide={() => undefined}
      />,
    );
    expect(html).not.toContain("<a href");
    expect(html).toContain("&lt;a href");
  });
});

describe("lookup records", () => {
  const approved: ToolCallRecord = {
    id: 1,
    round: 1,
    tool: "suggest_break",
    label: "Suggest a break",
    arguments: "{}",
    resultText: "Suggestion: Tea Sort",
    status: "approved",
    card: { activityId: "lullshift.tea-sort", name: "Tea Sort", minutes: 5 },
    host: "api.example.com",
    createdAt: "2026-11-06T10:00:00Z",
  };
  const declined: ToolCallRecord = {
    ...approved,
    id: 2,
    status: "declined",
    resultText: "The user declined.",
    card: null,
  };

  it("says what was sent for an approved lookup and offers the card as a button", () => {
    const html = renderToStaticMarkup(
      <ToolRecords records={[approved]} onStartActivity={() => undefined} />,
    );
    expect(html).toContain("approved");
    expect(html).toContain("This exact text was sent to api.example.com");
    expect(html).toContain("Start Tea Sort");
  });

  it("says only the fixed sentence was sent for a declined lookup, and shows no card", () => {
    const html = renderToStaticMarkup(
      <ToolRecords records={[declined]} onStartActivity={() => undefined} />,
    );
    expect(html).toContain("declined");
    expect(html).toContain("The user declined.");
    expect(html).not.toContain("Start ");
  });

  it("renders nothing for a message with no lookups", () => {
    expect(renderToStaticMarkup(<ToolRecords records={[]} />)).toBe("");
  });

  it("leaves out the Start button when it can't start anything", () => {
    const html = renderToStaticMarkup(<ToolRecords records={[approved]} />);
    expect(html).not.toContain("Start Tea Sort");
  });
});

describe("lookups setting", () => {
  it("is shown off by default and explains the per-lookup approval", () => {
    const html = renderToStaticMarkup(
      <ToolsSetting enabled={false} onChanged={() => undefined} say={() => undefined} />,
    );
    expect(html).toContain(TOOLS_SETTING_LABEL);
    expect(html).not.toContain("checked");
    expect(TOOLS_SETTING_HELP).toContain("Off by default");
    expect(TOOLS_SETTING_HELP).toContain("Approve or Decline for every single lookup");
  });

  it("shows checked when on", () => {
    const html = renderToStaticMarkup(
      <ToolsSetting enabled onChanged={() => undefined} say={() => undefined} />,
    );
    expect(html).toContain("checked");
  });
});
