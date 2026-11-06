// Decision 29 review (LOW 6): only the repository layout honours INIT_CWD.
import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { BUILT, startDir } from "../src/paths.ts";

describe("startDir", () => {
  it("honours INIT_CWD when running the sources from the repository", () => {
    expect(startDir({ INIT_CWD: "/somewhere/typed" }, false)).toBe("/somewhere/typed");
    expect(startDir({}, false)).toBeUndefined();
  });

  it("ignores INIT_CWD in the installed (dist/) layout", () => {
    expect(startDir({ INIT_CWD: "/somewhere/else" }, true)).toBeUndefined();
    expect(startDir({}, true)).toBeUndefined();
  });

  it("defaults to the layout it runs from (the sources here)", () => {
    expect(BUILT).toBe(false);
    expect(startDir({ INIT_CWD: "/x" })).toBe("/x");
  });

  it("is the only way main.ts changes folder", () => {
    const main = readFileSync(path.join(import.meta.dirname, "..", "src", "main.ts"), "utf8");
    expect(main).not.toMatch(/chdir\(process\.env/);
    expect(main.match(/process\.chdir\(/g)).toHaveLength(1);
    expect(main).toContain("startDir(process.env)");
  });
});
