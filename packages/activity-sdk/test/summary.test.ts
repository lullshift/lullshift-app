import { describe, expect, it } from "vitest";
import { isPlainText, parseActivityMessage, parseSummary } from "../src/host/protocol";

const N = "b".repeat(32);

describe("summary messages", () => {
  it("accepts one to four plain label/value pairs", () => {
    const items = [
      { label: "Moves", value: 12 },
      { label: "Best", value: "9 moves" },
    ];
    expect(parseActivityMessage({ lullshift: 1, nonce: N, type: "summary", items }, N)).toEqual({
      ok: true,
      message: { type: "summary", items },
    });
  });

  it("rejects empty, long, extra-field and non-text summaries", () => {
    expect(parseSummary([])).toBeNull();
    expect(parseSummary("Moves: 3")).toBeNull();
    expect(parseSummary(Array(5).fill({ label: "a", value: 1 }))).toBeNull();
    expect(parseSummary([{ label: "x".repeat(25), value: 1 }])).toBeNull();
    expect(parseSummary([{ label: "Moves", value: "y".repeat(25) }])).toBeNull();
    expect(parseSummary([{ label: "Moves", value: Infinity }])).toBeNull();
    expect(parseSummary([{ label: "Moves", value: { html: "<b>" } }])).toBeNull();
    expect(parseSummary([{ label: "Moves", value: 1, color: "red" }])).toBeNull();
    expect(parseSummary([{ label: "", value: 1 }])).toBeNull();
  });

  it("refuses control, bidi and invisible characters", () => {
    for (const bad of ["a\nb", "a‮b", "a​b", "a\u0000b", "a﻿b", "a⁦b"]) {
      expect(isPlainText(bad, 24)).toBe(false);
    }
    expect(isPlainText("Lanterns lit · 7/9", 24)).toBe(true);
    // Counted in characters, not UTF-16 units.
    expect(isPlainText("🏮".repeat(24), 24)).toBe(true);
  });

  it("copies items, so later changes by the activity don't leak in", () => {
    const item = { label: "Moves", value: 1 };
    const parsed = parseSummary([item]);
    item.value = 99;
    expect(parsed).toEqual([{ label: "Moves", value: 1 }]);
  });
});
