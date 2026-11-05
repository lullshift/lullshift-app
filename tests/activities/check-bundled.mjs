#!/usr/bin/env node
// Validates every bundled activity and content pack with the creator CLI, and simulates each one
// (S05-A01, ADR-017). Engines are simulated through their packs. Also checks that the built-in
// marketplace index (.lullshift/marketplace.json, ADR-002) is valid and lists every bundled
// activity exactly once, under its manifest id (S06-A03). Exit 1 on any problem.
//   node tests/activities/check-bundled.mjs [--no-simulate]
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { validateMarketplaceIndex } from "../../packages/agent-protocol/src/index.ts";

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

// The built-in marketplace lists exactly the bundled activities.
{
  const problems = [];
  const indexPath = path.join(root, ".lullshift/marketplace.json");
  const index = JSON.parse(readFileSync(indexPath, "utf8"));
  const checked = validateMarketplaceIndex(index);
  if (!checked.ok) problems.push(`invalid index: ${JSON.stringify(checked.errors ?? checked)}`);
  if (index.name !== "lullshift")
    problems.push('the built-in marketplace must be named "lullshift"');
  const listed = new Map();
  for (const entry of index.activities ?? []) {
    const p = entry.source?.path;
    if (!p) {
      problems.push(`${entry.id}: bundled entries use a path in this repository`);
      continue;
    }
    if (listed.has(p)) problems.push(`${p} is listed twice`);
    listed.set(p, entry.id);
  }
  for (const dir of dirs) {
    const rel = path.relative(root, dir).replaceAll("\\", "/");
    const id = JSON.parse(readFileSync(path.join(dir, "manifest.json"), "utf8")).id;
    if (!listed.has(rel)) problems.push(`${rel} (${id}) is not in the marketplace index`);
    else if (listed.get(rel) !== id)
      problems.push(`${rel}: index id ${listed.get(rel)} != manifest id ${id}`);
    listed.delete(rel);
  }
  for (const [p] of listed) problems.push(`${p} is in the index but has no activity`);
  console.log("=== marketplace index .lullshift/marketplace.json");
  for (const p of problems) console.error(`  error   ${p}`);
  if (problems.length) failed += 1;
  else console.log(`  ok: ${dirs.length} activities listed`);
}
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
