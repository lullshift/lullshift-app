// Rule and schema tests for the Creative Toy engine (activities/engines/toy).
/// <reference types="node" />
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { createAjv } from "@lullshift/agent-protocol";
import { describe, expect, it } from "vitest";

const root = fileURLToPath(new URL("../../../../../", import.meta.url));
new Function("window", readFileSync(join(root, "activities/engines/toy/logic.js"), "utf8"))(
  globalThis,
);

type Mark = [number, number, number, number];
interface Content {
  title: string;
  intro?: string;
  palette: { name: string; color: string }[];
  mode: string;
  symmetry: string;
  min: number;
  max: number;
  background: string;
}
interface State {
  marks: Mark[];
  cursor: { x: number; y: number };
  color: number;
  size: number;
}
interface Logic {
  W: number;
  H: number;
  MAX_MARKS: number;
  moveCursor(
    c: { x: number; y: number },
    dx: number,
    dy: number,
    big?: boolean,
  ): { x: number; y: number };
  snapToCell(p: { x: number; y: number }): { x: number; y: number };
  makeMark(x: number, y: number, c: number, s: number, mode: string): Mark;
  addMark(marks: Mark[], m: Mark): Mark[];
  undo(marks: Mark[]): Mark[];
  reflect(p: { x: number; y: number }, sym: string): { x: number; y: number }[];
  describe(marks: Mark[]): string;
  cursorInk(bg: string): string;
  normalizeContent(c: unknown): Content | null;
  restoreState(raw: unknown, content: Content): State | null;
}
const L = (globalThis as unknown as { ToyLogic: Logic }).ToyLogic;

const schema = JSON.parse(
  readFileSync(join(root, "activities/engines/toy/data.schema.json"), "utf8"),
);
const validate = createAjv({ allErrors: true }).compile(schema);

function fixtures(kind: "valid" | "invalid"): [string, unknown][] {
  const dir = join(root, "tests/fixtures/engines/lullshift.toy@1", kind);
  return readdirSync(dir)
    .filter((f) => f.endsWith(".json"))
    .map((f) => [f, JSON.parse(readFileSync(join(dir, f), "utf8")) as unknown]);
}

const content = L.normalizeContent({
  title: "T",
  palette: [
    { name: "A", color: "#111111" },
    { name: "B", color: "#222222" },
    { name: "C", color: "#333333" },
  ],
  mode: "stars",
  brushSize: { min: 4, max: 12 },
})!;

describe("ToyLogic cursor and marks", () => {
  it("moves in steps and stays on the canvas", () => {
    expect(L.moveCursor({ x: 160, y: 120 }, 1, 0)).toEqual({ x: 168, y: 120 });
    expect(L.moveCursor({ x: 160, y: 120 }, 0, -1, true)).toEqual({ x: 160, y: 88 });
    expect(L.moveCursor({ x: 0, y: 0 }, -1, -1)).toEqual({ x: 0, y: 0 });
    expect(L.moveCursor({ x: L.W, y: L.H }, 1, 1, true)).toEqual({ x: L.W, y: L.H });
  });

  it("snaps mosaic marks to the grid centre", () => {
    expect(L.snapToCell({ x: 3, y: 3 })).toEqual({ x: 8, y: 8 });
    expect(L.makeMark(20, 20, 1, 6, "mosaic")).toEqual([24, 24, 1, 6]);
    expect(L.makeMark(20.4, 20.6, 1, 6, "ripples")).toEqual([20, 21, 1, 6]);
    expect(L.makeMark(500, -5, 0, 4, "stars")).toEqual([L.W, 0, 0, 4]);
  });

  it("adds without mutating, caps the list and undoes", () => {
    const a: Mark[] = [[1, 1, 0, 4]];
    const b = L.addMark(a, [2, 2, 0, 4]);
    expect(a).toHaveLength(1);
    expect(b).toHaveLength(2);
    let many: Mark[] = [];
    for (let i = 0; i < L.MAX_MARKS + 5; i++) many = L.addMark(many, [i % 300, 1, 0, 4]);
    expect(many).toHaveLength(L.MAX_MARKS);
    expect(many[many.length - 1]![0]).toBe((L.MAX_MARKS + 4) % 300);
    expect(L.undo(b)).toEqual(a);
    expect(L.undo([])).toEqual([]);
  });

  it("describes the canvas", () => {
    expect(L.describe([])).toBe("Empty canvas");
    expect(L.describe([[1, 1, 0, 4]])).toBe("1 mark in 1 colour");
    expect(
      L.describe([
        [1, 1, 0, 4],
        [2, 2, 1, 4],
        [3, 3, 1, 4],
      ]),
    ).toBe("3 marks in 2 colours");
  });
});

