// Card kit DOM binding (ADR-019): each pile is a labelled group of card buttons, with a single
// tab stop that follows the table's focus (roving tabindex). An empty pile shows one "Empty"
// button so it can still be chosen as a target. Text is set with textContent only.

import { keyAction, type KeyAction, type KeyOptions } from "../core/keys.ts";
import { cardLabel, type ActivateResult, type Card, type Pile, type Table } from "./cards.ts";

export interface TableBindingOptions {
  /** Accessible name of the whole table, e.g. "Solitaire table". */
  name: string;
  /** Spoken label of a card (default `cardLabel`). "picked up" is added for the selection. */
  label?(card: Card, pile: Pile): string;
  /** Visible text of a card (default: the label for face-up cards, nothing for face-down). */
  text?(card: Card, pile: Pile): string;
  /** Extra class names for a card's button. */
  className?(card: Card, pile: Pile): string;
  /**
   * Enter, Space or a click on a card (index -1 for an empty pile); focus is already there.
   * Return true if you handled it; otherwise the table's pick-up-and-place `activate()` runs.
   */
  onActivate?(pileId: string, cardIndex: number): boolean | void;
  /** What the built-in `activate()` did, so the game can announce it. */
  onResult?(result: ActivateResult): void;
  /** Return true if you handled an action (undo, redo, cancel). */
  onAction?(action: KeyAction, event: KeyboardEvent): boolean | void;
  /** The binding changed the piles (a move, undo or redo): save and update your status. */
  onChange?(): void;
  keys?: KeyOptions;
}

export interface TableBinding {
  /** Rebuilds the piles from the model. Call after every change. Keeps focus inside. */
  render(): void;
  /** Moves keyboard focus to the table's focused card. */
  focus(): void;
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

/** Renders `table` into `container` (an empty element). */
export function bindTable(
  container: HTMLElement,
  table: Table,
  options: TableBindingOptions,
): TableBinding {
  const doc = container.ownerDocument;
  let current: HTMLButtonElement | null = null;
  let first: HTMLButtonElement | null = null;
  const label = (card: Card, pile: Pile) =>
    options.label ? options.label(card, pile) : cardLabel(card);

  container.setAttribute("role", "group");
  container.setAttribute("aria-label", options.name);

  function activate(pile: number, card: number): void {
    table.focus = { pile, card };
    const p = table.piles[pile] as Pile;
    const before = JSON.stringify(table.piles.map((x) => x.cards));
    if (!options.onActivate?.(p.id, card)) {
      const result = table.activate();
      options.onResult?.(result);
    }
    render();
    focus();
    if (JSON.stringify(table.piles.map((x) => x.cards)) !== before) options.onChange?.();
  }

  function cardButton(text: string, aria: string, pile: number, card: number): HTMLButtonElement {
    const b = doc.createElement("button");
    b.type = "button";
    b.textContent = text;
    b.setAttribute("aria-label", aria);
    const focused = table.focus.pile === pile && table.focus.card === card;
    b.tabIndex = focused ? 0 : -1;
    if (focused) current = b;
    first ??= b;
    b.addEventListener("click", () => activate(pile, card));
    return b;
  }

  function render(): void {
    const hadFocus = container.contains(doc.activeElement);
    current = null;
    first = null;
    container.textContent = "";
    const sel = table.selection;
    table.piles.forEach((pile, p) => {
      const group = doc.createElement("div");
      group.setAttribute("role", "group");
      const n = pile.cards.length;
      group.setAttribute("aria-label", `${pile.name}, ${n === 1 ? "1 card" : `${n} cards`}`);
      group.className = "kit-pile";
      group.setAttribute("data-pile", pile.id);
      if (n === 0) {
        const b = cardButton("", `Empty ${pile.name}`, p, -1);
        b.className = "kit-card kit-empty";
        group.appendChild(b);
      }
      pile.cards.forEach((card, c) => {
        const picked = !!sel && sel.pile === p && c >= sel.card;
        const aria = label(card, pile) + (picked ? ", picked up" : "");
        const text = options.text ? options.text(card, pile) : card.faceUp ? label(card, pile) : "";
        const b = cardButton(text, aria, p, c);
        const extra = options.className ? options.className(card, pile) : "";
        b.className = [
          "kit-card",
          card.faceUp ? "kit-face-up" : "kit-face-down",
          picked ? "kit-picked" : "",
          extra,
        ]
          .filter(Boolean)
          .join(" ");
        b.setAttribute("aria-pressed", picked ? "true" : "false");
        group.appendChild(b);
      });
      container.appendChild(group);
    });
    // cardButton sets these; TypeScript can't see that and narrows them to null.
    const rendered = current as HTMLButtonElement | null;
    const fallback = first as HTMLButtonElement | null;
    if (!rendered && fallback) {
      // table.focus points at nothing rendered (e.g. changed by the game): keep one tab stop.
      fallback.tabIndex = 0;
      current = fallback;
    }
    if (hadFocus) focus();
  }

  function focus(): void {
    current?.focus();
  }

  const onKey = (event: KeyboardEvent) => {
    const action = keyAction(event, options.keys);
    if (!action || action === "activate") return; // the focused button turns Enter/Space into a click
    if (NAVIGATION.has(action)) {
      event.preventDefault();
      if (table.navigate(action)) {
        render();
        focus();
      }
      return;
    }
    if (options.onAction?.(action, event)) {
      event.preventDefault();
      render();
      return;
    }
    if (action === "cancel" && table.selection) {
      event.preventDefault();
      table.selection = null;
      render();
      focus();
      return;
    }
    if (action === "undo" || action === "redo") {
      event.preventDefault();
      if (action === "undo" ? table.undo() : table.redo()) {
        render();
        focus();
        options.onChange?.();
      }
    }
  };
  container.addEventListener("keydown", onKey);

  render();
  return {
    render,
    focus,
    destroy() {
      container.removeEventListener("keydown", onKey);
      container.textContent = "";
    },
  };
}
