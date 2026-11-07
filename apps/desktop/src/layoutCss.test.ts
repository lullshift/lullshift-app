import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

// jsdom does no layout, so these guard the CSS rules that keep panels from scrolling sideways.
const css = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

function rule(selector: string): string {
  const start = css.indexOf(`\n${selector} {`);
  expect(start, `rule ${selector}`).toBeGreaterThan(-1);
  return css.slice(start, css.indexOf("}", start));
}

const companionCss = readFileSync(new URL("./companion/companion.css", import.meta.url), "utf8");

describe("model picker layout", () => {
  it("fits the dialog: no minimum width beyond it, up to 640px wide, no sideways scroll", () => {
    const picker = companionCss.slice(companionCss.indexOf("\n.picker {"));
    expect(picker.slice(0, picker.indexOf("}"))).toContain("min-width: 0");
    expect(companionCss).toContain(".dialog:has(.picker) {\n  width: min(640px");
    expect(companionCss).toContain("overflow-x: hidden");
  });
});

describe("detail panel layout", () => {
  it("keeps the scrolling grid to the panel width and never scrolls sideways", () => {
    const r = rule(".detail-scroll");
    expect(r).toContain("grid-template-columns: minmax(0, 1fr)");
    expect(r).toContain("overflow-x: hidden");
    expect(r).toContain("overflow-wrap: anywhere");
  });
});
