// S02B-A02: every text colour token meets WCAG AA (4.5:1) on the backgrounds it is used on,
// in both themes. Reads the real token file so the check can't drift from the styles.
/// <reference types="node" />
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const css = readFileSync(new URL("./styles/tokens.css", import.meta.url), "utf8");
// Dark values live twice: under the OS preference (System) and under an explicit data-theme.
const mediaStart = css.indexOf("@media (prefers-color-scheme: dark)");
const explicitStart = css.indexOf(
  ':root[data-theme="dark"] {',
  css.indexOf("--backdrop", mediaStart),
);

function tokens(block: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const [, name, hex] of block.matchAll(/--([a-z0-9-]+):\s*(#[0-9a-f]{6})\s*;/gi)) {
    if (name && hex) out[name] = hex;
  }
  return out;
}

const light = tokens(css.slice(0, mediaStart));
const darkSystem = tokens(css.slice(mediaStart, explicitStart));
const darkExplicit = tokens(css.slice(explicitStart));
const themes = { light, dark: { ...light, ...darkExplicit } };

describe("explicit theme", () => {
  it("has the same dark values as the OS-preference block", () => {
    expect(explicitStart).toBeGreaterThan(mediaStart);
    expect(Object.keys(darkSystem).length).toBeGreaterThan(20);
    expect(darkExplicit).toEqual(darkSystem);
  });

  it("lets an explicit light theme win over a dark OS preference", () => {
    expect(css).toContain(':root:not([data-theme="light"])');
  });
});

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

/** Non-text UI (WCAG 1.4.11, 3:1): input boundaries, the switch track, the focus ring. */
const NON_TEXT: [string, string[]][] = [
  ["control-border", ["bg", "surface"]],
  ["accent", ["bg", "surface", "surface-2"]],
];

describe.each(Object.entries(themes))("%s theme, non-text contrast", (_name, t) => {
  it.each(NON_TEXT)("%s is visible on its backgrounds", (fg, bgs) => {
    for (const bg of bgs) {
      const [f, b] = [t[fg], t[bg]];
      if (!f || !b) throw new Error(`missing token ${f ? bg : fg}`);
      expect(contrast(f, b), `${fg} on ${bg}`).toBeGreaterThanOrEqual(3);
    }
  });

  it("the switch thumb stands out from the track in both states", () => {
    for (const track of ["control-border", "accent"]) {
      expect(
        contrast(t["surface"] ?? "", t[track] ?? ""),
        `thumb on ${track}`,
      ).toBeGreaterThanOrEqual(3);
    }
  });
});
