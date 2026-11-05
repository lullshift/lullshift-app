// `create --kit` and the kit copy check in `validate` (ADR-019).
import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import {
  KIT_NAMES,
  addKitScript,
  checkCreateOptions,
  createActivity,
  kitFile,
} from "../src/create.ts";
import { KITS_DIR } from "../src/paths.ts";
import { validateFolder } from "../src/validate.ts";
import { tempDir } from "./helpers.ts";

function withKit(kit: (typeof KIT_NAMES)[number]) {
  const dir = path.join(tempDir(), "game");
  const files = createActivity({
    dir,
    kind: "app",
    name: "Kit Game",
    id: "dev.test.kit-game",
    author: "",
    engine: "",
    kit,
  });
  return { dir, files };
}

describe("create --kit", () => {
  it.each(KIT_NAMES)("copies the %s kit and loads it after the bridge", (kit) => {
    const { dir, files } = withKit(kit);
    expect(files).toContain(kitFile(kit));
    expect(
      readFileSync(path.join(dir, kitFile(kit))).equals(
        readFileSync(path.join(KITS_DIR, kitFile(kit))),
      ),
    ).toBe(true);
    const html = readFileSync(path.join(dir, "index.html"), "utf8");
    const scripts = [...html.matchAll(/<script src="([^"]+)"><\/script>/g)].map((m) => m[1]);
    expect(scripts).toEqual(["lullshift-bridge-v1.js", kitFile(kit), "main.js"]);
    const r = validateFolder(dir);
    expect(r.errors).toEqual([]);
    expect(r.warnings.join("\n")).not.toMatch(/kit/);
  });

  it("refuses unknown kits and kits on content packs", () => {
    const base = {
      dir: "x",
      kind: "app" as const,
      name: "Ok",
      id: "dev.a.b",
      author: "",
      engine: "",
    };
    expect(checkCreateOptions({ ...base, kit: "chess" as never }).join()).toMatch(/--kit chess/);
    expect(
      checkCreateOptions({
        ...base,
        kind: "content",
        engine: "lullshift.cards@1",
        kit: "grid",
      }).join(),
    ).toMatch(/content packs carry no code/);
    expect(checkCreateOptions({ ...base, kit: "card" })).toEqual([]);
  });

  it("keeps the bridge line's indentation", () => {
    const html = '<body>\n    <script src="lullshift-bridge-v1.js"></script>\n</body>';
    expect(addKitScript(html, "arcade")).toBe(
      '<body>\n    <script src="lullshift-bridge-v1.js"></script>\n    <script src="lullshift-arcade-kit-v1.js"></script>\n</body>',
    );
    expect(() => addKitScript("<body></body>", "grid")).toThrow(/doesn't load/);
  });
});

describe("validate kit copies", () => {
  it("warns when a kit copy differs from the SDK's", () => {
    const { dir } = withKit("grid");
    writeFileSync(path.join(dir, kitFile("grid")), "// edited\n");
    const r = validateFolder(dir);
    expect(r.errors).toEqual([]);
    expect(r.warnings.join("\n")).toMatch(/lullshift-grid-kit-v1\.js differs from the SDK's copy/);
  });

  it("warns about a kit name the SDK doesn't have", () => {
    const { dir } = withKit("card");
    writeFileSync(path.join(dir, "lullshift-chess-kit-v1.js"), "// mine\n");
    expect(validateFolder(dir).warnings.join("\n")).toMatch(
      /lullshift-chess-kit-v1\.js isn't a Lullshift kit/,
    );
  });
});
