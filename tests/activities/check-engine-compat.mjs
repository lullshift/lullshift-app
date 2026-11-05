#!/usr/bin/env node
// Engine data-schema compatibility (S07-T02, S07-A02, S07-A03, ADR-016). For every engine in
// activities/engines/*/engine.json:
//   - the data schema is strict (objects close additionalProperties, strings have maxLength,
//     arrays have maxItems) and uses only keywords the app's own check understands;
//   - tests/fixtures/engines/<id>@<major>/valid/*.json pass and invalid/*.json fail;
// and every bundled content pack names a bundled engine and its data (at most 256 KiB) passes
// that engine's schema. The shared parity corpus (tests/fixtures/schema-parity/cases.json) is
// run through Ajv here and through the app's validator in `cargo test`. Problems are printed in
// plain words: file, data path, message. Exit 1 on any problem.
//   node tests/activities/check-engine-compat.mjs
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createAjv, friendlyErrors } from "../../packages/agent-protocol/src/index.ts";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const rel = (p) => path.relative(root, p).replaceAll("\\", "/");
const DATA_MAX = 256 * 1024;

// Keep in step with ASSERTIONS and ANNOTATIONS in apps/desktop/src-tauri/src/data_schema.rs.
const HOST_KEYWORDS = new Set([
  "type", "enum", "const", "properties", "required", "additionalProperties", "minProperties",
  "maxProperties", "items", "minItems", "maxItems", "uniqueItems", "minLength", "maxLength",
  "pattern", "minimum", "maximum", "exclusiveMinimum", "exclusiveMaximum", "multipleOf", "allOf",
  "anyOf", "oneOf", "not", "if", "then", "else", "$ref", "$defs",
  "$schema", "$id", "$comment", "title", "description", "default", "examples", "deprecated",
  "readOnly", "writeOnly",
]); // prettier-ignore

// Everything printed may come from untrusted packs (file names, manifest fields, parser
// messages), so control, bidirectional and other invisible characters never reach the terminal.
const HIDDEN =
  // eslint-disable-next-line no-control-regex, no-misleading-character-class
  /[\x00-\x1F\x7F-\x9F\xAD\u{34F}\u{61C}\u{115F}\u{1160}\u{17B4}\u{17B5}\u{180B}-\u{180F}\u{200B}-\u{200F}\u{2028}-\u{202E}\u{2060}-\u{206F}\u{3164}\u{FE00}-\u{FE0F}\u{FEFF}\u{FFA0}\u{FFF9}-\u{FFFB}\p{Cc}\p{Cf}\p{Co}\p{Cn}\p{Cs}\p{Zl}\p{Zp}]/gu; // prettier-ignore
const clean = (text, max = 300) => [...String(text)].slice(0, max).join("").replace(HIDDEN, "?");

const problems = [];
const report = (where, message) => problems.push(`${clean(where)}: ${clean(message)}`);
const readJson = (file) => JSON.parse(readFileSync(file, "utf8"));
const subdirs = (dir) =>
  existsSync(dir)
    ? readdirSync(dir)
        .map((n) => path.join(dir, n))
        .filter((d) => statSync(d).isDirectory())
    : [];
const jsonFiles = (dir) =>
  existsSync(dir)
    ? readdirSync(dir)
        .filter((n) => n.endsWith(".json"))
        .sort()
        .map((n) => path.join(dir, n))
    : [];

/**
 * Strictness rules from ADR-016 and the host's keyword subset, walked through the schema. The
 * rules apply where a value's shape is defined (the top, properties, items, $defs); branches of
 * allOf/anyOf/oneOf/not/if/then/else only add conditions to a shape that is already closed.
 */
function lintSchema(node, at, file, isRoot = true, shape = true) {
  if (typeof node === "boolean") return;
  if (!node || typeof node !== "object" || Array.isArray(node)) {
    report(file, `${at} isn't a schema`);
    return;
  }
  for (const k of Object.keys(node)) {
    if (!HOST_KEYWORDS.has(k)) report(file, `${at} uses "${k}", which the app's check doesn't support`);
    if ((k === "$schema" || k === "$id") && !isRoot) report(file, `${at} sets ${k} below the top`);
  }
  const types = shape ? [node.type].flat() : [];
  if (types.includes("object") && node.additionalProperties !== false)
    report(file, `${at} is an object without "additionalProperties": false`);
  if (types.includes("string") && node.maxLength === undefined && node.enum === undefined && node.const === undefined)
    report(file, `${at} is a string without maxLength`);
  if (types.includes("array") && node.maxItems === undefined)
    report(file, `${at} is a list without maxItems`);
  for (const key of ["properties", "$defs"]) {
    for (const [name, sub] of Object.entries(node[key] ?? {})) lintSchema(sub, `${at}/${key}/${name}`, file, false);
  }
  for (const key of ["items", "additionalProperties"]) {
    if (node[key] !== undefined) lintSchema(node[key], `${at}/${key}`, file, false);
  }
  for (const key of ["not", "if", "then", "else"]) {
    if (node[key] !== undefined) lintSchema(node[key], `${at}/${key}`, file, false, false);
  }
  for (const key of ["allOf", "anyOf", "oneOf"]) {
    (node[key] ?? []).forEach((sub, i) => lintSchema(sub, `${at}/${key}/${i}`, file, false, false));
  }
} // prettier-ignore

