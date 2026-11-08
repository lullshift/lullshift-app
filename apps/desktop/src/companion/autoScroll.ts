// Follow-the-newest-text for the message list. On: the list keeps itself at the bottom while text
// streams in, until the user scrolls up; it follows again once they come back to the bottom.
// Off: it never moves by itself.

import { useCallback, useEffect, useRef, type RefObject, type UIEvent } from "react";

/** Within this many pixels of the end counts as "at the bottom". */
export const BOTTOM_SLACK_PX = 24;

export function isAtBottom(el: Pick<HTMLElement, "scrollHeight" | "scrollTop" | "clientHeight">) {
  return el.scrollHeight - el.scrollTop - el.clientHeight <= BOTTOM_SLACK_PX;
}

/**
 * `signal` changes whenever new content arrives. Returns `onScroll` for the container and
 * `resume` (call it when the user sends a message, to follow the new reply).
 */
export function useFollowScroll(
  ref: RefObject<HTMLElement | null>,
  enabled: boolean,
  signal: string,
) {
  const following = useRef(true);
  const lastTop = useRef(0);

  const toBottom = useCallback(() => {
    const el = ref.current;
    if (!el) return;
    el.scrollTop = el.scrollHeight;
    lastTop.current = el.scrollTop;
  }, [ref]);

  // Turning it on follows again from now.
  useEffect(() => {
    if (!enabled) return;
    following.current = true;
    toBottom();
  }, [enabled, toBottom]);

  useEffect(() => {
    if (enabled && following.current) toBottom();
  }, [enabled, signal, toBottom]);

  const onScroll = useCallback((e: UIEvent<HTMLElement>) => {
    const el = e.currentTarget;
    // Only an upward move pauses following: new content makes the list longer but never moves
    // the top, so growth can't be mistaken for the user leaving.
    if (el.scrollTop < lastTop.current - 1) following.current = false;
    if (isAtBottom(el)) following.current = true;
    lastTop.current = el.scrollTop;
  }, []);

  const resume = useCallback(() => {
    following.current = true;
  }, []);

  return { onScroll, resume };
}
