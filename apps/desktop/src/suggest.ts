// "Fits your wait" (ADR-026 section 3): which activity to suggest, and the words that go with it.
// Pure and local: no React, no invoke, no clock. The tray window reuses this module, so keep it
// that way.
//
// The estimate is a fact about agents (how long a tool's working stretches usually last), never
// about the person. Nothing here keeps a score, a count or a history.

import { savedNewestFirst, pageOf, type Listed, type Page } from "./activity/library";
import { providerName } from "./format";

/** One tool's typical working stretch, from the Rust `wait_estimates` command. */
export interface WaitEstimate {
  provider: string;
  medianSeconds: number;
  stretches: number;
}

/** At or above this, the wait is long enough for a game; below it, a rest fits better. */
export const PLAY_FROM_SECONDS = 4 * 60;

export type SuggestionKind = "resume" | "rest" | "play";

export interface Suggestion<T> {
  activity: T;
  kind: SuggestionKind;
}

/** A number for a calendar day (local), so the order is the same all day and changes tomorrow. */
export function dateSeed(d: Date): number {
  return d.getFullYear() * 10_000 + (d.getMonth() + 1) * 100 + d.getDate();
}

function hash(text: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return h >>> 0;
}

/** A stable order for `seed`: the same input gives the same order on every render. */
export function seededOrder<T extends { id: string }>(list: readonly T[], seed: number): T[] {
  return list
    .map((item) => ({ item, key: hash(`${seed}:${item.id}`) }))
    .sort((a, b) => a.key - b.key || a.item.id.localeCompare(b.item.id))
    .map((x) => x.item);
}

/**
 * The estimate to talk about: the first working tool (callers pass them most recent first) that
 * has one. Null when no working tool has enough history.
 */
export function waitFor(
  workingProviders: readonly string[],
  estimates: readonly WaitEstimate[],
): WaitEstimate | null {
  for (const p of workingProviders) {
    const e = estimates.find((x) => x.provider === p);
    if (e) return e;
  }
  return null;
}

/** Rest unless an agent is working and its usual stretch is 4 minutes or more. */
export function preferredPage(estimateSeconds: number | null, anyWorking: boolean): Page {
  return anyWorking && estimateSeconds !== null && estimateSeconds >= PLAY_FROM_SECONDS
    ? "play"
    : "rest";
}

export interface SuggestInput<T> {
  /** Enabled, installed activities (what `activities_list` returns). */
  activities: readonly T[];
  estimateSeconds: number | null;
  anyWorking: boolean;
  seed: number;
}

/**
 * Every candidate, in the order Shuffle walks them. The newest saved activity comes first; then
 * the preferred page's activities in the date-seeded order, then the other page's.
 */
export function suggestionOrder<T extends Listed>(input: SuggestInput<T>): Suggestion<T>[] {
  const { activities, estimateSeconds, anyWorking, seed } = input;
  const want = preferredPage(estimateSeconds, anyWorking);
  const rest = (page: Page) =>
    seededOrder(
      activities.filter((a) => pageOf(a.category) === page),
      seed,
    );
  const newest = savedNewestFirst(activities)[0];
  const ordered = [
    ...(newest ? [newest] : []),
    ...rest(want),
    ...rest(want === "play" ? "rest" : "play"),
  ];
  const seen = new Set<string>();
  const out: Suggestion<T>[] = [];
  for (const activity of ordered) {
    if (seen.has(activity.id)) continue;
    seen.add(activity.id);
    out.push({
      activity,
      kind: activity.savedUtc ? "resume" : pageOf(activity.category) === "rest" ? "rest" : "play",
    });
  }
  return out;
}

/** The suggestion after pressing Shuffle `shuffles` times; wraps around. Null if none. */
export function pickSuggestion<T extends Listed>(
  input: SuggestInput<T>,
  shuffles: number,
): Suggestion<T> | null {
  const order = suggestionOrder(input);
  if (order.length === 0) return null;
  return order[((shuffles % order.length) + order.length) % order.length] ?? null;
}

/**
 * The line above the suggestion. With an estimate: "Claude Code usually takes about 6 min".
 * Without: "For a short break", or "If you'd like a break" when no agent is working. Never a
 * finish time and never a countdown.
 */
export function waitLine(estimate: WaitEstimate | null, anyWorking: boolean): string {
  if (estimate && anyWorking) {
    const minutes = Math.max(1, Math.round(estimate.medianSeconds / 60));
    return `${providerName(estimate.provider)} usually takes about ${minutes} min`;
  }
  return anyWorking ? "For a short break" : "If you'd like a break";
}

/** The button for a suggestion: Resume, Rest or Play. */
export function suggestionAction(kind: SuggestionKind): "Resume" | "Rest" | "Play" {
  return kind === "resume" ? "Resume" : kind === "rest" ? "Rest" : "Play";
}
