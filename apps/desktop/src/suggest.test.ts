import { describe, expect, it } from "vitest";
import {
  dateSeed,
  pickSuggestion,
  preferredPage,
  seededOrder,
  suggestionAction,
  suggestionOrder,
  waitFor,
  waitLine,
  type SuggestInput,
} from "./suggest";

interface A {
  id: string;
  name: string;
  description: string;
  category: string;
  savedUtc: string | null;
}
const a = (id: string, category: string, savedUtc: string | null = null): A => ({
  id,
  name: id,
  description: "",
  category,
  savedUtc,
});
const ACTIVITIES = [
  a("g1", "arcade"),
  a("g2", "interactive"),
  a("g3", "productivity"),
  a("b1", "break"),
  a("b2", "break"),
];
const input = (over: Partial<SuggestInput<A>> = {}): SuggestInput<A> => ({
  activities: ACTIVITIES,
  estimateSeconds: null,
  anyWorking: true,
  seed: 20261107,
  ...over,
});

describe("preferredPage", () => {
  it("rests when the wait is short, unknown, or nothing is working", () => {
    expect(preferredPage(239, true)).toBe("rest");
    expect(preferredPage(null, true)).toBe("rest");
    expect(preferredPage(600, false)).toBe("rest");
  });
  it("plays when an agent is working and its usual stretch is 4 minutes or more", () => {
    expect(preferredPage(240, true)).toBe("play");
    expect(preferredPage(900, true)).toBe("play");
  });
});

describe("suggestionOrder", () => {
  it("offers the newest saved activity first, as a resume", () => {
    const list = [
      a("g1", "arcade", "2026-11-07T08:00:00Z"),
      a("b1", "break", "2026-11-07T09:00:00Z"),
      a("g2", "arcade"),
    ];
    const order = suggestionOrder(input({ activities: list, estimateSeconds: 900 }));
    expect(order[0]).toMatchObject({ kind: "resume", activity: { id: "b1" } });
    expect(order.map((s) => s.activity.id).sort()).toEqual(["b1", "g1", "g2"]);
  });

  it("puts rest items first for a short wait and play items first for a long one", () => {
    const short = suggestionOrder(input({ estimateSeconds: 120 }));
    expect(short.slice(0, 2).every((s) => s.kind === "rest")).toBe(true);
    const long = suggestionOrder(input({ estimateSeconds: 480 }));
    expect(long.slice(0, 3).every((s) => s.kind === "play")).toBe(true);
    expect(long.slice(3).every((s) => s.kind === "rest")).toBe(true);
  });

  it("suggests a rest item when no agent is working, whatever the estimate", () => {
    const order = suggestionOrder(input({ anyWorking: false, estimateSeconds: 900 }));
    expect(order[0]?.kind).toBe("rest");
  });

  it("lists every activity once and nothing when there are none", () => {
    expect(
      suggestionOrder(input())
        .map((s) => s.activity.id)
        .sort(),
    ).toEqual(ACTIVITIES.map((x) => x.id).sort());
    expect(suggestionOrder(input({ activities: [] }))).toEqual([]);
    expect(pickSuggestion(input({ activities: [] }), 3)).toBeNull();
  });

  it("keeps the same order for the same day and may change it on another day", () => {
    const ids = (seed: number) => suggestionOrder(input({ seed })).map((s) => s.activity.id);
    expect(ids(20261107)).toEqual(ids(20261107));
    const days = new Set(
      [20261107, 20261108, 20261109, 20261110, 20261111].map((s) => ids(s).join()),
    );
    expect(days.size).toBeGreaterThan(1);
  });
});

describe("pickSuggestion (Shuffle)", () => {
  it("walks the stable order and wraps around", () => {
    const order = suggestionOrder(input({ estimateSeconds: 600 }));
    for (let i = 0; i < order.length; i++) {
      expect(pickSuggestion(input({ estimateSeconds: 600 }), i)?.activity.id).toBe(
        order[i]?.activity.id,
      );
    }
    expect(pickSuggestion(input({ estimateSeconds: 600 }), order.length)?.activity.id).toBe(
      order[0]?.activity.id,
    );
  });
});

describe("seededOrder and dateSeed", () => {
  it("does not mutate its input and is a permutation", () => {
    const list = [{ id: "x" }, { id: "y" }, { id: "z" }];
    const out = seededOrder(list, 1);
    expect(list.map((i) => i.id)).toEqual(["x", "y", "z"]);
    expect(out.map((i) => i.id).sort()).toEqual(["x", "y", "z"]);
  });
  it("makes one number per calendar day", () => {
    expect(dateSeed(new Date(2026, 10, 7, 1))).toBe(20261107);
    expect(dateSeed(new Date(2026, 10, 7, 23))).toBe(20261107);
    expect(dateSeed(new Date(2026, 10, 8))).toBe(20261108);
  });
});

describe("model-written sets", () => {
  it("are never suggested, even if one slips into the list or has the newest save", () => {
    const set = a("lullshift.generated.movement.abcdefghij12", "break", "2026-11-08T10:00:00Z");
    const input: SuggestInput<A> = {
      activities: [...ACTIVITIES, set],
      estimateSeconds: null,
      anyWorking: true,
      seed: 20261108,
    };
    expect(suggestionOrder(input).map((s) => s.activity.id)).not.toContain(set.id);
    for (let i = 0; i < 20; i++) {
      expect(pickSuggestion(input, i)?.activity.id.startsWith("lullshift.generated.")).toBe(false);
    }
    // Only sets: nothing to suggest.
    expect(pickSuggestion({ ...input, activities: [set] }, 0)).toBeNull();
  });
});

describe("waitFor and waitLine", () => {
  const est = [
    { provider: "claude-code", medianSeconds: 370, stretches: 12 },
    { provider: "codex", medianSeconds: 100, stretches: 6 },
  ];
  it("picks the first working tool that has an estimate", () => {
    expect(waitFor(["cursor", "codex"], est)?.provider).toBe("codex");
    expect(waitFor(["cursor"], est)).toBeNull();
    expect(waitFor([], est)).toBeNull();
  });
  it("words it as a usual length, never a finish time", () => {
    expect(waitLine(est[0] ?? null, true)).toBe("Claude Code usually takes about 6 min");
    expect(waitLine({ provider: "codex", medianSeconds: 20, stretches: 9 }, true)).toBe(
      "Codex usually takes about 1 min",
    );
    expect(waitLine(null, true)).toBe("For a short break");
    expect(waitLine(est[0] ?? null, false)).toBe("If you'd like a break");
    expect(waitLine(null, false)).toBe("If you'd like a break");
  });
  it("names the action", () => {
    expect(suggestionAction("resume")).toBe("Resume");
    expect(suggestionAction("rest")).toBe("Rest");
    expect(suggestionAction("play")).toBe("Play");
  });
});
