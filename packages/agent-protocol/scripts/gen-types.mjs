// Generates src/generated/*.ts from the canonical schemas (ADR-006).
// --check: exit 1 if the committed files are out of date instead of writing them.
import { readFile, writeFile, mkdir } from "node:fs/promises";
import path from "node:path";
import { compile } from "json-schema-to-typescript";
import { format, resolveConfig } from "prettier";
import { repoRoot, schemas } from "./schemas.mjs";

const check = process.argv.includes("--check");
const outDir = path.join(repoRoot, "packages", "agent-protocol", "src", "generated");
await mkdir(outDir, { recursive: true });

let stale = 0;
for (const s of schemas) {
  const schema = JSON.parse(await readFile(path.join(repoRoot, s.file), "utf8"));
  schema.title = s.typeName;
  const banner = `/* Generated from ${s.file} by scripts/gen-types.mjs. Do not edit; run \`pnpm gen:types\`. */`;
  const raw = await compile(schema, s.typeName, {
    bannerComment: banner,
    additionalProperties: false,
    strictIndexSignatures: true,
    ignoreMinAndMaxItems: true,
    format: false,
  });
  const target = path.join(outDir, `${s.name}.ts`);
  const ts = await format(raw, { ...(await resolveConfig(target)), filepath: target });
  if (check) {
    const current = await readFile(target, "utf8").catch(() => "");
    if (current !== ts) {
      console.error(`stale: ${path.relative(repoRoot, target)} (run pnpm gen:types)`);
      stale++;
    }
  } else {
    await writeFile(target, ts);
    console.log(`wrote ${path.relative(repoRoot, target)}`);
  }
}
if (stale) process.exit(1);
if (check) console.log("generated types are up to date");
