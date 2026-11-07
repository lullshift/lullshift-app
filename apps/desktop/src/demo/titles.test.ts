// @vitest-environment jsdom
// The demo backend mirrors the core's thread titles (title_from_message in the local store).

import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { titleFromMessage } from "./backend";

describe("thread titles from the first message", () => {
  it("collapses whitespace and drops invisible and control characters", () => {
    expect(titleFromMessage("  How do I\n\tfix   the flaky​ test?\u0007 ")).toBe(
      "How do I fix the flaky test?",
    );
    expect(titleFromMessage("​‮\u0007")).toBeNull();
    expect(titleFromMessage(" \n ")).toBeNull();
  });

  it("cuts long messages on a word boundary with an ellipsis", () => {
    const long = "Please explain why the retry loop in the token refresh code never stops";
    expect(titleFromMessage(long)).toBe("Please explain why the retry loop in the token…");
    expect(titleFromMessage("a".repeat(48))).toBe("a".repeat(48));
    expect(titleFromMessage(`${"b".repeat(47)} tail words`)).toBe(`${"b".repeat(47)}…`);
    expect(titleFromMessage("c".repeat(100))).toBe(`${"c".repeat(48)}…`);
  });
});
