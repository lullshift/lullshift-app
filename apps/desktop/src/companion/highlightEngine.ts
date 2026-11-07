// The heavy half of Companion syntax highlighting (decision 36): lowlight (MIT) on highlight.js
// (BSD-3-Clause) with an explicit language list and no auto-detection. Loaded on demand by
// ./highlight the first time a reply shows a code block in a known language, so the grammars stay
// out of the main bundle. The hast tree is turned into React elements: text becomes React text
// nodes, elements become class-only spans, everything else is dropped. No HTML string is built.

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

export const LANGUAGES = {
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

export type Canonical = keyof typeof LANGUAGES;

const lowlight = createLowlight();
lowlight.register(LANGUAGES);

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

/** Highlights `code` as the already-resolved language. Any failure returns the plain code. */
export function highlightCanonical(name: Canonical, code: string): ReactNode {
  try {
    const tree = lowlight.highlight(name, code) as unknown as HastNode;
    return (tree.children ?? []).map((child, i) => toReact(child, i));
  } catch {
    return code;
  }
}
