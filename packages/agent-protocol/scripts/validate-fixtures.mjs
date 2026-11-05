// Validates every fixture against its schema (ADR-006):
// tests/fixtures/<schema>/valid/*.json must pass; tests/fixtures/<schema>/invalid/*.json must fail.
// Exits 1 if any fixture behaves unexpectedly or a schema has no valid or no invalid fixtures.
import { readdir, readFile } from "node:fs/promises";
import path from "node:path";
import { Ajv2020 } from "ajv/dist/2020.js";
import addFormatsModule from "ajv-formats";
import { fixturesDir, repoRoot, schemas } from "./schemas.mjs";

const addFormats = addFormatsModule.default ?? addFormatsModule;
const ajv = new Ajv2020({ allErrors: true, strict: true, strictRequired: false });
addFormats(ajv);

const listJson = async (dir) =>
  (await readdir(dir).catch(() => [])).filter((f) => f.endsWith(".json")).sort();

let failures = 0;
for (const s of schemas) {
  const validate = ajv.compile(JSON.parse(await readFile(path.join(repoRoot, s.file), "utf8")));
  for (const expectValid of [true, false]) {
    const dir = path.join(fixturesDir(s.name), expectValid ? "valid" : "invalid");
    const files = await listJson(dir);
    if (files.length === 0) {
      console.error(`FAIL ${s.name}: no ${expectValid ? "valid" : "invalid"} fixtures in ${dir}`);
      failures++;
    }
    for (const f of files) {
      const data = JSON.parse(await readFile(path.join(dir, f), "utf8"));
      const ok = validate(data);
      const rel = path.relative(repoRoot, path.join(dir, f));
      if (ok === expectValid) {
        console.log(`ok   ${rel}`);
      } else {
        failures++;
        const detail = ok ? "unexpectedly valid" : ajv.errorsText(validate.errors);
        console.error(`FAIL ${rel}: ${detail}`);
      }
    }
  }
}
if (failures) {
  console.error(`\n${failures} fixture problem(s)`);
  process.exit(1);
}
console.log("\nall fixtures behave as expected");
