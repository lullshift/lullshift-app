import { describe, expect, it } from "vitest";
import { createRng, dailySeed } from "../src/core/index.ts";
import { Grid, generateGrid, scatter } from "../src/grid/index.ts";

const at = (g: Grid<unknown>) => [g.cursor.x, g.cursor.y];

describe("grid model", () => {
  it("builds cells in reading order and checks bounds", () => {
    const g = new Grid(3, 2, (x, y) => `${x},${y}`);
    expect(g.cells).toEqual(["0,0", "1,0", "2,0", "0,1", "1,1", "2,1"]);
    expect(g.get(2, 1)).toBe("2,1");
    expect(g.get(3, 0)).toBeUndefined();
    expect(g.set(-1, 0, "x")).toBe(false);
    expect(g.point(4)).toEqual({ x: 1, y: 1 });
    expect(g.neighbours(0, 0)).toEqual([
      { x: 1, y: 0 },
      { x: 0, y: 1 },
    ]);
    expect(g.neighbours(1, 0, true)).toHaveLength(5);
    expect(() => new Grid(0, 3, 0)).toThrow(RangeError);
    expect(() => new Grid(300, 3, 0)).toThrow(RangeError);
  });

  it("stops at edges without wrap", () => {
    const g = new Grid(3, 3, 0);
    expect(g.navigate("left")).toBe(false);
    expect(g.navigate("up")).toBe(false);
    expect(g.navigate("right")).toBe(true);
    expect(g.navigate("right")).toBe(true);
    expect(g.navigate("right")).toBe(false);
    expect(at(g)).toEqual([2, 0]);
    expect(g.navigate("pageDown")).toBe(true);
    expect(at(g)).toEqual([2, 2]);
    expect(g.navigate("home")).toBe(true);
    expect(at(g)).toEqual([0, 2]);
    expect(g.navigate("end")).toBe(true);
    expect(at(g)).toEqual([2, 2]);
    expect(g.navigate("pageUp")).toBe(true);
    expect(at(g)).toEqual([2, 0]);
    expect(g.navigate("last")).toBe(true);
    expect(at(g)).toEqual([2, 2]);
    expect(g.navigate("first")).toBe(true);
    expect(at(g)).toEqual([0, 0]);
    expect(g.navigate("first")).toBe(false);
    expect(g.navigate("undo")).toBe(false);
  });

  it("wraps within the row or column when asked", () => {
    const g = new Grid(4, 3, 0, { wrap: true });
    expect(g.navigate("left")).toBe(true);
    expect(at(g)).toEqual([3, 0]);
    expect(g.navigate("up")).toBe(true);
    expect(at(g)).toEqual([3, 2]);
    expect(g.navigate("down")).toBe(true);
    expect(at(g)).toEqual([3, 0]);
    expect(g.navigate("right")).toBe(true);
    expect(at(g)).toEqual([0, 0]);
    const one = new Grid(1, 1, 0, { wrap: true });
    expect(one.navigate("left")).toBe(false);
  });

  it("selects cells", () => {
    const g = new Grid(3, 3, 0);
    g.select(2, 2);
    expect(g.toggleSelect(0, 1)).toBe(true);
    expect(g.toggleSelect(0, 1)).toBe(false);
    g.select(1, 0);
    g.select(9, 9);
    expect(g.selection()).toEqual([
      { x: 1, y: 0 },
      { x: 2, y: 2 },
    ]);
    g.clearSelection();
    expect(g.selection()).toEqual([]);
  });

  it("undoes and redoes edits, and skips no-op edits", () => {
    const g = new Grid(2, 2, 0);
    expect(g.edit((x) => x.set(0, 0, 1))).toBe(true);
    expect(g.edit((x) => x.set(1, 1, 2))).toBe(true);
    expect(g.edit((x) => x.set(1, 1, 2))).toBe(false);
    expect(g.cells).toEqual([1, 0, 0, 2]);
    expect(g.undo()).toBe(true);
    expect(g.cells).toEqual([1, 0, 0, 0]);
    expect(g.undo()).toBe(true);
    expect(g.undo()).toBe(false);
    expect(g.redo()).toBe(true);
    expect(g.cells).toEqual([1, 0, 0, 0]);
    expect(g.canRedo()).toBe(true);
  });

  it("caps history", () => {
    const g = new Grid(1, 1, 0, { historyLimit: 2 });
    for (let i = 1; i <= 5; i++) g.edit((x) => x.set(0, 0, i));
    expect(g.undo() && g.undo()).toBe(true);
    expect(g.undo()).toBe(false);
    expect(g.cells).toEqual([3]);
  });
});

