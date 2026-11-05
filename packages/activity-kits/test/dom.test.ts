// The DOM bindings against a tiny hand-written fake DOM (the workspace has no DOM library, ADR-019).
// Real focus and screen reader behaviour is checked through the simulator on real games.
import { describe, expect, it } from "vitest";
import { Table, customDeck } from "../src/card/index.ts";
import { Grid, bindGrid } from "../src/grid/index.ts";
import { bindTable } from "../src/card/index.ts";

class FakeDocument {
  activeElement: FakeElement | null = null;
  createElement(tag: string) {
    return new FakeElement(tag, this);
  }
}

class FakeElement {
  children: FakeElement[] = [];
  parent: FakeElement | null = null;
  attributes = new Map<string, string>();
  listeners = new Map<string, Set<(e: unknown) => void>>();
  className = "";
  tabIndex = -1;
  type = "";
  private text = "";
  tagName: string;
  ownerDocument: FakeDocument;
  constructor(tagName: string, ownerDocument: FakeDocument) {
    this.tagName = tagName;
    this.ownerDocument = ownerDocument;
  }
  get textContent() {
    return this.text + this.children.map((c) => c.textContent).join("");
  }
  set textContent(v: string) {
    for (const c of this.children) c.parent = null;
    this.children = [];
    this.text = v;
  }
  appendChild(c: FakeElement) {
    c.parent = this;
    this.children.push(c);
    return c;
  }
  setAttribute(k: string, v: string) {
    this.attributes.set(k, String(v));
  }
  getAttribute(k: string) {
    return this.attributes.get(k) ?? null;
  }
  addEventListener(t: string, fn: (e: unknown) => void) {
    if (!this.listeners.has(t)) this.listeners.set(t, new Set());
    this.listeners.get(t)!.add(fn);
  }
  removeEventListener(t: string, fn: (e: unknown) => void) {
    this.listeners.get(t)?.delete(fn);
  }
  dispatch(t: string, e: Record<string, unknown> = {}) {
    let defaultPrevented = false;
    const event = { ...e, target: this, preventDefault: () => (defaultPrevented = true) };
    const chain: FakeElement[] = [this];
    while (chain[chain.length - 1]!.parent) chain.push(chain[chain.length - 1]!.parent!);
    for (const el of chain) {
      for (const fn of el.listeners.get(t) ?? []) fn(event);
    }
    return defaultPrevented;
  }
  focus() {
    this.ownerDocument.activeElement = this;
  }
  contains(other: FakeElement | null): boolean {
    for (let el = other; el; el = el.parent) if (el === this) return true;
    return false;
  }
  all(): FakeElement[] {
    return this.children.flatMap((c) => [c, ...c.all()]);
  }
  buttons() {
    return this.all().filter((e) => e.tagName === "button");
  }
}

const asHtml = (e: FakeElement) => e as unknown as HTMLElement;

