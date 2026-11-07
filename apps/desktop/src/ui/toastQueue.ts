// A small in-app notice queue. Notices are non-modal, never take focus and carry a short title
// only, so nothing private (a reply, a prompt) is ever put in one. See ui/Toasts.tsx.

import { useSyncExternalStore } from "react";

export type ToastTone = "info" | "danger" | "attention";

export interface ToastInput {
  /** The same key replaces the notice that is already showing (and restarts its timer). */
  key: string;
  title: string;
  /** Text inside `title` that comes from the user (a conversation name); shown isolated. */
  subject?: string;
  tone: ToastTone;
  /** Only for something that blocks progress, such as an approval. Everything else is polite. */
  assertive?: boolean;
  action?: { label: string; onAction: () => void };
}

export interface Toast extends ToastInput {
  id: number;
}

/** At most this many show at once; the oldest goes first. */
export const MAX_TOASTS = 3;
/** Notices go away by themselves after this long, unless hovered or focused. */
export const TOAST_MS = 8_000;

let toasts: readonly Toast[] = [];
let nextId = 1;
const listeners = new Set<() => void>();

function commit(next: readonly Toast[]): void {
  toasts = next;
  for (const fn of [...listeners]) fn();
}

function subscribe(fn: () => void): () => void {
  listeners.add(fn);
  return () => {
    listeners.delete(fn);
  };
}

export function pushToast(input: ToastInput): void {
  const kept = toasts.filter((t) => t.key !== input.key);
  const next = [...kept, { ...input, id: nextId++ }];
  commit(next.slice(Math.max(0, next.length - MAX_TOASTS)));
}

export function dismissToast(id: number): void {
  if (toasts.some((t) => t.id === id)) commit(toasts.filter((t) => t.id !== id));
}

export function dismissToastByKey(key: string): void {
  if (toasts.some((t) => t.key === key)) commit(toasts.filter((t) => t.key !== key));
}

export function getToasts(): readonly Toast[] {
  return toasts;
}

export function useToasts(): readonly Toast[] {
  return useSyncExternalStore(subscribe, getToasts, getToasts);
}

/** For tests. */
export function resetToasts(): void {
  commit([]);
}
