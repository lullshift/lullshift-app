// Bundled first-party activities: game logic tests plus static sandbox/manifest hygiene checks.
/// <reference types="node" />
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { createAjv } from "@lullshift/agent-protocol";
import { describe, expect, it } from "vitest";
import "../../../../activities/games/lantern-pond/logic.js";

interface Logic {
  SIZE: number;
  CELLS: number;
  generate(seed: number): { board: boolean[]; presses: number[] };
  press(board: boolean[], index: number): boolean[];
  isSolved(board: boolean[]): boolean;
  solvedBoard(): boolean[];
  pushMove(stack: number[], index: number): number[];
  undo(board: boolean[], stack: number[]): { board: boolean[]; stack: number[] };
  dateSeed(d: Date): number;
}
const L = (globalThis as unknown as { LanternLogic: Logic }).LanternLogic;

describe("LanternLogic", () => {
  it("generates deterministically from a seed", () => {
    expect(L.generate(20260101)).toEqual(L.generate(20260101));
    expect(L.generate(1).board).not.toEqual(L.generate(2).board);
  });

  it("never generates a solved board and is always solvable", () => {
    for (let seed = 1; seed <= 300; seed++) {
      const { board, presses } = L.generate(seed);
      expect(board).toHaveLength(25);
      expect(presses.length).toBeGreaterThanOrEqual(6);
      expect(presses.length).toBeLessThanOrEqual(10);
      expect(L.isSolved(board)).toBe(false);
      let b = board;
      for (const p of presses) b = L.press(b, p);
      expect(L.isSolved(b)).toBe(true);
    }
  });

  const flipped = (before: boolean[], after: boolean[]) =>
    after.flatMap((v, i) => (v !== before[i] ? [i] : []));

  it("press flips the cell and its in-bounds neighbours", () => {
    const b = L.solvedBoard();
    expect(flipped(b, L.press(b, 12))).toEqual([7, 11, 12, 13, 17]);
    expect(flipped(b, L.press(b, 0))).toEqual([0, 1, 5]);
    expect(flipped(b, L.press(b, 4))).toEqual([3, 4, 9]);
    expect(flipped(b, L.press(b, 24))).toEqual([19, 23, 24]);
    expect(flipped(b, L.press(b, 2))).toEqual([1, 2, 3, 7]);
    expect(flipped(b, L.press(b, 10))).toEqual([5, 10, 11, 15]);
  });

  it("press does not mutate its input", () => {
    const b = L.solvedBoard();
    L.press(b, 6);
    expect(L.isSolved(b)).toBe(true);
  });

  it("undo reverses the last move", () => {
    const start = L.generate(7).board;
    let board = start;
    let stack: number[] = [];
    for (const i of [3, 12, 20]) {
      board = L.press(board, i);
      stack = L.pushMove(stack, i);
    }
    let r = L.undo(board, stack);
    expect(r.stack).toEqual([3, 12]);
    expect(r.board).toEqual(L.press(L.press(start, 3), 12));
    r = L.undo(r.board, r.stack);
    r = L.undo(r.board, r.stack);
    expect(r.board).toEqual(start);
    expect(L.undo(r.board, r.stack)).toEqual({ board: start, stack: [] });
  });

  it("dateSeed is YYYYMMDD in local time", () => {
    expect(L.dateSeed(new Date(2026, 10, 5))).toBe(20261105);
  });
});

const root = new URL("../../../../activities/", import.meta.url);
const rootPath = (p: string) => join(fileURLToPath(root), p);
const bridgePath = fileURLToPath(
  new URL("../../../../packages/activity-sdk/bridge/lullshift-bridge-v1.js", import.meta.url),
);

function activityDirs(): string[] {
  const dirs: string[] = [];
  for (const group of ["break", "games", "engines"]) {
    const g = rootPath(group);
    if (!existsSync(g)) continue;
    for (const name of readdirSync(g)) {
      const dir = join(g, name);
      if (statSync(dir).isDirectory() && existsSync(join(dir, "manifest.json"))) dirs.push(dir);
    }
  }
  return dirs;
}

const ALLOWED_EXT = new Set(["html", "js", "css", "json", "svg", "png", "jpg", "webp"]);
const FORBIDDEN = [
  'type="module"',
  "eval(",
  "new Function",
  "localStorage",
  "fetch(",
  "XMLHttpRequest",
  "WebSocket",
];

