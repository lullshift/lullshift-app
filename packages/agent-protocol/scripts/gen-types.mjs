// Generates src/generated/*.ts from the canonical schemas (ADR-006): one type module per schema,
// and schemas.ts with the schemas themselves.
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
/** Writes `target`, or with --check reports it as stale when it differs. */
async function emit(target, text) {
  if (check) {
    const current = await readFile(target, "utf8").catch(() => "");
    if (current !== text) {
      console.error(`stale: ${path.relative(repoRoot, target)} (run pnpm gen:types)`);
      stale++;
    }
  } else {
    await writeFile(target, text);
    console.log(`wrote ${path.relative(repoRoot, target)}`);
  }
}

// The schemas themselves, as a module (decision 29): the published package can't reach
// /reference, and plain JavaScript loads on every Node version without JSON import attributes.
const schemaLines = [
  "/* Generated from reference/*.schema.json by scripts/gen-types.mjs. Do not edit; run `pnpm gen:types`. */",
];
for (const s of schemas) {
  const schema = JSON.parse(await readFile(path.join(repoRoot, s.file), "utf8"));
  schemaLines.push(
    "",
    `/** ${s.file} */`,
    `export const ${s.exportName}: object = ${JSON.stringify(schema)};`,
  );
}
const schemasTarget = path.join(outDir, "schemas.ts");
await emit(
  schemasTarget,
  await format(schemaLines.join("\n"), {
    ...(await resolveConfig(schemasTarget)),
    filepath: schemasTarget,
  }),
);

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
  await emit(target, ts);
}
if (stale) process.exit(1);
if (check) console.log("generated types are up to date");
