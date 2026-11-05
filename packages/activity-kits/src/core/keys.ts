// Keys to actions (ADR-019). Named keys use `event.key`, so they work on every layout. Undo and
// redo use the letter the player typed (`key`), which is where the shortcut is printed on their
// keyboard. Optional WASD movement uses `event.code`, the physical position, so it is ZQSD on
// AZERTY. Alt combinations and other Ctrl/Meta shortcuts are left to the browser.

export type KeyAction =
  | "up"
  | "down"
  | "left"
  | "right"
  | "home"
  | "end"
  | "first"
  | "last"
  | "pageUp"
  | "pageDown"
  | "activate"
  | "cancel"
  | "undo"
  | "redo";

/** The parts of a KeyboardEvent the map reads. */
export interface KeyInput {
  key: string;
  code?: string;
  ctrlKey?: boolean;
  metaKey?: boolean;
  shiftKey?: boolean;
  altKey?: boolean;
}

export interface KeyOptions {
  /** Move with the W, A, S and D positions as well as the arrows. Off by default. */
  wasd?: boolean;
}

const NAMED: Record<string, KeyAction> = {
  ArrowUp: "up",
  ArrowDown: "down",
  ArrowLeft: "left",
  ArrowRight: "right",
  Up: "up",
  Down: "down",
  Left: "left",
  Right: "right",
  Home: "home",
  End: "end",
  PageUp: "pageUp",
  PageDown: "pageDown",
  Enter: "activate",
  " ": "activate",
  Spacebar: "activate",
  Escape: "cancel",
  Esc: "cancel",
};

const WASD: Record<string, KeyAction> = {
  KeyW: "up",
  KeyA: "left",
  KeyS: "down",
  KeyD: "right",
};

/**
 * The action for a key press, or null when the kit has no use for it (let it through then).
 * Ctrl/Cmd+Z is undo; Ctrl/Cmd+Y and Ctrl/Cmd+Shift+Z are redo; Ctrl/Cmd+Home and +End are
 * "first" and "last".
 */
export function keyAction(event: KeyInput, options: KeyOptions = {}): KeyAction | null {
  if (event.altKey) return null;
  const mod = !!(event.ctrlKey || event.metaKey);
  if (mod) {
    const k = event.key.toLowerCase();
    if (k === "z") return event.shiftKey ? "redo" : "undo";
    if (k === "y" && !event.shiftKey) return "redo";
    if (event.key === "Home") return "first";
    if (event.key === "End") return "last";
    return null;
  }
  const named = own(NAMED, event.key);
  if (named) return named;
  if (options.wasd && event.code && !event.shiftKey) return own(WASD, event.code);
  return null;
}

function own(map: Record<string, KeyAction>, key: string): KeyAction | null {
  return Object.prototype.hasOwnProperty.call(map, key) ? (map[key] as KeyAction) : null;
}
