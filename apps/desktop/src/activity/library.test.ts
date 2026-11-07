import { describe, expect, it } from "vitest";
import {
  availableChips,
  filterLibrary,
  helpLabel,
  matchesQuery,
  onPage,
  pageOf,
  savedNewestFirst,
  PLAY_CHIPS,
  REST_CHIPS,
  type Listed,
} from "./library";

const item = (
  id: string,
  category: string,
  tags: string[] = [],
  extra: Partial<Listed> = {},
): Listed => ({
  id,
  name: `Name ${id}`,
  description: `About ${id}`,
  category,
  tags,
  savedUtc: null,
  ...extra,
});

const LIST = [
  item("a", "arcade", ["puzzle", "calm"]),
  item("b", "interactive", ["quiz"]),
  item("c", "productivity", ["typing"]),
  item("d", "break", ["breathing", "calm"]),
  item("e", "break", ["evil-tag"]),
];

describe("pages", () => {
  it("splits by category only", () => {
    expect(pageOf("break")).toBe("rest");
    for (const c of ["arcade", "interactive", "productivity"]) expect(pageOf(c)).toBe("play");
    expect(onPage(LIST, "play").map((x) => x.id)).toEqual(["a", "b", "c"]);
    expect(onPage(LIST, "rest").map((x) => x.id)).toEqual(["d", "e"]);
  });
  it("words the help link for the page", () => {
    expect(helpLabel("arcade")).toBe("How to play");
    expect(helpLabel("break")).toBe("How it works");
  });
});

describe("search", () => {
  it("matches name, description and tags, ignoring case", () => {
    expect(matchesQuery(LIST[0]!, "NAME A")).toBe(true);
    expect(matchesQuery(LIST[0]!, "about a")).toBe(true);
    expect(matchesQuery(LIST[0]!, "puz")).toBe(true);
    expect(matchesQuery(LIST[0]!, "nothing")).toBe(false);
    expect(matchesQuery(LIST[0]!, "   ")).toBe(true);
  });
  it("finds a third-party tag by search", () => {
    expect(filterLibrary(LIST, "rest", "evil", null).map((x) => x.id)).toEqual(["e"]);
  });
});

describe("filter chips", () => {
  it("come from the fixed host map, never from manifest tags", () => {
    expect(PLAY_CHIPS.map((c) => c.label)).toEqual([
      "Puzzles",
      "Calm",
      "Quizzes",
      "Typing",
      "Toys",
      "Cards",
    ]);
    expect(REST_CHIPS.map((c) => c.label)).toEqual([
      "Breathing",
      "Movement",
      "Step away",
      "Reflection",
      "Pause",
      "Eyes",
    ]);
    const shown = availableChips(LIST, "rest").map((c) => c.label);
    expect(shown).toEqual(["Breathing"]);
    expect(shown.join()).not.toContain("evil");
  });
  it("filters the page by a chip and combines it with the search", () => {
    expect(filterLibrary(LIST, "play", "", "calm").map((x) => x.id)).toEqual(["a"]);
    expect(filterLibrary(LIST, "play", "name b", "calm")).toEqual([]);
    expect(filterLibrary(LIST, "play", "", "quizzes").map((x) => x.id)).toEqual(["b"]);
    expect(filterLibrary(LIST, "play", "", null)).toHaveLength(3);
  });
});

describe("savedNewestFirst", () => {
  it("lists only saved activities, newest first", () => {
    const l = [
      item("x", "arcade", [], { savedUtc: "2026-11-07T08:00:00Z" }),
      item("y", "break"),
      item("z", "arcade", [], { savedUtc: "2026-11-07T09:30:00Z" }),
    ];
    expect(savedNewestFirst(l).map((i) => i.id)).toEqual(["z", "x"]);
  });
});