describe("bundled activities", () => {
  const dirs = activityDirs();

  it("finds the first-party activities", () => {
    expect(dirs.length).toBeGreaterThanOrEqual(2);
  });

  for (const dir of dirs) {
    describe(dir.split(/[\\/]/).slice(-2).join("/"), () => {
      const files = readdirSync(dir);
      const manifest = JSON.parse(readFileSync(join(dir, "manifest.json"), "utf8"));

      it("carries an identical copy of the bridge client", () => {
        expect(
          readFileSync(join(dir, "lullshift-bridge-v1.js")).equals(readFileSync(bridgePath)),
        ).toBe(true);
      });

      it("has a valid manifest", () => {
        expect(manifest.schemaVersion).toBe(1);
        expect(manifest.kind).toBe("app");
        expect(manifest.entrypoint).toBe("index.html");
        expect(manifest.runtime).toBe("isolated-web-v1");
        expect(manifest.interruptibility).toEqual({
          pause: true,
          resume: true,
          persistState: true,
        });
        expect(manifest.id).toMatch(/^[a-z0-9]+(\.[a-z0-9-]+)+$/);
        expect(manifest.offline).toBe(true);
        expect(manifest.description.length).toBeLessThanOrEqual(500);
        expect(manifest.description).not.toContain("!");
        for (const p of manifest.permissions) expect(p).toBe("activity.storage");
        expect(files).toContain("index.html");
      });

      it("uses only allowed file types and names", () => {
        for (const f of files) {
          expect(f).toMatch(/^[A-Za-z0-9][A-Za-z0-9._-]*$/);
          expect(ALLOWED_EXT.has(f.split(".").pop() ?? "")).toBe(true);
        }
      });

      it("is CSP-clean and free of forbidden APIs", () => {
        for (const f of files) {
          const text = readFileSync(join(dir, f), "utf8");
          if (f.endsWith(".html")) {
            expect(text, `${f} inline script`).not.toMatch(/<script(?![^>]*\bsrc=)[^>]*>/i);
            expect(text, `${f} style block`).not.toMatch(/<style/i);
            expect(text, `${f} style attr`).not.toMatch(/\sstyle=/i);
          }
          for (const bad of FORBIDDEN) expect(text, `${f} contains ${bad}`).not.toContain(bad);
        }
      });
    });
  }
});

// Content packs (ADR-016): code-free data for a named engine.
function readJson(path: string): Record<string, unknown> {
  return JSON.parse(readFileSync(path, "utf8")) as Record<string, unknown>;
}

function packDirs(): string[] {
  const dirs: string[] = [];
  const base = rootPath("content");
  if (!existsSync(base)) return dirs;
  const walk = (dir: string) => {
    if (existsSync(join(dir, "manifest.json"))) return void dirs.push(dir);
    for (const name of readdirSync(dir)) {
      const sub = join(dir, name);
      if (statSync(sub).isDirectory()) walk(sub);
    }
  };
  walk(base);
  return dirs;
}

describe("content packs", () => {
  const ajv = createAjv({ allErrors: true });
  const validateManifest = ajv.compile(
    readJson(
      fileURLToPath(
        new URL("../../../../reference/ACTIVITY_MANIFEST_V1.schema.json", import.meta.url),
      ),
    ),
  );
  const engines = new Map<string, { dir: string; major: number; schema: string }>();
  const engineRoot = rootPath("engines");
  for (const name of existsSync(engineRoot) ? readdirSync(engineRoot) : []) {
    const dir = join(engineRoot, name);
    if (!existsSync(join(dir, "engine.json"))) continue;
    const e = readJson(join(dir, "engine.json")) as {
      id: string;
      major: number;
      dataSchema: string;
    };
    engines.set(`${e.id}@${e.major}`, { dir, major: e.major, schema: e.dataSchema });
  }
  const dirs = packDirs();

  it("finds the sample pack", () => {
    expect(dirs.length).toBeGreaterThanOrEqual(1);
  });

  it("validates engine manifests against the schema", () => {
    for (const { dir } of engines.values()) {
      const m = readJson(join(dir, "manifest.json"));
      expect(validateManifest(m), JSON.stringify(validateManifest.errors)).toBe(true);
    }
  });

  for (const dir of dirs) {
    describe(dir.split(/[\\/]/).slice(-3).join("/"), () => {
      const manifest = readJson(join(dir, "manifest.json")) as {
        kind: string;
        engine: string;
        data: string;
      };

      it("has a valid content manifest", () => {
        expect(validateManifest(manifest), JSON.stringify(validateManifest.errors)).toBe(true);
        expect(manifest.kind).toBe("content");
      });

      it("names an existing engine and its data matches the engine schema", () => {
        const engine = engines.get(manifest.engine);
        expect(engine, `engine ${manifest.engine}`).toBeDefined();
        if (!engine) return;
        const validateData = createAjv({ allErrors: true }).compile(
          readJson(join(engine.dir, engine.schema)),
        );
        const data = readJson(join(dir, manifest.data));
        expect(validateData(data), JSON.stringify(validateData.errors)).toBe(true);
        expect(statSync(join(dir, manifest.data)).size).toBeLessThanOrEqual(256 * 1024);
      });

      it("holds no code", () => {
        for (const f of readdirSync(dir)) expect(f).not.toMatch(/\.(html|js|mjs|css)$/i);
      });
    });
  }
});