describe("grid snapshot and restore", () => {
  function played() {
    const g = new Grid<boolean>(3, 2, false);
    g.edit((x) => x.set(1, 1, true));
    g.edit((x) => x.set(0, 0, true));
    g.undo();
    g.setCursor(2, 1);
    g.select(1, 0);
    return g;
  }

  it("round-trips through JSON", () => {
    const g = played();
    const saved = JSON.parse(JSON.stringify(g.snapshot())) as unknown;
    const h = new Grid<boolean>(3, 2, false);
    expect(h.restore(saved, (v) => typeof v === "boolean")).toBe(true);
    expect(h.snapshot()).toEqual(g.snapshot());
    expect(h.redo()).toBe(true);
    expect(h.cells).toEqual([true, false, false, false, true, false]);
  });

  it("accepts a snapshot without history or selection", () => {
    const h = new Grid<number>(2, 1, 0);
    expect(h.restore({ width: 2, height: 1, cells: [1, 2], cursor: { x: 1, y: 0 } })).toBe(true);
    expect(h.cells).toEqual([1, 2]);
    expect(h.canUndo()).toBe(false);
  });

  it("rejects malformed input and changes nothing", () => {
    const good = played().snapshot();
    const bad: unknown[] = [
      null,
      42,
      "grid",
      [],
      { ...good, width: 4 },
      { ...good, cells: good.cells.slice(1) },
      { ...good, cells: [...good.cells.slice(1), "x"] },
      { ...good, cells: [...good.cells.slice(1), undefined] },
      { ...good, cursor: { x: 3, y: 0 } },
      { ...good, cursor: { x: 0.5, y: 0 } },
      { ...good, cursor: null },
      { ...good, selected: [99] },
      { ...good, selected: "all" },
      { ...good, history: { undo: [[true]], redo: [] } },
      { ...good, history: { undo: "x", redo: [] } },
    ];
    const h = new Grid<boolean>(3, 2, false);
    const before = h.snapshot();
    for (const b of bad) {
      expect(h.restore(b, (v) => typeof v === "boolean")).toBe(false);
      expect(h.snapshot()).toEqual(before);
    }
    // Without a cell check, non-JSON values are still refused.
    expect(h.restore({ ...good, cells: [...good.cells.slice(1), () => 1] })).toBe(false);
  });

  it("restores a hostile oversized history fast", () => {
    const g = new Grid<number>(256, 256, 0, { historyLimit: 10_000 });
    const cells = new Array<number>(g.size).fill(1);
    // 10 000 full-size entries (one shared array, so building it is cheap): 655 million cells.
    const undo = new Array<number[]>(10_000).fill(cells);
    const state = { ...g.snapshot(), history: { undo, redo: undo } };
    const t0 = performance.now();
    expect(g.restore(state)).toBe(true);
    expect(performance.now() - t0).toBeLessThan(100);
    expect(g.canUndo()).toBe(true);
    expect(g.canRedo()).toBe(false); // the budget went to the newest undo steps
    // A selection longer than the grid is refused outright.
    const many = new Array<number>(g.size + 1).fill(0);
    expect(g.restore({ ...g.snapshot(), selected: many })).toBe(false);
  });
});

describe("seeded generators", () => {
  it("generates the same grid from the same seed", () => {
    const make = () => generateGrid(4, 4, createRng(dailySeed("2026-11-05")), (r) => r.int(0, 9));
    expect(make().cells).toEqual(make().cells);
    const other = generateGrid(4, 4, createRng(dailySeed("2026-11-06")), (r) => r.int(0, 9));
    expect(other.cells).not.toEqual(make().cells);
  });

  it("scatters distinct cells and shuffles with undo", () => {
    const g = new Grid(5, 5, 0);
    const picks = scatter(g, createRng(3), 6);
    expect(picks).toHaveLength(6);
    expect(new Set(picks.map((p) => g.index(p.x, p.y))).size).toBe(6);
    expect(scatter(g, createRng(3), 6)).toEqual(picks);
    expect(scatter(g, createRng(3), 99, (_c, x) => x === 0)).toHaveLength(5);
    const s = new Grid(3, 3, (x, y) => y * 3 + x);
    s.shuffle(createRng(1));
    expect([...s.cells].sort()).toEqual([0, 1, 2, 3, 4, 5, 6, 7, 8]);
    expect(s.undo()).toBe(true);
    expect(s.cells).toEqual([0, 1, 2, 3, 4, 5, 6, 7, 8]);
  });
});
