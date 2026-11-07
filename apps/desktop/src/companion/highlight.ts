// Syntax highlighting for Companion code blocks (decision 36). This module is small: language
// names, aliases and limits. The grammars live in ./highlightEngine, which is loaded on demand
// (dynamic import) the first time a known language needs highlighting, so they are not part of the
// main bundle. Until it has loaded, code is shown plain.

import type { ReactNode } from "react";
import type { Canonical } from "./highlightEngine";

/** Input above these limits is shown without highlighting. */
export const MAX_HIGHLIGHT_CHARS = 100_000;
export const MAX_HIGHLIGHT_LINES = 3000;

const LABELS: Record<Canonical, string> = {
  javascript: "JavaScript",
  typescript: "TypeScript",
  xml: "HTML/XML",
  css: "CSS",
  json: "JSON",
  yaml: "YAML",
  ini: "INI/TOML",
  sql: "SQL",
  python: "Python",
  rust: "Rust",
  go: "Go",
  java: "Java",
  csharp: "C#",
  c: "C",
  cpp: "C++",
  bash: "Bash",
  shell: "Shell",
  powershell: "PowerShell",
  markdown: "Markdown",
  dockerfile: "Dockerfile",
  diff: "Diff",
};

const ALIASES: Record<string, Canonical> = {
  js: "javascript",
  jsx: "javascript",
  mjs: "javascript",
  cjs: "javascript",
  ts: "typescript",
  tsx: "typescript",
  html: "xml",
  htm: "xml",
  svg: "xml",
  toml: "ini",
  py: "python",
  rs: "rust",
  golang: "go",
  cs: "csharp",
  "c#": "csharp",
  h: "c",
  hpp: "cpp",
  cc: "cpp",
  "c++": "cpp",
  sh: "bash",
  zsh: "bash",
  console: "shell",
  ps: "powershell",
  ps1: "powershell",
  pwsh: "powershell",
  yml: "yaml",
  md: "markdown",
  docker: "dockerfile",
  patch: "diff",
};

type Engine = typeof import("./highlightEngine");
let engine: Engine | null = null;
let loading: Promise<Engine> | null = null;

/** Loads the grammars once; later calls share the same promise. */
export function loadHighlighter(): Promise<Engine> {
  if (engine) return Promise.resolve(engine);
  loading ??= import("./highlightEngine").then(
    (m) => (engine = m),
    (err: unknown) => {
      loading = null; // allow a later retry
      throw err;
    },
  );
  return loading;
}

/** True once the grammars are loaded and `highlightToReact` can colour code. */
export function highlighterReady(): boolean {
  return engine !== null;
}

/** The registered language for a fence tag, or null for an unknown or missing one. */
function resolve(lang: string | null): Canonical | null {
  if (!lang) return null;
  const key = lang.toLowerCase();
  if (Object.prototype.hasOwnProperty.call(LABELS, key)) return key as Canonical;
  if (Object.prototype.hasOwnProperty.call(ALIASES, key)) return ALIASES[key] ?? null;
  return null;
}

/** True when the fence tag names a language that can be highlighted. */
export function isKnownLanguage(lang: string | null): boolean {
  return resolve(lang) !== null;
}

/** A readable name for the code block header; "Code" for an unknown or missing language. */
export function languageLabel(lang: string | null): string {
  const name = resolve(lang);
  return name ? LABELS[name] : "Code";
}

/**
 * The code as React nodes, highlighted when the language is known, the input is a sane size and
 * the grammars have loaded (see `loadHighlighter`). Otherwise, or on any failure, the plain code.
 */
export function highlightToReact(lang: string | null, code: string): ReactNode {
  const name = resolve(lang);
  if (!name || !engine || code.length > MAX_HIGHLIGHT_CHARS) return code;
  let lines = 1;
  for (let i = 0; i < code.length; i++) if (code.charCodeAt(i) === 10) lines++;
  if (lines > MAX_HIGHLIGHT_LINES) return code;
  return engine.highlightCanonical(name, code);
}
