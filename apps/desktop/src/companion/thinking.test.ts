import { renderToStaticMarkup } from "react-dom/server";
import { createElement } from "react";
import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(() => new Promise(() => undefined)),
  Channel: class {},
}));

import type { StreamEvent } from "./api";
import { applyEvent, beginRequest, getEntry, resetInflight } from "./inflight";
import { initialStream, reduceStream, thinkingMs, type StreamState } from "./logic";
import { ThinkingBlock, thinkingHeader } from "./ThinkingBlock";

const RID = "r-aaaaaaaa";
const run = (events: StreamEvent[], from: StreamState = initialStream(RID)) =>
  events.reduce(reduceStream, from);

describe("thinking in the stream state", () => {
  it("collects thinking apart from the reply text, for its own request only", () => {
    const s = run([
      { type: "thinking", requestId: RID, text: "Let me " },
      { type: "thinking", requestId: "r-bbbbbbbb", text: "LATE" },
      { type: "thinking", requestId: RID, text: "see." },
      { type: "delta", requestId: RID, text: "Answer" },
    ]);
    expect(s.thinking).toBe("Let me see.");
    expect(s.text).toBe("Answer");
    expect(thinkingMs(s)).not.toBeNull();
  });

  it("keeps thinking across rounds, with a blank line between them", () => {
    const s = run([
      { type: "thinking", requestId: RID, text: "First." },
      { type: "round", requestId: RID, round: 2 },
      { type: "thinking", requestId: RID, text: "Second." },
      { type: "thinking", requestId: RID, text: " More." },
    ]);
    expect(s.thinking).toBe("First.\n\nSecond. More.");
    expect(s.text).toBe("");
  });

  it("sums each round's thinking and leaves the approval wait out", () => {
    vi.useFakeTimers();
    try {
      vi.setSystemTime(1_000);
      let s = run([{ type: "thinking", requestId: RID, text: "a" }]);
      vi.setSystemTime(3_000);
      s = run([{ type: "thinking", requestId: RID, text: "b" }], s);
      expect(thinkingMs(s)).toBe(2_000);
      // The lookup waits for a minute; round 2 starts and thinks for one second.
      vi.setSystemTime(63_000);
      s = run([{ type: "round", requestId: RID, round: 2 }], s);
      vi.setSystemTime(64_000);
      s = run([{ type: "thinking", requestId: RID, text: "c" }], s);
      vi.setSystemTime(65_000);
      s = run([{ type: "thinking", requestId: RID, text: "d" }], s);
      expect(thinkingMs(s)).toBe(3_000);
    } finally {
      vi.useRealTimers();
    }
  });

  it("is active only while thinking came after the last text and nothing waits", () => {
    let s = run([{ type: "thinking", requestId: RID, text: "a" }]);
    expect(s.thinkingActive).toBe(true);
    s = run([{ type: "delta", requestId: RID, text: "x" }], s);
    expect(s.thinkingActive).toBe(false);
    s = run([{ type: "thinking", requestId: RID, text: "b" }], s);
    expect(s.thinkingActive).toBe(true);
    s = run(
      [
        {
          type: "toolRequest",
          requestId: RID,
          approvalId: "a",
          tool: "t",
          label: "l",
          arguments: "{}",
          resultText: "r",
          host: "h",
        },
      ],
      s,
    );
    expect(s.thinkingActive).toBe(false);
    s = run([{ type: "round", requestId: RID, round: 2 }], s);
    expect(s.thinkingActive).toBe(false);
  });

  it("has no thinking time before any thinking", () => {
    expect(thinkingMs(initialStream(RID))).toBeNull();
  });
});

describe("the in-flight store keeps live thinking", () => {
  it("holds it for a thread while the user is elsewhere", () => {
    resetInflight();
    beginRequest({
      threadId: 3,
      title: "T",
      requestId: RID,
      optimisticText: "q",
      retryId: null,
      profileId: "p",
      model: "m",
      host: null,
      before: 0,
    });
    applyEvent(3, { type: "thinking", requestId: RID, text: "Hmm." });
    applyEvent(3, { type: "thinking", requestId: "r-other", text: "IGNORED" });
    expect(getEntry(3)!.stream.thinking).toBe("Hmm.");
  });
});

describe("ThinkingBlock", () => {
  it("words the header", () => {
    expect(thinkingHeader(null, true)).toBe("Thinking…");
    expect(thinkingHeader(null, false)).toBe("Thinking");
    expect(thinkingHeader(12_000, false)).toBe("Thought for 12 s");
    expect(thinkingHeader(100, false)).toBe("Thought for 1 s");
    expect(thinkingHeader(125_000, false)).toBe("Thought for 2 min 5 s");
    expect(thinkingHeader(120_000, false)).toBe("Thought for 2 min");
  });

  it("renders nothing for blank text and plain text otherwise", () => {
    expect(renderToStaticMarkup(createElement(ThinkingBlock, { text: " ​ " }))).toBe("");
    const html = renderToStaticMarkup(createElement(ThinkingBlock, { text: "<b>x</b>", ms: 3000 }));
    expect(html).toContain("&lt;b&gt;x&lt;/b&gt;");
    expect(html).toContain('aria-expanded="false"');
    expect(html).toContain("Thought for 3 s");
  });

  it("is open by default only while active", () => {
    const open = renderToStaticMarkup(createElement(ThinkingBlock, { text: "x", active: true }));
    expect(open).toContain('aria-expanded="true"');
  });
});