// --- Engines and their fixtures.
const engines = new Map();
let fixtureCount = 0;
for (const dir of subdirs(path.join(root, "activities/engines"))) {
  const engineFile = path.join(dir, "engine.json");
  if (!existsSync(engineFile)) continue;
  const where = rel(engineFile);
  let e;
  try {
    e = readJson(engineFile);
  } catch (err) {
    report(where, `isn't valid JSON (${err.message})`);
    continue;
  }
  const ref = `${e.id}@${e.major}`;
  if (
    !/^[a-z0-9]+(\.[a-z0-9-]+)+$/.test(String(e.id)) ||
    !Number.isInteger(e.major) ||
    e.major < 1 ||
    !/^[A-Za-z0-9_][A-Za-z0-9_.-]*\.json$/.test(String(e.dataSchema))
  ) {
    report(where, "needs an id, a positive whole major and a data schema file beside it");
    continue;
  }
  if (engines.has(ref)) {
    report(where, `${ref} is also provided by ${rel(engines.get(ref).dir)}`);
    continue;
  }
  const schemaFile = path.join(dir, e.dataSchema);
  let validate;
  try {
    const schema = readJson(schemaFile);
    lintSchema(schema, "#", rel(schemaFile));
    validate = createAjv({ allErrors: true }).compile(schema);
  } catch (err) {
    report(rel(schemaFile), `can't be used: ${err.message}`);
    continue;
  }
  engines.set(ref, { dir, validate });

  const fixtures = path.join(root, "tests/fixtures/engines", ref);
  const valid = jsonFiles(path.join(fixtures, "valid"));
  const invalid = jsonFiles(path.join(fixtures, "invalid"));
  if (!valid.length || !invalid.length) {
    report(rel(fixtures), `${ref} needs at least one valid/*.json and one invalid/*.json fixture`);
  }
  for (const file of valid) {
    fixtureCount += 1;
    let data;
    try {
      data = readJson(file);
    } catch (err) {
      report(rel(file), `isn't valid JSON (${err.message})`);
      continue;
    }
    if (!validate(data)) {
      for (const line of friendlyErrors(validate.errors))
        report(rel(file), `should pass but ${line}`);
    }
  }
  for (const file of invalid) {
    fixtureCount += 1;
    let data;
    try {
      data = readJson(file);
    } catch {
      continue; // Not JSON at all: certainly invalid.
    }
    if (validate(data)) report(rel(file), "should fail validation but passes");
  }
}

// --- Bundled packs against their engines.
let packCount = 0;
for (const engineDir of subdirs(path.join(root, "activities/content"))) {
  for (const dir of subdirs(engineDir)) {
    const manifestFile = path.join(dir, "manifest.json");
    if (!existsSync(manifestFile)) continue;
    let m;
    try {
      m = readJson(manifestFile);
    } catch (err) {
      report(rel(manifestFile), `isn't valid JSON (${err.message})`);
      continue;
    }
    if (m?.kind !== "content") continue;
    packCount += 1;
    const where = rel(dir);
    const engine = engines.get(m.engine);
    if (!engine) {
      report(where, `engine ${m.engine} isn't a bundled engine`);
      continue;
    }
    const dataFile = path.join(dir, String(m.data));
    if (!existsSync(dataFile)) {
      report(where, `data ${m.data} is missing`);
      continue;
    }
    if (statSync(dataFile).size > DATA_MAX) {
      report(where, `${m.data} is over 256 KiB`);
      continue;
    }
    let data;
    try {
      data = readJson(dataFile);
    } catch (err) {
      report(where, `${m.data} isn't valid JSON (${err.message})`);
      continue;
    }
    if (!engine.validate(data)) {
      for (const line of friendlyErrors(engine.validate.errors, m.data)) report(where, line);
    }
  }
}

// --- Shared parity corpus: the same cases run through the app's validator in `cargo test`.
const corpusFile = path.join(root, "tests/fixtures/schema-parity/cases.json");
let corpusCount = 0;
try {
  const cases = readJson(corpusFile);
  if (!Array.isArray(cases) || cases.length < 20) report(rel(corpusFile), "has too few cases");
  for (const [i, c] of (Array.isArray(cases) ? cases : []).entries()) {
    corpusCount += 1;
    const name = `case ${i} (${c?.name ?? "unnamed"})`;
    try {
      const got = Boolean(createAjv({ allErrors: true }).compile(c.schema)(c.data));
      if (got !== c.valid)
        report(rel(corpusFile), `${name}: Ajv says ${got}, the corpus says ${c.valid}`);
    } catch (err) {
      report(rel(corpusFile), `${name}: can't be checked (${err.message})`);
    }
  }
} catch (err) {
  report(rel(corpusFile), `can't be read (${err.message})`);
}

console.log(
  `=== engine compatibility: ${engines.size} engine(s), ${fixtureCount} fixture(s), ${packCount} pack(s), ${corpusCount} parity case(s)`,
);
for (const p of problems) console.error(`  error   ${p}`);
console.log(
  problems.length ? `✗ ${problems.length} problem(s)` : "✓ all engines, fixtures and packs agree",
);
process.exit(problems.length ? 1 : 0);
