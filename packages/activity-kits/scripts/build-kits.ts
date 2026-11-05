#!/usr/bin/env node
// Builds the classic-script kit files (ADR-019). Deterministic: the same sources give the same
// bytes, so the committed dist/ can be checked for drift.
//   node scripts/build-kits.ts           writes dist/lullshift-<kit>-kit-v1.{js,d.ts}
//   node scripts/build-kits.ts --check   exits 1 when dist/ is out of date
//
// Each kit's sources plus the shared core are type-stripped, their imports removed and their
// exports gathered into one IIFE that sets LullshiftKits.<kit> on window (or globalThis where
// there is no window). The sources must follow simple rules, which this script enforces with the
// TypeScript parser: named imports only, and only between the listed files (no side-effect
// imports); top-level `export function|const|class` only; no duplicate top-level names; and none
// of the APIs the sandbox blocks or that would parse HTML.
//
// The banned-word scan is a lint that keeps the kits honest and reviewable. It is not a security
// boundary: the activity sandbox (CSP, iframe sandbox, no IPC) is, whatever a kit file contains.

import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { stripTypeScriptTypes } from "node:module";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import * as prettier from "prettier";
import ts from "typescript";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
export const DIST_DIR = path.join(ROOT, "dist");

const CORE = ["src/core/random.ts", "src/core/json.ts", "src/core/history.ts", "src/core/keys.ts"];
export const KITS: Record<string, string[]> = {
  grid: ["src/grid/grid.ts", "src/grid/dom.ts"],
  card: ["src/card/cards.ts", "src/card/dom.ts"],
  arcade: ["src/arcade/loop.ts", "src/arcade/session.ts"],
};

/**
 * Text the kits must never contain, in code or comments (the copies are scanned by validate). A
 * lint, not a boundary: the sandbox is what stops these APIs.
 */
