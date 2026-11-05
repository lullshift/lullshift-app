import { existsSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import {
  checkCreateOptions,
  createActivity,
  idFromName,
  initMarketplace,
  nameFromFolder,
} from "../src/create.ts";
import { BRIDGE_PATH } from "../src/paths.ts";
import { activityCsp } from "../src/serve.ts";
import { scaffold, tempDir } from "./helpers.ts";

describe("create", () => {
  it("copies the SDK's bridge and fills the template", () => {
    const dir = scaffold("app", 'Quote "Pebbles"');
    expect(
      readFileSync(path.join(dir, "lullshift-bridge-v1.js")).equals(readFileSync(BRIDGE_PATH)),
    ).toBe(true);
    const m = JSON.parse(readFileSync(path.join(dir, "manifest.json"), "utf8")) as { name: string };
    expect(m.name).toBe('Quote "Pebbles"');
  });

  it("refuses a folder that already has files", () => {
    const dir = tempDir();
    writeFileSync(path.join(dir, "keep.txt"), "mine");
    expect(() =>
      createActivity({ dir, kind: "app", name: "X", id: "dev.x.y", author: "", engine: "" }),
    ).toThrow(/isn't empty/);
    expect(readFileSync(path.join(dir, "keep.txt"), "utf8")).toBe("mine");
  });

  it("derives a name and id, and checks options", () => {
    expect(nameFromFolder("some/path/quiet-counter")).toBe("Quiet Counter");
    expect(idFromName("Quiet Counter!")).toBe("dev.local.quiet-counter");
    const base = {
      dir: "x",
      kind: "content" as const,
      name: "Ok",
      id: "dev.a.b",
      author: "",
      engine: "x",
    };
    expect(checkCreateOptions(base).join()).toMatch(/--engine/);
    expect(
      checkCreateOptions({ ...base, engine: "lullshift.cards@1", id: "Bad Id" }).join(),
    ).toMatch(/--id/);
  });
});

describe("marketplace init", () => {
  it("writes a valid empty index and never replaces one", () => {
    const repo = tempDir();
    const file = initMarketplace(repo, "my-games", "Me");
    expect(JSON.parse(readFileSync(file, "utf8"))).toMatchObject({
      schemaVersion: 1,
      name: "my-games",
    });
    expect(() => initMarketplace(repo, "my-games", "")).toThrow(/already exists/);
  });

  it("refuses bad and reserved names", () => {
    expect(() => initMarketplace(tempDir(), "Bad Name", "")).toThrow(/--name/);
    const repo = tempDir();
    expect(() => initMarketplace(repo, "lullshift", "")).toThrow(/reserved/);
    expect(existsSync(path.join(repo, ".lullshift"))).toBe(false);
  });
});

describe("simulator sandbox", () => {
  it("serves activities with the app's CSP (activities.rs)", () => {
    const rust = readFileSync(
      new URL("../../../apps/desktop/src-tauri/src/activities.rs", import.meta.url),
      "utf8",
    );
    const body =
      /pub fn activity_csp\(\) -> String \{\s*format!\(\s*"([\s\S]*?)",\s*frame_ancestors\(\)/.exec(
        rust,
      );
    expect(body).not.toBeNull();
    const fromRust = body![1]!.replace(/\\\s*\n\s*/g, "").replace("{}", "ANCESTORS");
    expect(activityCsp("ANCESTORS")).toBe(fromRust);
  });
});
