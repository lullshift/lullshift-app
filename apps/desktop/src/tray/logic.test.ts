import { describe, expect, it } from "vitest";
import type { Listed } from "../activity/library";
import { CHIPS_MAX, quickChips } from "./logic";

const act = (id: string, category: string, savedUtc: string | null = null): Listed => ({
  id,
  name: id,
  description: "",
  category,
  savedUtc,
});

const LIST = [
  act("g1", "arcade"),
  act("g2", "arcade"),
  act("g3", "interactive"),
  act("b1", "break"),
  act("b2", "break"),
];

const input = (activities: Listed[], anyWorking = true, estimateSeconds: number | null = 600) => ({
  activities,
  anyWorking,
  estimateSeconds,
  seed: 20261107,
});

describe("quickChips", () => {
  it("never repeats the suggestion and shows at most three", () => {
    const chips = quickChips(input(LIST), "g1");
    expect(chips.length).toBeLessThanOrEqual(CHIPS_MAX);
    expect(chips.map((c) => c.id)).not.toContain("g1");
    expect(new Set(chips.map((c) => c.id)).size).toBe(chips.length);
  });

  it("mixes a rest and a game before more of one kind", () => {
    const chips = quickChips(input(LIST), "g1");
    expect(chips.some((c) => c.category === "break")).toBe(true);
    expect(chips.some((c) => c.category !== "break")).toBe(true);
  });

  it("is stable for the same date and different tomorrow only through the seed", () => {
    expect(quickChips(input(LIST), "g1")).toEqual(quickChips(input(LIST), "g1"));
  });

  it("copes with nothing, or with only the suggestion", () => {
    expect(quickChips(input([]), null)).toEqual([]);
    expect(quickChips(input([act("only", "arcade")]), "only")).toEqual([]);
  });

  it("includes a saved activity only through the normal order (it is the suggestion)", () => {
    const withSave = [...LIST, act("saved", "arcade", "2026-11-07T10:00:00Z")];
    // The newest save is the first suggestion, so with it shown it is not a chip.
    expect(quickChips(input(withSave), "saved").map((c) => c.id)).not.toContain("saved");
  });
});
