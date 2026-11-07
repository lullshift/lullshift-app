// Syntax highlighting for Companion code blocks (decision 36). lowlight (MIT) on highlight.js
// (BSD-3-Clause), with an explicit language list and no auto-detection. The hast tree it returns
// is turned into React elements here: text becomes React text nodes, elements become class-only
// spans, everything else is dropped. No HTML string is ever built.

import { createElement, type ReactNode } from "react";
import bash from "highlight.js/lib/languages/bash";
import c from "highlight.js/lib/languages/c";
import cpp from "highlight.js/lib/languages/cpp";
import csharp from "highlight.js/lib/languages/csharp";
import css from "highlight.js/lib/languages/css";
import diff from "highlight.js/lib/languages/diff";
import dockerfile from "highlight.js/lib/languages/dockerfile";
import go from "highlight.js/lib/languages/go";
import ini from "highlight.js/lib/languages/ini";
import java from "highlight.js/lib/languages/java";
import javascript from "highlight.js/lib/languages/javascript";
import json from "highlight.js/lib/languages/json";
import markdown from "highlight.js/lib/languages/markdown";
import powershell from "highlight.js/lib/languages/powershell";
import python from "highlight.js/lib/languages/python";
import rust from "highlight.js/lib/languages/rust";
import shell from "highlight.js/lib/languages/shell";
import sql from "highlight.js/lib/languages/sql";
import typescript from "highlight.js/lib/languages/typescript";
import xml from "highlight.js/lib/languages/xml";
import yaml from "highlight.js/lib/languages/yaml";
import { createLowlight } from "lowlight";

/** Input above these limits is shown without highlighting. */
export const MAX_HIGHLIGHT_CHARS = 100_000;
export const MAX_HIGHLIGHT_LINES = 3000;

const LANGUAGES = {
  javascript,
  typescript,
  xml,
  css,
  json,
  yaml,
  ini,
  sql,
  python,
  rust,
  go,
  java,
  csharp,
  c,
  cpp,
  bash,
  shell,
  powershell,
  markdown,
  dockerfile,
  diff,
};

type Canonical = keyof typeof LANGUAGES;

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

const lowlight = createLowlight();
lowlight.register(LANGUAGES);
for (const [alias, name] of Object.entries(ALIASES)) lowlight.registerAlias({ [name]: [alias] });

/** The registered language for a fence tag, or null for an unknown or missing one. */
function resolve(lang: string | null): Canonical | null {
  if (!lang) return null;
  const key = lang.toLowerCase();
  if (Object.prototype.hasOwnProperty.call(LANGUAGES, key)) return key as Canonical;
  if (Object.prototype.hasOwnProperty.call(ALIASES, key)) return ALIASES[key] ?? null;
  return null;
}

/** A readable name for the code block header; "Code" for an unknown or missing language. */
export function languageLabel(lang: string | null): string {
  const name = resolve(lang);
  return name ? LABELS[name] : "Code";
}

interface HastNode {
  type: string;
  value?: unknown;
  properties?: { className?: unknown } | null;
  children?: HastNode[];
}

function toReact(node: HastNode, key: number): ReactNode {
  if (node.type === "text") return typeof node.value === "string" ? node.value : null;
  if (node.type === "element") {
    const cls = node.properties?.className;
    const className = Array.isArray(cls)
      ? cls.filter((x): x is string => typeof x === "string").join(" ")
      : undefined;
    const kids = (node.children ?? []).map((child, i) => toReact(child, i));
    return createElement("span", { key, className: className || undefined }, ...kids);
  }
  return null;
}

/**
 * The code as React nodes, highlighted when the language is known and the input is a sane size.
 * Any failure returns the plain code string.
 */
export function highlightToReact(lang: string | null, code: string): ReactNode {
  const name = resolve(lang);
  if (!name || code.length > MAX_HIGHLIGHT_CHARS) return code;
  let lines = 1;
  for (let i = 0; i < code.length; i++) if (code.charCodeAt(i) === 10) lines++;
  if (lines > MAX_HIGHLIGHT_LINES) return code;
  try {
    const tree = lowlight.highlight(name, code) as unknown as HastNode;
    return (tree.children ?? []).map((child, i) => toReact(child, i));
  } catch {
    return code;
  }
}
