// Follow-the-newest-text for the message list. On: the list keeps itself at the bottom while text
// streams in, until the user scrolls up; it follows again once they come back to the bottom.
// Off: it never moves by itself.

import {
  useCallback,
  useLayoutEffect,
  useRef,
  type KeyboardEvent,
  type RefObject,
  type UIEvent,
  type WheelEvent,
} from "react";

/** Within this many pixels of the end counts as "at the bottom". */
export const BOTTOM_SLACK_PX = 24;

export function isAtBottom(el: Pick<HTMLElement, "scrollHeight" | "scrollTop" | "clientHeight">) {
  return el.scrollHeight - el.scrollTop - el.clientHeight <= BOTTOM_SLACK_PX;
}

/**
 * `signal` changes whenever new content arrives. Returns handlers for the scroll container and
 * `resume` (call it when the user sends a message: with auto-scroll on it follows the new reply;
 * with it off it scrolls once to show the message you just sent, and streaming does not follow).
 */
export function useFollowScroll(
  ref: RefObject<HTMLElement | null>,
  enabled: boolean,
  signal: string,
) {
  const following = useRef(true);
  const lastTop = useRef(0);
  const revealOnce = useRef(false);
  const enabledRef = useRef(enabled);
  useLayoutEffect(() => {
    enabledRef.current = enabled;
  }, [enabled]);

  const toBottom = useCallback(() => {
    const el = ref.current;
    if (!el) return;
    el.scrollTop = el.scrollHeight;
    lastTop.current = el.scrollTop;
  }, [ref]);

  // Turning it on follows again from now.
  useLayoutEffect(() => {
    if (!enabled) return;
    following.current = true;
    toBottom();
  }, [enabled, toBottom]);

  useLayoutEffect(() => {
    if (enabled && following.current) toBottom();
    else if (!enabled && revealOnce.current) {
      revealOnce.current = false;
      toBottom();
    }
  }, [enabled, signal, toBottom]);

  const onScroll = useCallback((e: UIEvent<HTMLElement>) => {
    const el = e.currentTarget;
    // Any upward move pauses following (even a few pixels); new content makes the list longer
    // but never moves the top, so growth can't be mistaken for the user leaving. Coming back to
    // the bottom resumes.
    const up = el.scrollTop < lastTop.current - 1;
    if (up) following.current = false;
    else if (isAtBottom(el)) following.current = true;
    lastTop.current = el.scrollTop;
  }, []);

  // Wheel up and the keys that scroll up pause at once, before a scroll event arrives.
  const onWheel = useCallback((e: WheelEvent<HTMLElement>) => {
    if (e.deltaY < 0) following.current = false;
  }, []);
  const onKeyDown = useCallback((e: KeyboardEvent<HTMLElement>) => {
    if (e.target !== e.currentTarget) return;
    if (e.key === "ArrowUp" || e.key === "PageUp" || e.key === "Home") following.current = false;
  }, []);

  const resume = useCallback(() => {
    following.current = true;
    if (!enabledRef.current) revealOnce.current = true;
  }, []);

  return { onScroll, onWheel, onKeyDown, resume };
}
