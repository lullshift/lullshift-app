// Seeded random numbers (ADR-019). The generator is mulberry32 (Tommy Ettinger, public domain):
// one 32-bit integer of state, fast, and good enough for shuffles and puzzle generation. It is not
// for anything secret. The state is a plain number, so a game can save it and carry on exactly
// where it left off.

/** A seeded random source. `state()` is a 32-bit integer; `createRng(rng.state())` resumes it. */
export interface Rng {
  /** A float in [0, 1). */
  next(): number;
  /** An integer in [min, max], both inclusive. */
  int(min: number, max: number): number;
  /** One item of a non-empty array. */
  pick<T>(items: readonly T[]): T;
  /** A shuffled copy (Fisher-Yates); the input is left alone. */
  shuffle<T>(items: readonly T[]): T[];
  /** The current state: save it, then `createRng(state)` continues the same sequence. */
  state(): number;
}

/** Creates a generator from a seed (any number; it is reduced to 32 bits). */
export function createRng(seed: number): Rng {
  let a = toUint32(seed);
  const next = (): number => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
  const int = (min: number, max: number): number => {
    const lo = Math.ceil(Math.min(min, max));
    const hi = Math.floor(Math.max(min, max));
    return lo + Math.floor(next() * (hi - lo + 1));
  };
  return {
    next,
    int,
    pick<T>(items: readonly T[]): T {
      if (items.length === 0) throw new RangeError("pick needs at least one item");
      return items[int(0, items.length - 1)] as T;
    },
    shuffle<T>(items: readonly T[]): T[] {
      const out = items.slice();
      for (let i = out.length - 1; i > 0; i--) {
        const j = int(0, i);
        const tmp = out[i] as T;
        out[i] = out[j] as T;
        out[j] = tmp;
      }
      return out;
    },
    state: () => a,
  };
}

/** Any number as an unsigned 32-bit integer; non-finite numbers become 0. */
export function toUint32(n: number): number {
  return Number.isFinite(n) ? Math.trunc(n) >>> 0 : 0;
}

/** FNV-1a, 32-bit, over UTF-16 code units: a stable seed from a string. */
export function hashString(text: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h >>> 0;
}

/** "YYYY-MM-DD" for the player's local calendar day. */
export function localDateKey(date: Date): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

/**
 * The seed of the day: everyone playing on the same local date gets the same puzzle. Pass a Date
 * (its local date counts) or a "YYYY-MM-DD" string. Optional `salt` gives each game its own
 * puzzle for the same day.
 */
export function dailySeed(date: Date | string = new Date(), salt = ""): number {
  let key: string;
  if (typeof date === "string") {
    if (!/^\d{4}-\d{2}-\d{2}$/.test(date)) throw new RangeError("dailySeed wants YYYY-MM-DD");
    key = date;
  } else {
    if (Number.isNaN(date.getTime())) throw new RangeError("dailySeed got an invalid date");
    key = localDateKey(date);
  }
  return hashString(salt ? `${salt}:${key}` : key);
}

/** A fresh, unpredictable seed for a "new game" (not for anything secret). */
export function randomSeed(): number {
  return Math.floor(Math.random() * 4294967296) >>> 0;
}
