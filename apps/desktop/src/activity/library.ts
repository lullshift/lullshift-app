// Play, Rest and Continue (ADR-026 section 4): which page an activity belongs to, the search and
// the filter chips. Pure functions with no view code, so the tray window can reuse them.
//
// Manifest tags are untrusted strings. They are matched against this fixed, host-owned map and
// searched as text; they never become a chip, a label or any other visible text.

import type { ActivityInfo } from "./ActivityHost";

export type Page = "play" | "rest";

/** The only fields the library logic reads. */
export type Listed = Pick<ActivityInfo, "id" | "name" | "description" | "category" | "savedUtc"> & {
  tags?: readonly string[];
};

export interface TagChip {
  id: string;
  /** Our own wording, never taken from a manifest. */
  label: string;
  /** Manifest tags that put an activity under this chip. */
  tags: readonly string[];
}

export const PLAY_CHIPS: readonly TagChip[] = [
  { id: "puzzles", label: "Puzzles", tags: ["puzzle", "logic", "teaser"] },
  { id: "calm", label: "Calm", tags: ["calm"] },
  { id: "quizzes", label: "Quizzes", tags: ["quiz", "trivia"] },
  { id: "typing", label: "Typing", tags: ["typing"] },
  { id: "toys", label: "Toys", tags: ["toy", "toys", "arcade", "sandbox"] },
  { id: "cards", label: "Cards", tags: ["cards"] },
];

export const REST_CHIPS: readonly TagChip[] = [
  { id: "breathing", label: "Breathing", tags: ["breathing"] },
  { id: "movement", label: "Movement", tags: ["movement", "stretch"] },
  { id: "step-away", label: "Step away", tags: ["away", "outside", "outdoors", "walk"] },
  { id: "reflection", label: "Reflection", tags: ["reflection"] },
  { id: "pause", label: "Pause", tags: ["pause", "noticing"] },
  { id: "eyes", label: "Eyes", tags: ["eyes"] },
];

/** `break` is Rest; everything else (arcade, interactive, productivity) is Play. */
export function pageOf(category: string): Page {
  return category === "break" ? "rest" : "play";
}

/** The help link and dialog title follow the page: games are played, breaks are not. */
export function helpLabel(category: string): "How to play" | "How it works" {
  return pageOf(category) === "rest" ? "How it works" : "How to play";
}

const KINDS: Record<string, string> = {
  break: "Rest",
  arcade: "Game",
  productivity: "Focus",
  interactive: "Play",
};

/** A short word for what an activity is, from the manifest category. */
export function kindLabel(category: string): string {
  return KINDS[category] ?? "Play";
}

export const chipsFor = (page: Page): readonly TagChip[] =>
  page === "rest" ? REST_CHIPS : PLAY_CHIPS;

export function onPage<T extends Listed>(list: readonly T[], page: Page): T[] {
  return list.filter((a) => pageOf(a.category) === page);
}

/** Case-insensitive match on name, description and tags. An empty query matches everything. */
export function matchesQuery(a: Listed, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  return (
    a.name.toLowerCase().includes(q) ||
    a.description.toLowerCase().includes(q) ||
    (a.tags ?? []).some((t) => t.toLowerCase().includes(q))
  );
}

function inChip(a: Listed, chip: TagChip): boolean {
  return (a.tags ?? []).some((t) => chip.tags.includes(t));
}

/** The page's activities that match the search and the chip (null chip = all). */
export function filterLibrary<T extends Listed>(
  list: readonly T[],
  page: Page,
  query: string,
  chipId: string | null,
): T[] {
  const chip = chipId ? chipsFor(page).find((c) => c.id === chipId) : undefined;
  return onPage(list, page).filter(
    (a) => matchesQuery(a, query) && (chip === undefined || inChip(a, chip)),
  );
}

/** Chips with at least one activity, so none leads to an empty page. */
export function availableChips<T extends Listed>(list: readonly T[], page: Page): TagChip[] {
  const items = onPage(list, page);
  return chipsFor(page).filter((chip) => items.some((a) => inChip(a, chip)));
}

/** Activities with a saved state, newest save first. Unreadable times sort last. */
export function savedNewestFirst<T extends Pick<ActivityInfo, "savedUtc">>(
  list: readonly T[],
): T[] {
  const time = (a: T) => {
    const t = a.savedUtc ? Date.parse(a.savedUtc) : NaN;
    return Number.isNaN(t) ? -Infinity : t;
  };
  return list
    .filter((a) => a.savedUtc)
    .sort((a, b) => (time(a) === time(b) ? 0 : time(b) > time(a) ? 1 : -1));
}
