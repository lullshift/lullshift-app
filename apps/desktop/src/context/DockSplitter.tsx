import { useRef, type KeyboardEvent, type PointerEvent } from "react";
import { DOCK_MIN_PX, clampDock, dockForKey, dockMax } from "./dockLogic";

interface Props {
  /** The current panel width in pixels. */
  value: number;
  /** The width of the area holding the board and the panel. */
  stageWidth: number;
  /** While dragging; nothing is saved yet. */
  onPreview: (px: number | null) => void;
  /** Dragging ended or a key moved it; a number is saved, null goes back to the default. */
  onCommit: (px: number | null) => void;
}

/**
 * A vertical splitter between the board and the docked panel. Drag it, or focus it and use the
 * arrow keys (16px), Home and End; double-click goes back to the default width.
 */
export function DockSplitter({ value, stageWidth, onPreview, onCommit }: Props) {
  const drag = useRef<{ startX: number; startWidth: number; last: number } | null>(null);

  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.currentTarget.setPointerCapture?.(e.pointerId);
    drag.current = { startX: e.clientX, startWidth: value, last: value };
    e.preventDefault();
  };
  const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    if (!d) return;
    // The panel is on the right: moving left makes it wider.
    d.last = clampDock(d.startWidth + (d.startX - e.clientX), stageWidth);
    onPreview(d.last);
  };
  const end = () => {
    const d = drag.current;
    drag.current = null;
    if (d && d.last !== d.startWidth) onCommit(d.last);
    onPreview(null);
  };
  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const next = dockForKey(e.key, value, stageWidth);
    if (next === null) return;
    e.preventDefault();
    if (next !== value) onCommit(next);
  };

  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label="Resize the session panel"
      aria-valuenow={value}
      aria-valuemin={DOCK_MIN_PX}
      aria-valuemax={dockMax(stageWidth)}
      tabIndex={0}
      className="dock-splitter"
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={end}
      onPointerCancel={end}
      onDoubleClick={() => onCommit(null)}
      onKeyDown={onKeyDown}
    />
  );
}
