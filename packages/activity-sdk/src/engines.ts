// Pack data types for the bundled engines whose packs can draw rounds (ADR-016, ADR-028
// section 10). The engine's data.schema.json is the authority; these types follow it.

/** A quiz question picture: a file listed in the pack's `images`, and alt text. */
export interface QuizImage {
  src: string;
  alt: string;
}

export interface QuizQuestion {
  prompt: string;
  /** 2 to 5 choices. */
  choices: string[];
  /** Index of the right choice. */
  answer: number;
  explanation: string;
  image?: QuizImage;
}

/** Data for a `lullshift.quiz@1` pack. */
export interface QuizPackData {
  title: string;
  intro?: string;
  shuffle?: boolean;
  /** Questions per round (3 to 40, at most the question count). Leave out to ask them all. */
  roundSize?: number;
  /** 1 to 40 questions. */
  questions: QuizQuestion[];
}

export type TeaserKind = "logic" | "pattern" | "wordplay" | "math";

export interface Teaser {
  question: string;
  hint?: string;
  answer: string;
  explanation: string;
  kind?: TeaserKind;
}

/** Data for a `lullshift.teaser@1` pack. */
export interface TeaserPackData {
  title: string;
  intro?: string;
  /** Teasers per round (3 to 40, at most the teaser count). Leave out to show them all. */
  roundSize?: number;
  /** 1 to 40 teasers. */
  teasers: Teaser[];
}

export interface CalmCard {
  front: string;
  back?: string;
}

/** Data for a `lullshift.cards@1` (Calm Cards) pack. */
export interface CardsPackData {
  title: string;
  intro?: string;
  /** Cards per round (3 to 60, at most the card count). Leave out to show the whole deck. */
  roundSize?: number;
  /** 1 to 60 cards. */
  cards: CalmCard[];
}

/** The smallest `roundSize` a pack may set. */
export const ROUND_SIZE_MIN = 3;

/** Engines whose packs may set `roundSize`, and the list each round is drawn from. */
export const ROUND_ENGINES: Readonly<Record<string, string>> = Object.freeze({
  "lullshift.quiz@1": "questions",
  "lullshift.teaser@1": "teasers",
  "lullshift.cards@1": "cards",
});

/**
 * The rule the engine schemas can't state: `roundSize` is at most the number of items. Returns
 * a plain-words problem when a pack for `engine` (`id@major`) breaks it, otherwise null. The app
 * doesn't refuse such a pack; the engine plays the whole pack every round, so creators get this
 * as a warning.
 */
export function roundSizeProblem(engine: string, data: unknown): string | null {
  const list = ROUND_ENGINES[engine];
  if (!list || !data || typeof data !== "object" || Array.isArray(data)) return null;
  const d = data as Record<string, unknown>;
  const size = d.roundSize;
  const items = d[list];
  if (!Number.isInteger(size) || !Array.isArray(items)) return null;
  if ((size as number) <= items.length) return null;
  return (
    `roundSize ${String(size)} is more than the ${items.length} ${list}, so every round ` +
    `shows all of them (set roundSize to at most ${items.length}, or leave it out)`
  );
}
