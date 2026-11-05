#!/usr/bin/env node
// Lullshift local check command (ADR-004, ADR-005). Runs every quality step, reports each
// result with the OS, and exits non-zero if any step fails.
//   pnpm check          full run (pre-push, sprint closeout)
//   pnpm check --fast   skips Rust and the desktop build (pre-commit)
import { spawnSync } from "node:child_process";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const fast = process.argv.includes("--fast");

const steps = [
  { name: "secret scan", cmd: "pnpm", args: ["secrets"] },
  { name: "schema fixtures", cmd: "pnpm", args: ["validate:schemas"] },
  {
    name: "generated types up to date",
    cmd: "pnpm",
    args: ["--filter", "@lullshift/agent-protocol", "gen:types:check"],
  },
  { name: "activity kits up to date", cmd: "pnpm", args: ["kits:check"] },
  { name: "typecheck", cmd: "pnpm", args: ["typecheck"] },
  { name: "lint", cmd: "pnpm", args: ["lint"] },
  { name: "format", cmd: "pnpm", args: ["format:check"] },
  { name: "unit tests (ts)", cmd: "pnpm", args: ["test"] },
  {
    name: "bundled activities (validate, simulate)",
    cmd: "node",
    args: ["tests/activities/check-bundled.mjs"],
    slow: true,
  },
  { name: "cargo fmt", cmd: "cargo", args: ["fmt", "--all", "--check"], slow: false },
  {
    name: "cargo clippy",
    cmd: "cargo",
    args: ["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
    slow: true,
  },
  { name: "cargo test", cmd: "cargo", args: ["test", "--workspace"], slow: true },
  { name: "desktop build", cmd: "pnpm", args: ["desktop:build"], slow: true },
];

const results = [];
for (const step of steps) {
  if (fast && step.slow) {
    results.push({ ...step, status: "skipped" });
    continue;
  }
  console.log(`\n=== ${step.name}: ${step.cmd} ${step.args.join(" ")}`);
  const started = Date.now();
  // shell:true lets Windows resolve pnpm.cmd/cargo.exe; arguments are fixed strings above.
  const r = spawnSync(step.cmd, step.args, { cwd: root, stdio: "inherit", shell: true });
  const code = r.error ? `error: ${r.error.message}` : r.status;
  results.push({ ...step, status: code === 0 ? "pass" : "FAIL", code, ms: Date.now() - started });
}

const osLine = `${os.type()} ${os.release()} (${process.platform}/${process.arch}), node ${process.version}`;
console.log(`\n=== Summary (${fast ? "fast" : "full"}) on ${osLine}`);
for (const r of results) {
  const time = r.ms === undefined ? "" : ` ${(r.ms / 1000).toFixed(1)}s`;
  const code = r.status === "FAIL" ? ` (exit ${r.code})` : "";
  console.log(`${r.status.padEnd(7)} ${r.name}${code}${time}`);
}
const failed = results.filter((r) => r.status === "FAIL").length;
console.log(failed ? `\n${failed} step(s) failed` : "\nall steps passed");
process.exit(failed ? 1 : 0);
