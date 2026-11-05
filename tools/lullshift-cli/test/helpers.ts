import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { afterAll } from "vitest";
import { createActivity, type Kind } from "../src/create.ts";

const made: string[] = [];
afterAll(() => {
  for (const d of made) rmSync(d, { recursive: true, force: true });
});

export function tempDir(): string {
  const d = mkdtempSync(path.join(tmpdir(), "lullshift-cli-test-"));
  made.push(d);
  return d;
}

/** A fresh activity from a template, in a temp folder. */
export function scaffold(kind: Kind = "app", name = "Test Pebbles"): string {
  const dir = path.join(tempDir(), "activity");
  createActivity({
    dir,
    kind,
    name,
    id: "dev.test.pebbles",
    author: "Tester",
    engine: "lullshift.cards@1",
  });
  return dir;
}

export function editJson(file: string, change: (j: Record<string, unknown>) => void) {
  const j = JSON.parse(readFileSync(file, "utf8")) as Record<string, unknown>;
  change(j);
  writeFileSync(file, JSON.stringify(j, null, 2));
}
