// `lullshift validate` (ADR-017): the schema plus the folder rules the host and the install flow
// apply (TECHNICAL_ARCHITECTURE §7.3, §7.5). Errors block packing; warnings don't.

import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import path from "node:path";
import { BRIDGE_FILE, SDK_VERSION, packTextProblems } from "@lullshift/activity-sdk";
import {
  activityManifestSchema,
  createAjv,
  dataPath,
  friendlyErrors,
  validateActivityManifest,
  type ActivityManifestV1,
} from "@lullshift/agent-protocol";
import { CODE_TYPES, LIMITS, listFiles, type FileEntry } from "./files.ts";
import { KIT_NAMES } from "./create.ts";
import { IMAGE_LIMITS, checkImage, imageReferences } from "./images.ts";
import { BRIDGE_PATH, ENGINES_DIR, KITS_DIR } from "./paths.ts";

export interface Engine {
  id: string;
  major: number;
  dir: string;
  manifest: ActivityManifestV1;
  dataSchema: object;
}

export interface Report {
  dir: string;
  manifest: ActivityManifestV1 | null;
  files: FileEntry[];
  errors: string[];
  warnings: string[];
  /** For content packs: the engine and the parsed data. */
  engine?: Engine;
  data?: unknown;
}

// Source text that names things the sandbox blocks. The sandbox is the boundary; this is a
// warning so creators learn early, and a hint for reviewers.
const RISKY: [RegExp, string][] = [
  [/\bfetch\s*\(/, "fetch()"],
  [/\bXMLHttpRequest\b/, "XMLHttpRequest"],
  [/\bWebSocket\b/, "WebSocket"],
  [/\bEventSource\b/, "EventSource"],
  [/\bsendBeacon\b/, "sendBeacon"],
  [/\bRTCPeerConnection\b/, "RTCPeerConnection"],
  [/__TAURI/, "Tauri internals"],
  [/\beval\s*\(/, "eval()"],
  [/\bnew\s+Function\s*\(/, "new Function()"],
  [
    /\blocalStorage\b|\bsessionStorage\b|\bindexedDB\b/,
    "browser storage (use LullshiftBridge.storage)",
  ],
  [/\bnavigator\.clipboard\b/, "the clipboard"],
];

// A kit file name; the name part can't hold dots or slashes, so it never leaves KITS_DIR.
const KIT_FILE = /^lullshift-([a-z0-9]+(?:-[a-z0-9]+)*)-kit-v1[.]js$/;

const ENGINE_REF = /^([a-z0-9]+(?:\.[a-z0-9-]+)+)@([0-9]+)$/;

/** Notes a pack may keep beside its data (packed, never served). */
const PACK_NOTE_TYPES = new Set(["md", "txt"]);

const readJson = (file: string): unknown => JSON.parse(readFileSync(file, "utf8"));

/** Engines in `activities/engines` plus any extra folders, keyed by `id@major`. */
export function findEngines(extra: string[] = []): Map<string, Engine> {
  const found = new Map<string, Engine>();
  const roots = [ENGINES_DIR, ...extra];
  for (const root of roots) {
    if (!existsSync(root)) continue;
    const dirs = existsSync(path.join(root, "engine.json"))
      ? [root]
      : readdirSync(root)
          .map((n) => path.join(root, n))
          .filter((d) => statSync(d).isDirectory());
    for (const dir of dirs) {
      const enginePath = path.join(dir, "engine.json");
      if (!existsSync(enginePath)) continue;
      try {
        const e = readJson(enginePath) as {
          schemaVersion: number;
          id: string;
          major: number;
          dataSchema: string;
        };
        const manifest = readJson(path.join(dir, "manifest.json")) as ActivityManifestV1;
        const schemaFile = path.resolve(dir, e.dataSchema);
        if (path.dirname(schemaFile) !== path.resolve(dir)) continue;
        const key = `${e.id}@${e.major}`;
        if (!found.has(key)) {
          found.set(key, {
            id: e.id,
            major: e.major,
            dir,
            manifest,
            dataSchema: readJson(schemaFile) as object,
          });
        }
      } catch {
        // A broken engine folder is that engine's problem; packs naming it report "not found".
      }
    }
  }
  return found;
}

export function validateFolder(dir: string, opts: { engines?: string[] } = {}): Report {
  const report: Report = { dir, manifest: null, files: [], errors: [], warnings: [] };
  const { errors, warnings } = report;
  if (!existsSync(dir) || !statSync(dir).isDirectory()) {
    errors.push(`${dir} isn't a folder`);
    return report;
  }

  const listing = listFiles(dir);
  report.files = listing.files;
  errors.push(...listing.errors);
  for (const s of listing.skipped) warnings.push(`${s}: hidden, so it is never served or packed`);
  const has = (rel: string) => listing.files.find((f) => f.rel === rel);

  if (!has("manifest.json")) {
    errors.push("manifest.json is missing");
    return report;
  }
  let raw: unknown;
  try {
    raw = readJson(path.join(dir, "manifest.json"));
  } catch (e) {
    errors.push(`manifest.json isn't valid JSON: ${(e as Error).message}`);
    return report;
  }
  const checked = validateManifestAll(raw);
  if (!checked.ok) {
    errors.push(...checked.errors.map((e) => `manifest.json ${e}`));
    return report;
  }
  const m = checked.value;
  report.manifest = m;

  // Declared files exist.
  if (m.kind === "app") {
    if (!has("index.html")) errors.push("index.html (the entry point) is missing");
  }
  if (m.preview) {
    const p = has(m.preview);
    if (!p) errors.push(`preview ${m.preview} is missing`);
    else if (p.bytes > LIMITS.previewBytes) errors.push(`preview ${m.preview} is over 256 KiB`);
  } else {
    warnings.push('no preview image: cards and the paused view will show none (add "preview")');
  }
  if (!m.help) {
    warnings.push('no help: players won\'t see how to play (add "help" to manifest.json)');
  }
  if (m.sdkVersion !== SDK_VERSION && compareSemver(m.sdkVersion, SDK_VERSION) > 0) {
    errors.push(`sdkVersion ${m.sdkVersion} is newer than this SDK (${SDK_VERSION})`);
  }

  if (m.kind === "app") checkApp(report, has);
  else checkContent(report, has, opts.engines ?? []);
  return report;
}

/** Like `validateActivityManifest`, but reporting every problem (tools want them all). */
const allErrors = (() => {
  const ajv = createAjv({ allErrors: true });
  const validate = ajv.compile(activityManifestSchema);
  return (input: unknown): string[] =>
    validate(input)
      ? []
      : (validate.errors ?? []).map((e) => {
          const allowed = (e.params as { allowedValues?: unknown[] }).allowedValues;
          const more = allowed ? `: ${allowed.map(String).join(", ")}` : "";
          return `${e.instancePath || "/"} ${e.message ?? "is invalid"}${more}`;
        });
})();

function validateManifestAll(
  raw: unknown,
): { ok: true; value: ActivityManifestV1 } | { ok: false; errors: string[] } {
  const quick = validateActivityManifest(raw);
  if (quick.ok) return quick;
  return { ok: false, errors: allErrors(raw) };
}

function checkApp(report: Report, has: (rel: string) => FileEntry | undefined) {
  const bridge = has(BRIDGE_FILE);
  if (!bridge) {
    report.warnings.push(
      `${BRIDGE_FILE} is missing: copy it from the SDK (lullshift create does this) and load it before your own scripts`,
    );
  } else if (!readFileSync(bridge.abs).equals(readFileSync(BRIDGE_PATH))) {
    report.warnings.push(`${BRIDGE_FILE} differs from the SDK's copy (${SDK_VERSION}); replace it`);
  }
  // Kits (ADR-019) are copied in like the bridge; a changed copy is the activity's own code then.
  for (const f of report.files) {
    const kit = KIT_FILE.exec(path.posix.basename(f.rel));
    if (!kit) continue;
    const sdkCopy = path.join(KITS_DIR, `lullshift-${kit[1]}-kit-v1.js`);
    if (!existsSync(sdkCopy)) {
      report.warnings.push(`${f.rel} isn't a Lullshift kit (known kits: ${KIT_NAMES.join(", ")})`);
    } else if (!readFileSync(f.abs).equals(readFileSync(sdkCopy))) {
      report.warnings.push(`${f.rel} differs from the SDK's copy; replace it`);
    }
  }
  // For a pack, the app serves `images/` from the pack, never from its engine (S12-T11).
  if (has("engine.json")) {
    for (const f of report.files) {
      if (f.rel.startsWith("images/")) {
        report.errors.push(
          `${f.rel}: an engine can't use an images/ folder; packs' pictures are served there`,
        );
      }
    }
  }
  for (const f of report.files) {
    if (f.rel === BRIDGE_FILE) continue;
    if (f.ext === "html") checkHtml(f, readFileSync(f.abs, "utf8"), report);
    if (f.ext === "js" || f.ext === "mjs" || f.ext === "html") {
      const text = readFileSync(f.abs, "utf8");
      for (const [re, what] of RISKY) {
        if (re.test(text))
          report.warnings.push(`${f.rel} mentions ${what}, which the sandbox blocks`);
      }
    }
  }
}

function checkHtml(f: FileEntry, html: string, report: Report) {
  // The activity CSP forbids inline script and module scripts (ADR-012): say so before the
  // creator finds a blank page.
  for (const tag of html.matchAll(/<script\b([^>]*)>([\s\S]*?)<\/script>/gi)) {
    const attrs = tag[1] ?? "";
    if (/type\s*=\s*["']?module/i.test(attrs)) {
      report.errors.push(`${f.rel}: module scripts can't load in the sandbox; use classic scripts`);
    }
    if (!/\bsrc\s*=/.test(attrs) && (tag[2] ?? "").trim() !== "") {
      report.errors.push(`${f.rel}: inline <script> is blocked by the sandbox; move it to a file`);
    }
  }
  if (/\son[a-z]+\s*=\s*["']/i.test(html)) {
    report.errors.push(
      `${f.rel}: inline event handlers (onclick=…) are blocked; use addEventListener`,
    );
  }
  if (/<style\b/i.test(html) || /\sstyle\s*=\s*["']/i.test(html)) {
    report.warnings.push(`${f.rel}: inline styles are blocked by the sandbox; use a stylesheet`);
  }
  if (/\b(?:src|href)\s*=\s*["']?(?:https?:)?\/\//i.test(html)) {
    report.errors.push(`${f.rel}: loads something from the network, which the sandbox blocks`);
  }
}

function checkContent(
  report: Report,
  has: (rel: string) => FileEntry | undefined,
  extraEngines: string[],
) {
  const m = report.manifest!;
  const images = new Set(m.images ?? []);
  for (const f of report.files) {
    if (CODE_TYPES.has(f.ext)) {
      report.errors.push(`${f.rel}: content packs can't contain code`);
    } else if (f.rel.startsWith("images/") && !images.has(f.rel)) {
      // Only listed pictures are checked and served (ADR-016 amendment, S12-T11).
      report.errors.push(
        `${f.rel}: isn't listed in "images" in manifest.json, so the app would refuse the pack`,
      );
    } else if (
      f.rel !== "manifest.json" &&
      f.rel !== m.data &&
      f.rel !== m.preview &&
      !images.has(f.rel) &&
      !PACK_NOTE_TYPES.has(f.ext)
    ) {
      // Packs are data, a preview, listed pictures and notes (ADR-016): the host serves only
      // the engine's files and the pack's listed pictures, so anything else never reaches the
      // player.
      report.errors.push(
        `${f.rel}: content packs hold only manifest.json, their data file, a preview image, pictures listed in "images" and notes (.md, .txt)`,
      );
    }
  }
  for (const image of images) {
    const f = has(image);
    if (!f) {
      report.errors.push(`${image} is listed in "images" but is missing`);
      continue;
    }
    if (f.bytes > IMAGE_LIMITS.bytes) {
      report.errors.push(`${image} is over 512 KiB`);
      continue;
    }
    const r = checkImage(image, readFileSync(f.abs));
    if (!r.ok) report.errors.push(`${image} ${r.why}`);
  }
  const ref = ENGINE_REF.exec(m.engine ?? "");
  const dataFile = m.data ? has(m.data) : undefined;
  if (!dataFile) {
    report.errors.push(`data ${m.data ?? ""} is missing`);
    return;
  }
  if (dataFile.bytes > LIMITS.dataBytes) {
    report.errors.push(`data ${m.data} is over 256 KiB`);
    return;
  }
  let data: unknown;
  try {
    data = readJson(dataFile.abs);
  } catch (e) {
    report.errors.push(`${m.data} isn't valid JSON: ${(e as Error).message}`);
    return;
  }
  report.data = data;
  // Every picture the data names must be listed, so the app can check it before the pack starts.
  for (const ref of new Set(imageReferences(data))) {
    if (!images.has(ref)) {
      report.errors.push(
        `${m.data} names ${JSON.stringify(ref.slice(0, 80))}, which isn't listed in "images" in manifest.json`,
      );
    }
  }
  const engine = ref ? findEngines(extraEngines).get(`${ref[1]}@${ref[2]}`) : undefined;
  if (!engine) {
    report.errors.push(
      `engine ${m.engine} wasn't found (looked in activities/engines${extraEngines.length ? " and --engines" : ""})`,
    );
    return;
  }
  report.engine = engine;
  const ajv = createAjv({ allErrors: true });
  let validate;
  try {
    validate = ajv.compile(engine.dataSchema);
  } catch (e) {
    report.errors.push(`engine ${m.engine} has a broken data schema: ${(e as Error).message}`);
    return;
  }
  const problems: string[] = [];
  if (!validate(data)) {
    // Plain words with a data path (S07-A03): "deck.json: cards[3].front is longer than 200
    // characters". The app's own check words problems the same way.
    problems.push(...friendlyErrors(validate.errors, m.data));
  }
  // Then the host's plain-text rule on every string, as the app runs it after the schema
  // (security review LOW-2: the combining-mark half can't be a schema pattern).
  for (const line of packTextProblems(data, dataPath)) {
    const shown = `${m.data}: ${line}`;
    if (!problems.includes(shown)) problems.push(shown);
  }
  report.errors.push(...problems);
}

/** -1, 0 or 1, comparing MAJOR.MINOR.PATCH; pre-releases sort before their release. */
export function compareSemver(a: string, b: string): number {
  const parse = (v: string) => {
    const [core = "", pre] = v.split("-", 2);
    return { nums: core.split(".").map(Number), pre };
  };
  const x = parse(a);
  const y = parse(b);
  for (let i = 0; i < 3; i++) {
    const d = (x.nums[i] ?? 0) - (y.nums[i] ?? 0);
    if (d !== 0) return Math.sign(d);
  }
  if (x.pre === y.pre) return 0;
  if (x.pre === undefined) return 1;
  if (y.pre === undefined) return -1;
  return x.pre < y.pre ? -1 : 1;
}

/** Prints a report; returns the exit code. */
export function printReport(r: Report, out: (line: string) => void = console.log): number {
  const name = r.manifest ? `${r.manifest.name} (${r.manifest.id}@${r.manifest.version})` : r.dir;
  for (const e of r.errors) out(`  error   ${e}`);
  for (const w of r.warnings) out(`  warning ${w}`);
  if (r.errors.length) {
    out(`✗ ${name}: ${r.errors.length} error(s), ${r.warnings.length} warning(s)`);
    return 1;
  }
  out(`✓ ${name}: valid${r.warnings.length ? `, ${r.warnings.length} warning(s)` : ""}`);
  return 0;
}
