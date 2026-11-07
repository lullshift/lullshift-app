import { invoke } from "@tauri-apps/api/core";
import type { SessionKey } from "../context/api";

/** The views the tray may ask the main window to open (a fixed list, checked again in Rust). */
export type MainView = "home" | "sessions" | "play" | "rest" | "continue" | "companion";

export interface OpenMainRequest {
  view: MainView;
  /** Start this activity in the main window (an installed, enabled one; Rust checks). */
  activityId?: string;
  /** Open this session's panel (an existing one; Rust checks). */
  sessionKey?: SessionKey;
}

/** Shows the main window and hides this one. Activities never run in the tray window. */
export const openMain = (r: OpenMainRequest) =>
  invoke<null>("open_main", {
    view: r.view,
    activityId: r.activityId ?? null,
    sessionKey: r.sessionKey ?? null,
  });

export const hideTray = () => invoke<null>("tray_hide");
