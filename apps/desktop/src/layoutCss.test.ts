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

describe("panel header buttons", () => {
  it("share one size rule; the pressed state changes colour only", () => {
    const r = rule(".head-btn");
    expect(r).toContain("min-height: 32px");
    expect(r).toContain("padding: 0 var(--space-3)");
    expect(r).toContain("font-size: var(--text-sm)");
    expect(r).toContain("align-items: center");
    const pressed = rule('.pin-toggle[aria-pressed="true"]');
    expect(pressed).not.toMatch(/height|padding|font|border-width/);
  });
});

describe("item rows", () => {
  it("the item text looks like text and the icon buttons are square", () => {
    expect(rule(".item-text")).toContain("background: transparent");
    expect(rule(".btn-icon")).toContain("width: 28px");
  });
});
