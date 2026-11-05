// The committed classic scripts: up to date, loadable without modules, and exposing each kit on
// window.LullshiftKits without touching other kits.
import { readFileSync } from "node:fs";
import path from "node:path";
import vm from "node:vm";
import { describe, expect, it } from "vitest";
import { DIST_DIR, KITS, buildKits, convertSource } from "../scripts/build-kits.ts";
import * as arcade from "../src/arcade/index.ts";
import * as card from "../src/card/index.ts";
import * as grid from "../src/grid/index.ts";

const sources: Record<string, Record<string, unknown>> = { grid, card, arcade };
const read = (name: string) =>
  readFileSync(path.join(DIST_DIR, name), "utf8").replaceAll("\r\n", "\n");

function load(kits: string[], window: Record<string, unknown> = {}) {
  const context = vm.createContext({ window });
  for (const kit of kits) vm.runInContext(read(`lullshift-${kit}-kit-v1.js`), context);
  return window as { LullshiftKits?: Record<string, Record<string, unknown>> };
}

describe("generated kit files", () => {
  it("match a fresh build", { timeout: 60_000 }, async () => {
    const built = await buildKits();
    expect([...built.keys()].sort()).toEqual(
      Object.keys(KITS)
        .flatMap((k) => [`lullshift-${k}-kit-v1.js`, `lullshift-${k}-kit-v1.d.ts`])
        .sort(),
    );
    for (const [name, text] of built) expect(read(name), name).toBe(text);
  });

  it("expose each kit's API as a frozen object", () => {
    for (const kit of Object.keys(KITS)) {
      const w = load([kit]);
      const api = w.LullshiftKits?.[kit];
      expect(api, kit).toBeDefined();
      expect(Object.isFrozen(api)).toBe(true);
      expect(Object.keys(api!).sort()).toEqual(Object.keys(sources[kit]!).sort());
    }
  });

  it("add to window.LullshiftKits without replacing other kits", () => {
    const mine = { custom: true };
    const w = load(["grid", "card", "arcade"], { LullshiftKits: { mine } });
    expect(w.LullshiftKits?.mine).toBe(mine);
    expect(Object.keys(w.LullshiftKits!).sort()).toEqual(["arcade", "card", "grid", "mine"]);
  });

  it("attach to globalThis where there is no window", () => {
    const context = vm.createContext({});
    vm.runInContext(read("lullshift-grid-kit-v1.js"), context);
    const kits = vm.runInContext("globalThis.LullshiftKits", context) as Record<string, unknown>;
    expect(Object.keys(kits)).toEqual(["grid"]);
  });

  it("work the same as the TypeScript sources", () => {
    const w = load(["grid", "card"]);
    const g = w.LullshiftKits!.grid as unknown as typeof grid;
    const c = w.LullshiftKits!.card as unknown as typeof card;
    expect(g.createRng(42).shuffle([1, 2, 3, 4, 5])).toEqual(
      grid.createRng(42).shuffle([1, 2, 3, 4, 5]),
    );
    expect(g.dailySeed("2026-11-05")).toBe(grid.dailySeed("2026-11-05"));
    const model = new g.Grid(3, 3, 0);
    model.navigate("right");
    model.edit((x) => x.set(1, 0, 1));
    expect(JSON.stringify(model.snapshot())).toBe(
      JSON.stringify(
        (() => {
          const m = new grid.Grid(3, 3, 0);
          m.navigate("right");
          m.edit((x) => x.set(1, 0, 1));
          return m.snapshot();
        })(),
      ),
    );
    expect(c.buildDeck()).toHaveLength(52);
  });

  it("carry the license and do-not-edit header and no blocked APIs", () => {
    for (const kit of Object.keys(KITS)) {
      const js = read(`lullshift-${kit}-kit-v1.js`);
      expect(js.split("\n")[0]).toMatch(new RegExp(`^// Lullshift ${kit} kit v1`));
      expect(js).toMatch(/Apache License, Version 2\.0/);
      expect(js).toMatch(/GENERATED, DO NOT EDIT/);
      expect(js).not.toMatch(/\bimport\b|\bexport\b|innerHTML|fetch|localStorage|eval\(/);
    }
  });
});

describe("kit source rules", () => {
  const allowed = new Set(["src/grid/a.ts", "src/core/json.ts"]);
  const convert = (src: string) => convertSource("src/grid/a.ts", src, allowed);

  it("inlines allowed imports and gathers exports", () => {
    const out = convert(
      'import { isRecord } from "../core/json.ts";\nimport type { X } from "./x.ts";\nexport function f(a: number): number { return a; }\nexport const N: number = 1;\n',
    );
    expect(out.exports).toEqual(["f", "N"]);
    expect(out.code).not.toMatch(/\bimport\b|\bexport\b/);
  });

  it("refuses side-effect imports and keeps code after imports", () => {
    expect(() => convert('import "../core/json.ts";\nexport const A = 1;\n')).toThrow(
      /side-effect import/,
    );
    // The old regex swallowed everything from a side-effect import to the next `from "..."`.
    const out = convert(
      'import { isRecord } from "../core/json.ts";\nexport const A = isRecord({});\nconst s = "x from \\"y\\"";\nexport function f() { return s; }\n',
    );
    expect(out.exports).toEqual(["A", "f"]);
    expect(out.code).toContain("const A = isRecord({});");
    expect(out.code).toContain('const s = "x from \\"y\\"";');
    expect(out.code).toContain("function f() { return s; }");
    expect(out.code).not.toMatch(/\bimport\b|\bexport\b/);
  });

  it("refuses export let and var", () => {
    expect(() => convert("export let n = 1;\n")).toThrow(/only export/);
    expect(() => convert("export var n = 1;\n")).toThrow(/only export/);
  });

  it("refuses foreign imports, other exports and blocked APIs", () => {
    expect(() => convert('import x from "node:fs";\n')).toThrow(/isn't in this kit/);
    expect(() => convert('import { y } from "../card/cards.ts";\n')).toThrow(/isn't in this kit/);
    expect(() => convert("export default 1;\n")).toThrow(/only export/);
    expect(() => convert('export * from "./b.ts";\n')).toThrow(/only export/);
    for (const bad of [
      "el.innerHTML = s;",
      "fetch('/x');",
      "localStorage.x = 1;",
      "eval('1');",
      "new Function('x');",
      "navigator.clipboard.readText();",
      "window.__TAURI__;",
      "parent.postMessage(1);",
      "import('./x.ts');",
    ]) {
      expect(() => convert(`${bad}\n`), bad).toThrow(/kits must not use/);
    }
  });
});
