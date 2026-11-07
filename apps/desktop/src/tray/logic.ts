// The tray window's choices, pure so they are unit-tested (ADR-026 section 6). It reuses
// suggest.ts for the suggestion and home/status.ts for the words; nothing here keeps a history.

import { pageOf, type Listed } from "../activity/library";
import { providerName, relativeTime, sessionName } from "../format";
import { liveState, type SessionView } from "../sessions";
import { suggestionOrder, type SuggestInput } from "../suggest";

/** At most this many quick chips under the suggestion. */
export const CHIPS_MAX = 3;

/**
 * Up to three other things to try, not the suggestion itself: a rest, a game, then whatever comes
 * next in Shuffle's order, so the chips are not three of a kind. Local and deterministic (same
 * date seed as Shuffle).
 */
export function quickChips<T extends Listed>(input: SuggestInput<T>, shownId: string | null): T[] {
  const rest = suggestionOrder(input)
    .map((s) => s.activity)
    .filter((a) => a.id !== shownId);
  const firstRest = rest.find((a) => pageOf(a.category) === "rest");
  const firstPlay = rest.find((a) => pageOf(a.category) === "play");
  const picked: T[] = [firstRest, firstPlay].filter((a): a is T => a !== undefined);
  for (const a of rest) {
    if (picked.length >= CHIPS_MAX) break;
    if (!picked.includes(a)) picked.push(a);
  }
  return picked.slice(0, CHIPS_MAX);
}

/** Who needs you, for the card: the tool when there is one session, the tools when several. */
export function needsWho(needs: readonly SessionView[], now: number): string {
  const [first] = needs;
  if (!first) return "";
  if (needs.length > 1) {
    return [...new Set(needs.map((s) => providerName(s.provider)))].join(", ");
  }
  const name = sessionName(first);
  const since = relativeTime(first.lastSeenUtc, now);
  return name.source === "tool" ? `Waiting for you · ${since}` : `${name.primary} · ${since}`;
}

export function needing(sessions: readonly SessionView[]): SessionView[] {
  return sessions.filter((s) => liveState(s) === "needs_attention");
}
