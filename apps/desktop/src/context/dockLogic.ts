// Pure decisions for the splitter between the board and the docked session panel.

/** The docked panel is never narrower than this. */
export const DOCK_MIN_PX = 320;
/** Without a saved width the panel is 40% of the stage, between the minimum and this. */
export const DOCK_DEFAULT_MAX_PX = 400;
/** The docked panel never takes more than this share of the stage. */
export const DOCK_MAX_SHARE = 0.6;
/** One arrow key press moves the splitter this far. */
export const DOCK_KEY_STEP_PX = 16;

/** The widest the panel may be in a stage this wide (never below the minimum). */
export const dockMax = (stageWidth: number) =>
  Math.max(DOCK_MIN_PX, Math.floor(stageWidth * DOCK_MAX_SHARE));

export const clampDock = (px: number, stageWidth: number) =>
  Math.round(Math.min(dockMax(stageWidth), Math.max(DOCK_MIN_PX, px)));

/** The width used when none is saved; matches the CSS default. */
export const defaultDock = (stageWidth: number) =>
  clampDock(Math.min(DOCK_DEFAULT_MAX_PX, stageWidth * 0.4), stageWidth);

/** The panel width to show: the saved one kept within the current stage, or the default. */
export const effectiveDock = (saved: number | null | undefined, stageWidth: number) =>
  typeof saved === "number" && Number.isFinite(saved)
    ? clampDock(saved, stageWidth)
    : defaultDock(stageWidth);

/**
 * The new panel width for a key on the splitter, or null for other keys. The panel is on the
 * right, so Left (toward the board) widens it and Right narrows it.
 */
export function dockForKey(key: string, current: number, stageWidth: number): number | null {
  switch (key) {
    case "ArrowLeft":
    case "ArrowUp":
      return clampDock(current + DOCK_KEY_STEP_PX, stageWidth);
    case "ArrowRight":
    case "ArrowDown":
      return clampDock(current - DOCK_KEY_STEP_PX, stageWidth);
    case "Home":
      return DOCK_MIN_PX;
    case "End":
      return dockMax(stageWidth);
    default:
      return null;
  }
}