describe("ToyLogic symmetry", () => {
  const p = { x: 40, y: 50 };
  it("gives the right number of points", () => {
    expect(L.reflect(p, "none")).toHaveLength(1);
    expect(L.reflect(p, "mirror")).toHaveLength(2);
    expect(L.reflect(p, "quad")).toHaveLength(4);
    expect(L.reflect(p, "radial6")).toHaveLength(6);
  });
  it("mirrors across the centre line", () => {
    expect(L.reflect(p, "mirror")[1]).toEqual({ x: L.W - 40, y: 50 });
    expect(L.reflect(p, "quad")[3]).toEqual({ x: L.W - 40, y: L.H - 50 });
  });
  it("rotates six ways around the centre", () => {
    const pts = L.reflect({ x: L.W / 2 + 50, y: L.H / 2 }, "radial6");
    expect(pts[0]).toEqual({ x: L.W / 2 + 50, y: L.H / 2 });
    expect(pts[3]!.x).toBeCloseTo(L.W / 2 - 50, 0);
    expect(pts[3]!.y).toBeCloseTo(L.H / 2, 0);
  });
});

describe("ToyLogic content and restore", () => {
  it("normalises pack data and fills defaults", () => {
    expect(content.symmetry).toBe("none");
    expect(content.background).toBe("#fffdf9");
    expect(L.normalizeContent(null)).toBeNull();
    expect(L.normalizeContent({ title: "x", palette: [], mode: "ripples" })).toBeNull();
    expect(
      L.normalizeContent({
        title: "x",
        palette: [
          { name: "a", color: "#000000" },
          { name: "b", color: "red" },
        ],
        mode: "ripples",
      }),
    ).toBeNull();
    expect(
      L.normalizeContent({
        title: "x",
        palette: content.palette,
        mode: "ripples",
        brushSize: { min: 10, max: 5 },
      }),
    ).toBeNull();
  });

  it("restores exactly what was saved", () => {
    const saved: State = {
      marks: [
        [10, 20, 0, 4],
        [30, 40, 2, 12],
      ],
      cursor: { x: 72, y: 96 },
      color: 2,
      size: 8,
    };
    const roundTrip = JSON.parse(JSON.stringify(saved)) as unknown;
    expect(L.restoreState(roundTrip, content)).toEqual(saved);
  });

  it("drops bad marks and clamps odd values", () => {
    const s = L.restoreState(
      {
        marks: [[1, 1, 0, 4], [1, 1, 9, 4], "x", [1.5, 1, 0, 4], [5, 5, 1, 30]],
        cursor: { x: 9999, y: "a" },
        color: 7,
        size: 1,
      },
      content,
    )!;
    expect(s.marks).toEqual([
      [1, 1, 0, 4],
      [5, 5, 1, 12],
    ]);
    expect(s.cursor).toEqual({ x: L.W, y: L.H / 2 });
    expect(s.color).toBe(0);
    expect(s.size).toBe(4);
    expect(L.restoreState(null, content)).toBeNull();
    expect(L.restoreState({ marks: "no" }, content)).toBeNull();
  });

  it("keeps only the newest marks when a saved list is too long", () => {
    const marks = Array.from({ length: L.MAX_MARKS + 10 }, (_, i) => [i % 300, 0, 0, 4]);
    const s = L.restoreState({ marks }, content)!;
    expect(s.marks).toHaveLength(L.MAX_MARKS);
  });

  it("picks a readable cursor ink", () => {
    expect(L.cursorInk("#ffffff")).toBe("#2a2633");
    expect(L.cursorInk("#14213d")).toBe("#f6f2ec");
  });
});

describe("lullshift.toy@1 data schema", () => {
  it("accepts every valid fixture", () => {
    const list = fixtures("valid");
    expect(list.length).toBeGreaterThanOrEqual(3);
    for (const [name, data] of list) {
      expect(validate(data), `${name}: ${JSON.stringify(validate.errors)}`).toBe(true);
    }
  });
  it("rejects every invalid fixture", () => {
    const list = fixtures("invalid");
    expect(list.length).toBeGreaterThanOrEqual(4);
    for (const [name, data] of list) expect(validate(data), name).toBe(false);
  });
  it("accepts the bundled packs", () => {
    const dir = join(root, "activities/content/toy");
    for (const slug of readdirSync(dir)) {
      const data = JSON.parse(readFileSync(join(dir, slug, "toy.json"), "utf8")) as unknown;
      expect(validate(data), slug).toBe(true);
      expect(L.normalizeContent(data), slug).not.toBeNull();
    }
  });
});