const BANNED: [RegExp, string][] = [
  [/\beval\b/, "eval"],
  [/\bFunction\s*\(/, "Function()"],
  [/\bfetch\b/, "fetch"],
  [/\bXMLHttpRequest\b/, "XMLHttpRequest"],
  [/\bWebSocket\b/, "WebSocket"],
  [/\bEventSource\b/, "EventSource"],
  [/\bsendBeacon\b/, "sendBeacon"],
  [/\bRTCPeerConnection\b/, "RTCPeerConnection"],
  [/\blocalStorage\b|\bsessionStorage\b|\bindexedDB\b|\bcookie\b/, "browser storage"],
  [/\bclipboard\b/i, "the clipboard"],
  [/__TAURI|\b__proto__\s*=/, "Tauri internals or prototype writes"],
  [/\binnerHTML\b|\bouterHTML\b|\binsertAdjacentHTML\b|\bdocument\.write\b/, "HTML parsing"],
  [/\bimport\s*\(/, "dynamic import"],
  [/\bpostMessage\b/, "postMessage (only the bridge talks to the host)"],
];

const ORPHAN_DOC = /^[ \t]*\/\*\*(?:[^*]|\*(?!\/))*\*\/[ \t]*\r?\n(?=[ \t]*\r?\n)/gm;
const TOP_NAME = /^(?:async\s+)?(?:function\*?|const|let|var|class)\s+([A-Za-z_$][\w$]*)/gm;

function kitVersion(): string {
  const pkg = JSON.parse(readFileSync(path.join(ROOT, "package.json"), "utf8")) as {
    version: string;
  };
  return pkg.version;
}

function strip(source: string): string {
  // Node marks stripTypeScriptTypes experimental; the output is tested, so the warning is noise.
  const warn = process.emitWarning;
  process.emitWarning = () => {};
  try {
    return stripTypeScriptTypes(source, { mode: "strip" });
  } finally {
    process.emitWarning = warn;
  }
}

/** `text` without the given [start, end) ranges. */
function cut(text: string, ranges: [number, number][]): string {
  let out = text;
  for (const [start, end] of [...ranges].sort((a, b) => b[0] - a[0])) {
    out = out.slice(0, start) + out.slice(end);
  }
  return out;
}

/**
 * Finds every top-level import with the TypeScript parser (no regex, so nothing after an import
 * can be swallowed) and returns the text without them. `check` may throw to refuse one.
 */
function removeImports(
  name: string,
  code: string,
  kind: ts.ScriptKind,
  check: (from: string, named: boolean) => void,
): string {
  const file = ts.createSourceFile(name, code, ts.ScriptTarget.Latest, true, kind);
  const ranges: [number, number][] = [];
  for (const st of file.statements) {
    if (ts.isImportEqualsDeclaration(st)) throw new Error(`${name}: import = is not supported`);
    if (!ts.isImportDeclaration(st)) continue;
    if (!ts.isStringLiteral(st.moduleSpecifier)) throw new Error(`${name}: a malformed import`);
    check(st.moduleSpecifier.text, st.importClause !== undefined);
    ranges.push([st.getStart(file), st.end]);
  }
  return cut(code, ranges);
}

/** One source file as plain script text, and the names it exports. Throws on a broken rule. */
export function convertSource(
  rel: string,
  source: string,
  allowed: Set<string>,
): { code: string; exports: string[] } {
  for (const [re, what] of BANNED) {
    if (re.test(source)) throw new Error(`${rel}: mentions ${what}, which kits must not use`);
  }
  // Doc comments of erased types are left behind followed by blank lines; drop them.
  const stripped = strip(source).replace(ORPHAN_DOC, "");
  const jsName = rel.replace(/\.ts$/, ".js");
  const noImports = removeImports(jsName, stripped, ts.ScriptKind.JS, (from, named) => {
    if (!named) {
      throw new Error(`${rel}: side-effect import of ${from}; kits import named bindings only`);
    }
    const target = path.posix.join(path.posix.dirname(rel), from);
    if (!allowed.has(target)) throw new Error(`${rel}: imports ${from}, which isn't in this kit`);
  });
  if (/^\s*import\b/m.test(noImports)) throw new Error(`${rel}: an import the build can't remove`);

  const file = ts.createSourceFile(
    jsName,
    noImports,
    ts.ScriptTarget.Latest,
    true,
    ts.ScriptKind.JS,
  );
  const exports: string[] = [];
  const ranges: [number, number][] = [];
  const only = `${rel}: only export function|const|class is supported`;
  for (const st of file.statements) {
    if (ts.isExportDeclaration(st) || ts.isExportAssignment(st)) throw new Error(only);
    const mods = ts.canHaveModifiers(st) ? ts.getModifiers(st) : undefined;
    const exp = mods?.find((m) => m.kind === ts.SyntaxKind.ExportKeyword);
    if (!exp) continue;
    if (mods?.some((m) => m.kind === ts.SyntaxKind.DefaultKeyword)) throw new Error(only);
    if ((ts.isFunctionDeclaration(st) || ts.isClassDeclaration(st)) && st.name) {
      exports.push(st.name.text);
    } else if (
      ts.isVariableStatement(st) &&
      (st.declarationList.flags & ts.NodeFlags.BlockScoped) === ts.NodeFlags.Const
    ) {
      // Not `export let` or `var`: the kit object holds each value once, frozen, so a later
      // reassignment would silently not show.
      for (const d of st.declarationList.declarations) {
        if (!ts.isIdentifier(d.name)) throw new Error(`${rel}: export plain names, not patterns`);
        exports.push(d.name.text);
      }
    } else {
      throw new Error(only);
    }
    ranges.push([exp.getStart(file), exp.end]);
  }
  return { code: `// ${rel}\n${cut(noImports, ranges).trim()}\n`, exports };
}

function header(kit: string, version: string, ext: "js" | "d.ts"): string {
  const files = [...CORE, ...(KITS[kit] ?? [])].join(", ");
  const lines = [
    `// Lullshift ${kit} kit v1 (ADR-019), from @lullshift/activity-kits ${version}.`,
    "// Licensed under the Apache License, Version 2.0. Copyright the Lullshift authors.",
    "//",
    "// GENERATED, DO NOT EDIT. Built by packages/activity-kits/scripts/build-kits.ts from",
    `// packages/activity-kits/{${files}}.`,
    "//",
  ];
  if (ext === "js") {
    lines.push(
      "// Copy this file next to index.html (`lullshift create --kit` does it) and load it as a",
      "// classic script after the bridge and before your own code:",
      '//   <script src="lullshift-bridge-v1.js"></script>',
      `//   <script src="lullshift-${kit}-kit-v1.js"></script>`,
      '//   <script src="main.js"></script>',
      `// It adds window.LullshiftKits.${kit}. Kits never save anything; save through the bridge.`,
    );
  } else {
    lines.push(
      `// Types for window.LullshiftKits.${kit}. Reference this file from a TypeScript activity:`,
      `//   /// <reference path="./lullshift-${kit}-kit-v1.d.ts" />`,
    );
  }
  return `${lines.join("\n")}\n`;
}

function buildScript(kit: string, version: string): string {
  const files = [...CORE, ...(KITS[kit] ?? [])];
  const allowed = new Set(files);
  const parts = files.map((f) =>
    convertSource(f, readFileSync(path.join(ROOT, f), "utf8"), allowed),
  );
  const exports = parts.flatMap((p) => p.exports);
  const body = parts.map((p) => p.code).join("\n");
  const names = [...body.matchAll(TOP_NAME)].map((m) => m[1] as string);
  const seen = new Set<string>(["root", "kits"]); // the wrapper's own names
  for (const n of names) {
    if (seen.has(n)) throw new Error(`${kit}: the name ${n} is declared twice across kit files`);
    seen.add(n);
  }
  return [
    header(kit, version, "js"),
    "(function () {",
    '"use strict";',
    body,
    // A browser has window; Node (tests) has only globalThis.
    'var root = typeof window !== "undefined" ? window : globalThis;',
    "var kits = (root.LullshiftKits = root.LullshiftKits || {});",
    `kits.${kit} = Object.freeze({ ${exports.join(", ")} });`,
    "})();",
    "",
  ].join("\n");
}

/** Declarations of every kit source, from one TypeScript program. */
function declarations(): Map<string, string> {
  const files = [...CORE, ...Object.values(KITS).flat()].map((f) => path.join(ROOT, f));
  const program = ts.createProgram(files, {
    target: ts.ScriptTarget.ES2022,
    module: ts.ModuleKind.ESNext,
    moduleResolution: ts.ModuleResolutionKind.Bundler,
    lib: ["lib.es2023.d.ts", "lib.dom.d.ts"],
    types: [],
    strict: true,
    declaration: true,
    emitDeclarationOnly: true,
    allowImportingTsExtensions: true,
    skipLibCheck: true,
  });
  const out = new Map<string, string>();
  const result = program.emit(undefined, (fileName, text) => {
    const rel = path
      .relative(ROOT, fileName)
      .replaceAll("\\", "/")
      .replace(/\.d\.ts$/, ".ts");
    out.set(rel, text);
  });
  const problems = ts
    .getPreEmitDiagnostics(program)
    .concat(result.diagnostics)
    .map((d) => ts.flattenDiagnosticMessageText(d.messageText, "\n"));
  if (problems.length) throw new Error(`declaration build failed:\n${problems.join("\n")}`);
  return out;
}

function buildTypes(kit: string, version: string, decls: Map<string, string>): string {
  const body = [...CORE, ...(KITS[kit] ?? [])]
    .map((rel) => {
      const d = decls.get(rel);
      if (d === undefined) throw new Error(`no declarations for ${rel}`);
      return removeImports(`${rel}.d.ts`, d, ts.ScriptKind.TS, () => {})
        .replace(/^export \{\};?\s*$/gm, "")
        .replace(/^export declare /gm, "export ")
        .replace(/^declare /gm, "")
        .trim();
    })
    .join("\n\n");
  return [
    header(kit, version, "d.ts"),
    "declare namespace LullshiftKits {",
    `namespace ${kit} {`,
    body,
    "}",
    "}",
    "",
    "interface Window {",
    "readonly LullshiftKits: typeof LullshiftKits;",
    "}",
    "",
  ].join("\n");
}

/** Every generated file, by name, formatted the way the repository formats code. */
export async function buildKits(): Promise<Map<string, string>> {
  const version = kitVersion();
  const decls = declarations();
  const out = new Map<string, string>();
  for (const kit of Object.keys(KITS)) {
    const base = `lullshift-${kit}-kit-v1`;
    const js = buildScript(kit, version);
    const dts = buildTypes(kit, version, decls);
    const config = (await prettier.resolveConfig(path.join(DIST_DIR, `${base}.js`))) ?? {};
    out.set(`${base}.js`, await prettier.format(js, { ...config, parser: "babel" }));
    out.set(`${base}.d.ts`, await prettier.format(dts, { ...config, parser: "typescript" }));
  }
  return out;
}

/** Names of dist files that differ from a fresh build. */
export async function staleFiles(): Promise<string[]> {
  const built = await buildKits();
  const stale: string[] = [];
  for (const [name, text] of built) {
    let current: string;
    try {
      current = readFileSync(path.join(DIST_DIR, name), "utf8").replaceAll("\r\n", "\n");
    } catch {
      current = "";
    }
    if (current !== text) stale.push(name);
  }
  return stale;
}

async function main(): Promise<number> {
  if (process.argv.includes("--check")) {
    const stale = await staleFiles();
    if (stale.length) {
      console.error(`Out of date: ${stale.join(", ")}. Run: pnpm kits:build`);
      return 1;
    }
    console.log("activity kits: dist is up to date");
    return 0;
  }
  const built = await buildKits();
  mkdirSync(DIST_DIR, { recursive: true });
  for (const [name, text] of built) writeFileSync(path.join(DIST_DIR, name), text);
  console.log(`activity kits: wrote ${[...built.keys()].join(", ")}`);
  return 0;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().then(
    (code) => process.exit(code),
    (e: unknown) => {
      console.error((e as Error).message ?? e);
      process.exit(1);
    },
  );
}
