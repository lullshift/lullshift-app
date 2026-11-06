import type { RefObject } from "react";

/** Moves focus to `target` (which needs tabIndex={-1}). Exported so it can be tested. */
export function skipTo(target: { focus: () => void } | null | undefined): boolean {
  if (!target) return false;
  target.focus();
  return true;
}

/**
 * First focusable element on the page, hidden until it has focus. Jumps past the navigation to
 * the main landmark, which takes focus.
 */
export function SkipLink({
  targetId,
  targetRef,
}: {
  targetId: string;
  targetRef: RefObject<HTMLElement | null>;
}) {
  return (
    <a
      className="skip-link"
      href={`#${targetId}`}
      onClick={(e) => {
        e.preventDefault();
        skipTo(targetRef.current);
      }}
    >
      Skip to main content
    </a>
  );
}