describe("bindGrid", () => {
  function setup() {
    const doc = new FakeDocument();
    const root = doc.createElement("div");
    root.textContent = "<b>old</b>";
    const grid = new Grid<boolean>(3, 2, false);
    const activated: number[][] = [];
    let changes = 0;
    const binding = bindGrid(asHtml(root), grid, {
      name: "Pond",
      label: (lit, x, y) => `Row ${y + 1}, column ${x + 1}, ${lit ? "lit" : "unlit"}`,
      text: (lit) => (lit ? "<i>on</i>" : ""),
      pressed: (lit) => lit,
      onActivate: (x, y) => {
        grid.edit((g) => g.set(x, y, !g.get(x, y)));
        activated.push([x, y]);
      },
      onChange: () => changes++,
    });
    return { doc, root, grid, binding, activated, changes: () => changes };
  }

  it("renders a labelled ARIA grid with one tab stop", () => {
    const { root } = setup();
    expect(root.getAttribute("role")).toBe("grid");
    expect(root.getAttribute("aria-label")).toBe("Pond");
    const rows = root.children;
    expect(rows.map((r) => r.getAttribute("role"))).toEqual(["row", "row"]);
    expect(rows[0]!.children.map((c) => c.getAttribute("role"))).toEqual([
      "gridcell",
      "gridcell",
      "gridcell",
    ]);
    const buttons = root.buttons();
    expect(buttons).toHaveLength(6);
    expect(buttons.filter((b) => b.tabIndex === 0)).toHaveLength(1);
    expect(buttons[4]!.getAttribute("aria-label")).toBe("Row 2, column 2, unlit");
    expect(buttons[0]!.getAttribute("aria-pressed")).toBe("false");
    expect(root.textContent).not.toContain("old");
  });

  it("moves the tab stop and focus with the arrow keys", () => {
    const { root, doc, grid } = setup();
    const prevented = root.buttons()[0]!.dispatch("keydown", { key: "ArrowRight" });
    expect(prevented).toBe(true);
    expect(grid.cursor).toEqual({ x: 1, y: 0 });
    const buttons = root.buttons();
    expect(buttons[1]!.tabIndex).toBe(0);
    expect(buttons[0]!.tabIndex).toBe(-1);
    expect(doc.activeElement).toBe(buttons[1]);
    root.buttons()[1]!.dispatch("keydown", { key: "End", ctrlKey: true });
    expect(grid.cursor).toEqual({ x: 2, y: 1 });
    // Keys it doesn't use pass through.
    expect(root.buttons()[5]!.dispatch("keydown", { key: "q" })).toBe(false);
  });

  it("activates by click, sets text safely, and undoes with Ctrl+Z", () => {
    const { root, binding, activated, grid, changes } = setup();
    root.buttons()[4]!.dispatch("click");
    binding.render();
    expect(activated).toEqual([[1, 1]]);
    expect(grid.cursor).toEqual({ x: 1, y: 1 });
    const b = root.buttons()[4]!;
    expect(b.getAttribute("aria-pressed")).toBe("true");
    expect(b.textContent).toBe("<i>on</i>"); // as text, never markup
    expect(b.children).toHaveLength(0);
    b.dispatch("keydown", { key: "z", ctrlKey: true });
    expect(grid.cells[4]).toBe(false);
    expect(changes()).toBe(1);
  });

  it("cleans up on destroy", () => {
    const { root, binding } = setup();
    const b = root.buttons()[0]!;
    binding.destroy();
    expect(root.children).toHaveLength(0);
    expect(b.listeners.get("click")?.size ?? 0).toBe(0);
    expect(root.listeners.get("keydown")?.size ?? 0).toBe(0);
  });
});

describe("bindTable", () => {
  function setup() {
    const doc = new FakeDocument();
    const root = doc.createElement("div");
    const cards = customDeck([1, 2, 3].map((rank) => ({ suit: "hearts", rank })));
    cards[2]!.faceUp = true;
    const table = new Table([
      { id: "stock", name: "Stock", cards },
      { id: "waste", name: "Waste" },
    ]);
    let changes = 0;
    const results: string[] = [];
    bindTable(asHtml(root), table, {
      name: "Table",
      onResult: (r) => results.push(r),
      onChange: () => changes++,
    });
    return { doc, root, table, results, changes: () => changes };
  }

  it("labels piles and cards, and empty piles stay reachable", () => {
    const { root } = setup();
    expect(root.getAttribute("aria-label")).toBe("Table");
    expect(root.children.map((p) => p.getAttribute("aria-label"))).toEqual([
      "Stock, 3 cards",
      "Waste, 0 cards",
    ]);
    const labels = root.buttons().map((b) => b.getAttribute("aria-label"));
    expect(labels).toEqual(["Face-down card", "Face-down card", "3 of hearts", "Empty Waste"]);
    expect(root.buttons().filter((b) => b.tabIndex === 0)).toHaveLength(1);
  });

  it("picks up and places with clicks, then undoes", () => {
    const { root, table, results, changes } = setup();
    root.buttons()[2]!.dispatch("click");
    expect(results).toEqual(["selected"]);
    expect(root.buttons()[2]!.getAttribute("aria-label")).toBe("3 of hearts, picked up");
    root.buttons()[3]!.dispatch("click");
    expect(results).toEqual(["selected", "moved"]);
    expect(table.pile("waste")!.cards).toHaveLength(1);
    expect(changes()).toBe(1);
    root.buttons()[0]!.dispatch("keydown", { key: "z", metaKey: true });
    expect(table.pile("waste")!.cards).toHaveLength(0);
    expect(changes()).toBe(2);
  });

  it("moves focus between piles by keyboard", () => {
    const { root, table, doc } = setup();
    const first = root.buttons().find((b) => b.tabIndex === 0)!;
    first.focus();
    first.dispatch("keydown", { key: "ArrowRight" });
    expect(table.focus).toEqual({ pile: 1, card: -1 });
    expect(doc.activeElement?.getAttribute("aria-label")).toBe("Empty Waste");
    doc.activeElement!.dispatch("keydown", { key: "Home" });
    expect(table.focus).toEqual({ pile: 0, card: 2 });
  });
});
