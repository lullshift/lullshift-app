#!/usr/bin/env node
// Builds one npm package into its dist/ folder (owner decision 29, ADR-017 amendment). Run from the
// package folder, as its `build` and `prepack` scripts do:
//   @lullshift/agent-protocol, @lullshift/activity-sdk: JavaScript and .d.ts from src/.
//   @lullshift/cli: JavaScript from src/, plus what the installed CLI reads at run time and can't
//   find outside its own package: the harness page, the kit scripts and the bundled engines.
// Inside the repository nothing uses dist/: the workspace runs the TypeScript sources.
import { spawnSync } from "node:child_process";
import {
  copyFileSync,
  lstatSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { stripTypeScriptTypes } from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const pkgDir = process.cwd();
const pkg = JSON.parse(readFileSync(path.join(pkgDir, "package.json"), "utf8"));
const dist = path.join(pkgDir, "dist");

const PACKAGES = ["@lullshift/agent-protocol", "@lullshift/activity-sdk", "@lullshift/cli"];
if (!PACKAGES.includes(pkg.name)) {
  console.error(
    `npm-build: ${pkg.name} isn't one of the published packages (${PACKAGES.join(", ")})`,
  );
  process.exit(2);
}

rmSync(dist, { recursive: true, force: true });
const tsc = path.join(pkgDir, "node_modules", "typescript", "bin", "tsc");
const r = spawnSync(process.execPath, [tsc, "-p", "tsconfig.build.json"], {
  cwd: pkgDir,
  stdio: "inherit",
});
if (r.status !== 0) {
  console.error(`npm-build: tsc failed for ${pkg.name}`);
  process.exit(1);
}

// tsc rewrites `./x.ts` to `./x.js` in JavaScript but leaves declarations as written; point them
// at the emitted .d.ts files so TypeScript users resolve them in any module mode.
for (const file of listFiles(dist).filter((f) => f.endsWith(".d.ts"))) {
  const text = readFileSync(file, "utf8");
  const fixed = text.replace(
    /((?:from|import)\s*\(?\s*)(["'])(\.\.?\/[^"']+)\.ts\2/g,
    (_, lead, q, spec) => `${lead}${q}${spec}.js${q}`,
  );
  if (fixed !== text) writeFileSync(file, fixed);
}

// Apache-2.0 asks for a copy of the licence with every distribution (gitignored copy).
copyFileSync(path.join(root, "LICENSE"), path.join(pkgDir, "LICENSE"));

if (pkg.name === "@lullshift/cli") {
  // The harness page: host.ts is stripped to plain JavaScript once here, not on every request.
  const harness = path.join(pkgDir, "harness");
  const out = path.join(dist, "harness");
  mkdirSync(out, { recursive: true });
  for (const f of ["index.html", "harness.css"])
    copyFileSync(path.join(harness, f), path.join(out, f));
  const warn = process.emitWarning;
  process.emitWarning = () => {}; // stripTypeScriptTypes is flagged experimental; output is tested
  try {
    const ts = readFileSync(path.join(harness, "host.ts"), "utf8");
    writeFileSync(path.join(out, "host.js"), stripTypeScriptTypes(ts, { mode: "strip" }));
  } finally {
    process.emitWarning = warn;
  }

  // Kits (ADR-019): the generated classic scripts that `create --kit` copies.
  const kits = path.join(root, "packages", "activity-kits", "dist");
  const kitsOut = path.join(dist, "assets", "kits");
  mkdirSync(kitsOut, { recursive: true });
  for (const f of readdirSync(kits).filter((n) => /^lullshift-[a-z]+-kit-v1\.js$/.test(n))) {
    copyFileSync(path.join(kits, f), path.join(kitsOut, f));
  }

  // Bundled engines (ADR-016): validate checks packs against them, simulate runs them.
  copyTree(path.join(root, "activities", "engines"), path.join(dist, "assets", "engines"));
}

console.log(`npm-build: built ${pkg.name}@${pkg.version} into dist/`);

function listFiles(dir) {
  return readdirSync(dir, { recursive: true, withFileTypes: true })
    .filter((e) => e.isFile())
    .map((e) => path.join(e.parentPath, e.name));
}

/** Copies plain files and folders only: no links, no hidden files (they are never served). */
function copyTree(from, to) {
  mkdirSync(to, { recursive: true });
  for (const name of readdirSync(from).sort()) {
    if (name.startsWith(".")) continue;
    const src = path.join(from, name);
    const st = lstatSync(src);
    if (st.isSymbolicLink()) throw new Error(`npm-build: refusing to copy the link ${src}`);
    if (st.isDirectory()) copyTree(src, path.join(to, name));
    else if (st.isFile()) copyFileSync(src, path.join(to, name));
  }
}
