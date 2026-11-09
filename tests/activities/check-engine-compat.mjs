#!/usr/bin/env node
// Engine data-schema compatibility (S07-T02, S07-A02, S07-A03, ADR-016). For every engine in
// activities/engines/*/engine.json:
//   - the data schema is strict (objects close additionalProperties, strings have maxLength,
//     arrays have maxItems) and uses only keywords the app's own check understands;
//   - tests/fixtures/engines/<id>@<major>/valid/*.json pass and invalid/*.json fail;
// and every bundled content pack names a bundled engine and its data (at most 256 KiB) passes
// that engine's schema (and sets roundSize, if at all, to at most its item count). The shared parity corpus (tests/fixtures/schema-parity/cases.json) is
// run through Ajv here and through the app's validator in `cargo test`. Problems are printed in
// plain words: file, data path, message. Exit 1 on any problem.
//   node tests/activities/check-engine-compat.mjs
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createAjv, dataPath, friendlyErrors } from "../../packages/agent-protocol/src/index.ts";
import { packTextProblems } from "../../packages/activity-sdk/src/host/protocol.ts";
import { roundSizeProblem } from "../../packages/activity-sdk/src/engines.ts";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const rel = (p) => path.relative(root, p).replaceAll("\\", "/");
const DATA_MAX = 256 * 1024;

/**
 * Pack-data problems exactly as the app finds them: the engine schema, then the host's
 * plain-text rule on every string (security review LOW-2; crates/marketplace/src/pack_text.rs).
 */
function dataProblems(validate, data, file) {
  const out = validate(data) ? [] : friendlyErrors(validate.errors, file);
  for (const line of packTextProblems(data, dataPath)) {
    const shown = file ? `${file}: ${line}` : line;
    if (!out.includes(shown)) out.push(shown);
  }
  return out;
}

// The shared plain-text pattern starts like this (friendly.ts and data_schema.rs detect it so).
const PLAIN_PREFIX = "^[^\\u0000-\\u001F";
const plainPatterns = new Map(); // pattern -> first schema file using it

// Keep in step with ASSERTIONS and ANNOTATIONS in crates/marketplace/src/data_schema.rs.
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
    if ((k === "$schema" || k === "$id" || k === "$defs") && !isRoot) report(file, `${at} sets ${k} below the top`);
  }
  if (isRoot && node.$id !== undefined && !String(node.$id).startsWith("urn:"))
    report(file, `${at} $id must be a urn: name`);
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

// Same limit as MAX_UNFOLDED in data_schema.rs: the app refuses larger schemas because its
// validator (the jsonschema crate) has no work budget of its own.
const MAX_UNFOLDED = 2000;

/**
 * The schema's size with every `$ref` replaced by its target (plus enum, const and required
 * entries), as `unfolded_size` in data_schema.rs measures it. Throws on a `$ref` cycle.
 */
function unfoldedSize(root, node, memo = new Map(), visiting = new Set()) {
  if (memo.has(node)) return memo.get(node);
  if (!node || typeof node !== "object" || Array.isArray(node)) return 1;
  if (visiting.has(node)) throw new Error("refers to itself in a loop, which the app doesn't support");
  visiting.add(node);
  const valueSize = (v) =>
    Array.isArray(v) ? 1 + v.reduce((s, x) => s + valueSize(x), 0)
    : v && typeof v === "object" ? 1 + Object.values(v).reduce((s, x) => s + valueSize(x), 0)
    : 1;
  const resolve = (r) =>
    r === "#" ? root : typeof r === "string" && r.startsWith("#/$defs/") ? root.$defs?.[r.slice(8)] : undefined;
  let total = 1;
  for (const [k, v] of Object.entries(node)) {
    if (k === "properties") for (const sub of Object.values(v ?? {})) total += unfoldedSize(root, sub, memo, visiting);
    else if (["additionalProperties", "items", "not", "if", "then", "else"].includes(k)) total += unfoldedSize(root, v, memo, visiting);
    else if (["allOf", "anyOf", "oneOf"].includes(k)) for (const sub of v ?? []) total += unfoldedSize(root, sub, memo, visiting);
    else if (k === "$ref") { const t = resolve(v); if (t !== undefined) total += unfoldedSize(root, t, memo, visiting); }
    else if (k === "enum") total += (v ?? []).reduce((s, x) => s + valueSize(x), 0);
    else if (k === "const") total += valueSize(v);
    else if (k === "required") total += (v ?? []).length;
    if (total > MAX_UNFOLDED) break;
  }
  visiting.delete(node);
  memo.set(node, total);
  return total;
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
    if (unfoldedSize(schema, schema) > MAX_UNFOLDED)
      report(
        rel(schemaFile),
        `is too large for the app to check (more than ${MAX_UNFOLDED} parts once its references are expanded)`,
      );
    validate = createAjv({ allErrors: true }).compile(schema);
    (function collect(v) {
      if (Array.isArray(v)) v.forEach(collect);
      else if (v && typeof v === "object") {
        if (typeof v.pattern === "string" && v.pattern.startsWith(PLAIN_PREFIX))
          if (!plainPatterns.has(v.pattern)) plainPatterns.set(v.pattern, rel(schemaFile));
        Object.values(v).forEach(collect);
      }
    })(schema);
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
    for (const line of dataProblems(validate, data)) report(rel(file), `should pass but ${line}`);
  }
  for (const file of invalid) {
    fixtureCount += 1;
    let data;
    try {
      data = readJson(file);
    } catch {
      continue; // Not JSON at all: certainly invalid.
    }
    if (!dataProblems(validate, data).length)
      report(rel(file), "should fail validation but passes");
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
    for (const line of dataProblems(engine.validate, data, m.data)) report(where, line);
    // ADR-028 section 10: roundSize at most the item count. Creators get a warning from
    // `lullshift validate`; bundled packs must simply follow it.
    const round = roundSizeProblem(m.engine, data);
    if (round) report(where, `${m.data}: ${round}`);
  }
}

// --- Every engine's plain-text pattern against the shared plain-text corpus (decision 18.7,
// security review LOW-2). The pattern must refuse every hidden character; with the text walk,
// pattern and walk together must agree with the host on every case.
const plainCorpusFile = path.join(root, "tests/fixtures/plain-text/cases.json");
let plainCount = 0;
try {
  const { cases } = readJson(plainCorpusFile);
  if (!plainPatterns.size) report("activities/engines", "no engine uses the plain-text pattern");
  for (const [pattern, file] of plainPatterns) {
    const check = createAjv().compile({
      type: "object",
      properties: { field: { type: "string", pattern } },
    });
    for (const c of cases) {
      plainCount += 1;
      const byPattern = check({ field: c.text });
      if (byPattern !== (c.plain || c.reason === "marks"))
        report(file, `plain-text pattern disagrees with the host on "${c.name}"`);
      const combined = byPattern && !packTextProblems({ field: c.text }, dataPath).length;
      if (combined !== c.plain)
        report(file, `pattern plus text check disagree with the host on "${c.name}"`);
    }
  }
} catch (err) {
  report(rel(plainCorpusFile), `can't be read (${err.message})`);
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
  `=== engine compatibility: ${engines.size} engine(s), ${fixtureCount} fixture(s), ${packCount} pack(s), ${corpusCount} parity case(s), ${plainCount} plain-text check(s)`,
);
for (const p of problems) console.error(`  error   ${p}`);
console.log(
  problems.length ? `✗ ${problems.length} problem(s)` : "✓ all engines, fixtures and packs agree",
);
process.exit(problems.length ? 1 : 0);
