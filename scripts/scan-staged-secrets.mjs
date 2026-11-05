#!/usr/bin/env node
// Secret-scans the *staged* content (the index), not the working tree, so a secret that was
// staged and then removed from disk is still caught. Used by the opt-in pre-commit hook.
import { execFileSync, spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const git = (args, opts = {}) => execFileSync("git", args, { cwd: root, ...opts });

const staged = git(["diff", "--cached", "--name-only", "-z", "--diff-filter=ACMR"])
  .toString("utf8")
  .split("\0")
  .filter(Boolean);

if (staged.length === 0) {
  console.log("secret scan (staged): nothing staged");
  process.exit(0);
}

const tmp = mkdtempSync(path.join(os.tmpdir(), "lullshift-staged-"));
let code;
try {
  for (const file of staged) {
    const target = path.join(tmp, file);
    mkdirSync(path.dirname(target), { recursive: true });
    writeFileSync(target, git(["show", `:${file}`], { maxBuffer: 64 * 1024 * 1024 }));
  }
  const bin = path.join(
    root,
    "node_modules",
    ".bin",
    process.platform === "win32" ? "secretlint.cmd" : "secretlint",
  );
  const r = spawnSync(
    bin,
    [
      "--secretlintrc",
      path.join(root, ".secretlintrc.json"),
      "--secretlintignore",
      path.join(root, ".secretlintignore"),
      "**/*",
    ],
    { cwd: tmp, stdio: "inherit", shell: process.platform === "win32" },
  );
  code = r.status ?? 1;
  console.log(
    `secret scan (staged): ${staged.length} file(s), ${code === 0 ? "clean" : "SECRETS FOUND"}`,
  );
} finally {
  rmSync(tmp, { recursive: true, force: true });
}
process.exit(code ?? 1);
