// S02B-A02: every text colour token meets WCAG AA (4.5:1) on the backgrounds it is used on,
// in both themes. Reads the real token file so the check can't drift from the styles.
/// <reference types="node" />
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const css = readFileSync(new URL("./styles/tokens.css", import.meta.url), "utf8");
const darkStart = css.indexOf("@media (prefers-color-scheme: dark)");

function tokens(block: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const [, name, hex] of block.matchAll(/--([a-z0-9-]+):\s*(#[0-9a-f]{6})\s*;/gi)) {
    if (name && hex) out[name] = hex;
  }
  return out;
}

const light = tokens(css.slice(0, darkStart));
const themes = { light, dark: { ...light, ...tokens(css.slice(darkStart)) } };

function luminance(hex: string): number {
  const channel = (i: number) => {
    const c = parseInt(hex.slice(i, i + 2), 16) / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5);
}

function contrast(a: string, b: string): number {
  const [x, y] = [luminance(a), luminance(b)];
  return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
}

/** [text token, background tokens it appears on] */
const PAIRS: [string, string[]][] = [
  ["text", ["bg", "surface", "surface-2", "surface-hover"]],
  ["text-muted", ["bg", "surface", "surface-2", "surface-hover"]],
  ["text-subtle", ["bg", "surface", "surface-hover"]],
  ["accent", ["bg", "surface", "accent-soft"]],
  ["on-accent", ["accent", "accent-hover"]],
  ["attention", ["surface", "attention-soft"]],
  ["success", ["surface", "success-soft"]],
  ["danger", ["surface", "danger-soft"]],
  ["text-muted", ["neutral-soft"]],
];

describe.each(Object.entries(themes))("%s theme", (_name, t) => {
  it.each(PAIRS)("%s is readable on its backgrounds", (fg, bgs) => {
    for (const bg of bgs) {
      const [f, b] = [t[fg], t[bg]];
      if (!f || !b) throw new Error(`missing token ${f ? bg : fg}`);
      expect(contrast(f, b), `${fg} on ${bg}`).toBeGreaterThanOrEqual(4.5);
    }
  });
});
