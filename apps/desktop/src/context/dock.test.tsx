// @vitest-environment jsdom
// The splitter between the board and the docked session panel: sizes stay in range and the
// separator works from the keyboard.

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DockSplitter } from "./DockSplitter";
import {
  DOCK_KEY_STEP_PX,
  DOCK_MIN_PX,
  clampDock,
  defaultDock,
  dockForKey,
  dockMax,
  effectiveDock,
} from "./dockLogic";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

describe("dock sizes", () => {
  it("keeps the width between the minimum and 60% of the stage", () => {
    expect(clampDock(100, 1000)).toBe(DOCK_MIN_PX);
    expect(clampDock(900, 1000)).toBe(600);
    expect(clampDock(500, 1000)).toBe(500);
    expect(dockMax(1000)).toBe(600);
  });

  it("never lets the maximum fall under the minimum in a small stage", () => {
    expect(dockMax(400)).toBe(DOCK_MIN_PX);
    expect(clampDock(380, 400)).toBe(DOCK_MIN_PX);
  });

  it("defaults to 40% of the stage between 320 and 400", () => {
    expect(defaultDock(1000)).toBe(400);
    expect(defaultDock(2000)).toBe(400);
    expect(defaultDock(700)).toBe(320);
  });

  it("uses a saved width within the current stage and the default otherwise", () => {
    expect(effectiveDock(null, 1000)).toBe(400);
    expect(effectiveDock(undefined, 1000)).toBe(400);
    expect(effectiveDock(550, 1000)).toBe(550);
    expect(effectiveDock(550, 700)).toBe(420);
    expect(effectiveDock(Number.NaN, 1000)).toBe(400);
  });

  it("moves 16px per arrow key (left widens), Home and End jump to the ends", () => {
    expect(dockForKey("ArrowLeft", 400, 1000)).toBe(400 + DOCK_KEY_STEP_PX);
    expect(dockForKey("ArrowRight", 400, 1000)).toBe(400 - DOCK_KEY_STEP_PX);
    expect(dockForKey("ArrowRight", DOCK_MIN_PX, 1000)).toBe(DOCK_MIN_PX);
    expect(dockForKey("Home", 400, 1000)).toBe(DOCK_MIN_PX);
    expect(dockForKey("End", 400, 1000)).toBe(600);
    expect(dockForKey("a", 400, 1000)).toBeNull();
  });
});

describe("DockSplitter", () => {
  let host: HTMLDivElement;
  let root: Root;
  const onPreview = vi.fn();
  const onCommit = vi.fn();

  beforeEach(() => {
    onPreview.mockClear();
    onCommit.mockClear();
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    act(() => {
      root.render(
        <DockSplitter value={400} stageWidth={1000} onPreview={onPreview} onCommit={onCommit} />,
      );
    });
  });
  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  const sep = () => host.querySelector<HTMLElement>('[role="separator"]')!;
  const key = (k: string) =>
    act(() => {
      sep().dispatchEvent(
        new KeyboardEvent("keydown", { key: k, bubbles: true, cancelable: true }),
      );
    });

  it("is a focusable vertical separator that reports its range", () => {
    expect(sep().getAttribute("aria-orientation")).toBe("vertical");
    expect(sep().tabIndex).toBe(0);
    expect(sep().getAttribute("aria-valuenow")).toBe("400");
    expect(sep().getAttribute("aria-valuemin")).toBe("320");
    expect(sep().getAttribute("aria-valuemax")).toBe("600");
  });

  it("saves the new width for arrow keys, Home and End", () => {
    key("ArrowLeft");
    expect(onCommit).toHaveBeenLastCalledWith(416);
    key("ArrowRight");
    expect(onCommit).toHaveBeenLastCalledWith(384);
    key("Home");
    expect(onCommit).toHaveBeenLastCalledWith(320);
    key("End");
    expect(onCommit).toHaveBeenLastCalledWith(600);
  });

  it("goes back to the default on double-click", () => {
    act(() => {
      sep().dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
    });
    expect(onCommit).toHaveBeenCalledWith(null);
  });
});
