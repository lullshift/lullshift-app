#!/usr/bin/env node
// Validates every bundled activity and content pack with the creator CLI, and simulates each one
// (S05-A01, ADR-017). Engines are simulated through their packs. Exit 1 on any problem.
//   node tests/activities/check-bundled.mjs [--no-simulate]
import { spawnSync } from "node:child_process";
import { existsSync, readdirSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const cli = path.join(root, "tools/lullshift-cli/src/main.ts");
const simulateToo = !process.argv.includes("--no-simulate");

/** Folders with a manifest.json, up to three levels under activities/. */
function activityDirs(dir, depth = 0) {
  if (existsSync(path.join(dir, "manifest.json"))) return [dir];
  if (depth > 3) return [];
  return readdirSync(dir)
    .map((n) => path.join(dir, n))
    .filter((d) => statSync(d).isDirectory())
    .flatMap((d) => activityDirs(d, depth + 1));
}

const dirs = activityDirs(path.join(root, "activities")).sort();
let failed = 0;
for (const dir of dirs) {
  const rel = path.relative(root, dir).replaceAll("\\", "/");
  const engine = existsSync(path.join(dir, "engine.json"));
  const commands = [["validate"]];
  if (simulateToo && !engine) commands.push(["simulate"]);
  for (const [command] of commands) {
    console.log(`\n=== ${command} ${rel}`);
    const r = spawnSync(process.execPath, [cli, command, dir], { stdio: "inherit" });
    if (r.status !== 0) {
      failed += 1;
      console.error(`FAIL ${command} ${rel} (exit ${r.status})`);
    }
  }
}
console.log(
  failed
    ? `\n${failed} problem(s) in ${dirs.length} activities`
    : `\nall ${dirs.length} activities pass`,
);
process.exit(failed ? 1 : 0);
