// Grid kit DOM binding (ADR-019): one button per cell inside an ARIA grid, a single tab stop that
// follows the cursor (roving tabindex), a text label per cell, and keyboard navigation. Text is
// set with textContent only; the binding never parses HTML.

import { keyAction, type KeyAction, type KeyOptions } from "../core/keys.ts";
import type { Grid } from "./grid.ts";

export interface GridBindingOptions<C> {
  /** Accessible name of the whole grid, e.g. "Lantern pond, 5 by 5". */
  name: string;
  /** Accessible label of one cell, e.g. "Row 2, column 3, lit". Required. */
  label(cell: C, x: number, y: number): string;
  /** Visible text of one cell (default none). Decorative text is fine; the label is what is read. */
  text?(cell: C, x: number, y: number): string;
  /** Extra class names for one cell's button, for styling by state. */
  className?(cell: C, x: number, y: number): string;
  /** When given, the button gets aria-pressed with this value. */
  pressed?(cell: C, x: number, y: number): boolean;
  /** Enter, Space or a click on a cell. The cursor is already on it. */
  onActivate?(x: number, y: number): void;
  /** The cursor moved by keyboard. */
  onMove?(x: number, y: number): void;
  /**
   * Any other action from `keyAction` (undo, redo, cancel). Return true if you handled it.
   * Otherwise undo and redo call `grid.undo()` and `grid.redo()`, then `onChange`.
   */
  onAction?(action: KeyAction, event: KeyboardEvent): boolean | void;
  /** The binding changed the cells (keyboard undo or redo): save and update your status. */
  onChange?(): void;
  /** The game uses the selection: cells then carry aria-selected. */
  selectable?: boolean;
  keys?: KeyOptions;
}

export interface GridBinding {
  /** Updates labels, text, classes and the tab stop from the model. Call after every change. */
  render(): void;
  /** Moves keyboard focus to the cursor cell. */
  focus(): void;
  /** Removes the cells and listeners. */
  destroy(): void;
}

const NAVIGATION: ReadonlySet<KeyAction> = new Set<KeyAction>([
  "up",
  "down",
  "left",
  "right",
  "home",
  "end",
  "first",
  "last",
  "pageUp",
  "pageDown",
]);

/** Renders `grid` into `container` (an empty element) and keeps it in step on `render()`. */
export function bindGrid<C>(
  container: HTMLElement,
  grid: Grid<C>,
  options: GridBindingOptions<C>,
): GridBinding {
  const doc = container.ownerDocument;
  const buttons: HTMLButtonElement[] = [];
  const gridcells: HTMLElement[] = [];
  const cleanups: (() => void)[] = [];

  container.textContent = "";
  container.setAttribute("role", "grid");
  container.setAttribute("aria-label", options.name);
  container.setAttribute("aria-rowcount", String(grid.height));
  container.setAttribute("aria-colcount", String(grid.width));
  if (options.selectable) container.setAttribute("aria-multiselectable", "true");

  for (let y = 0; y < grid.height; y++) {
    const row = doc.createElement("div");
    row.setAttribute("role", "row");
    row.className = "kit-row";
    for (let x = 0; x < grid.width; x++) {
      const cell = doc.createElement("div");
      cell.setAttribute("role", "gridcell");
      cell.className = "kit-gridcell";
      const button = doc.createElement("button");
      button.type = "button";
      const onClick = () => {
        grid.setCursor(x, y);
        render();
        options.onActivate?.(x, y);
      };
      button.addEventListener("click", onClick);
      cleanups.push(() => button.removeEventListener("click", onClick));
      cell.appendChild(button);
      row.appendChild(cell);
      buttons.push(button);
      gridcells.push(cell);
    }
    container.appendChild(row);
  }

  function render(): void {
    const cursor = grid.index(grid.cursor.x, grid.cursor.y);
    for (let i = 0; i < buttons.length; i++) {
      const button = buttons[i] as HTMLButtonElement;
      const { x, y } = grid.point(i);
      const value = grid.cells[i] as C;
      button.textContent = options.text ? options.text(value, x, y) : "";
      button.setAttribute("aria-label", options.label(value, x, y));
      const extra = options.className ? options.className(value, x, y) : "";
      button.className = extra ? `kit-cell ${extra}` : "kit-cell";
      if (options.pressed) {
        button.setAttribute("aria-pressed", options.pressed(value, x, y) ? "true" : "false");
      }
      button.tabIndex = i === cursor ? 0 : -1;
      if (options.selectable) {
        const selected = grid.isSelected(x, y) ? "true" : "false";
        (gridcells[i] as HTMLElement).setAttribute("aria-selected", selected);
      }
    }
  }

  function focus(): void {
    buttons[grid.index(grid.cursor.x, grid.cursor.y)]?.focus();
  }

  const onKey = (event: KeyboardEvent) => {
    const action = keyAction(event, options.keys);
    if (!action || action === "activate") return; // the focused button turns Enter/Space into a click
    if (NAVIGATION.has(action)) {
      event.preventDefault();
      if (grid.navigate(action)) {
        render();
        focus();
        options.onMove?.(grid.cursor.x, grid.cursor.y);
      }
      return;
    }
    if (options.onAction?.(action, event)) {
      event.preventDefault();
      render();
      return;
    }
    if (action === "undo" || action === "redo") {
      event.preventDefault();
      if (action === "undo" ? grid.undo() : grid.redo()) {
        render();
        options.onChange?.();
      }
    }
  };
  container.addEventListener("keydown", onKey);
  cleanups.push(() => container.removeEventListener("keydown", onKey));

  render();
  return {
    render,
    focus,
    destroy() {
      for (const c of cleanups) c();
      container.textContent = "";
    },
  };
}
