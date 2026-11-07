// The tray window's request to the main window (ADR-026 section 6). Rust validates it (view from
// a fixed list, activity installed and enabled, session existing) and emits `open-main` to the
// main window only; this reads the payload defensively anyway, and ignores anything odd.

import { listen } from "@tauri-apps/api/event";
import type { SessionKey } from "./context/api";

export const OPEN_MAIN_VIEWS = [
  "home",
  "sessions",
  "play",
  "rest",
  "continue",
  "companion",
] as const;
export type OpenMainView = (typeof OPEN_MAIN_VIEWS)[number];

export interface OpenMainEvent {
  view: OpenMainView;
  activityId?: string;
  sessionKey?: SessionKey;
}

/** The event's payload as a typed request, or null if it isn't one. */
export function parseOpenMain(payload: unknown): OpenMainEvent | null {
  if (typeof payload !== "object" || payload === null) return null;
  const p = payload as Record<string, unknown>;
  const view = OPEN_MAIN_VIEWS.find((v) => v === p.view);
  if (!view) return null;
  const out: OpenMainEvent = { view };
  if (p.activityId !== undefined) {
    if (typeof p.activityId !== "string" || p.activityId === "") return null;
    out.activityId = p.activityId;
  }
  if (p.sessionKey !== undefined) {
    const k = p.sessionKey as Record<string, unknown> | null;
    if (
      typeof k !== "object" ||
      k === null ||
      typeof k.provider !== "string" ||
      typeof k.sessionId !== "string"
    ) {
      return null;
    }
    out.sessionKey = { provider: k.provider, sessionId: k.sessionId };
  }
  return out;
}

/** Calls `handler` for each request from the tray window. Returns the unsubscribe. */
export function onOpenMain(handler: (r: OpenMainEvent) => void): () => void {
  let unlisten: (() => void) | undefined;
  let disposed = false;
  listen<unknown>("open-main", (event) => {
    const request = parseOpenMain(event.payload);
    if (request) handler(request);
  })
    .then((fn) => (disposed ? fn() : (unlisten = fn)))
    .catch(() => undefined);
  return () => {
    disposed = true;
    unlisten?.();
  };
}
