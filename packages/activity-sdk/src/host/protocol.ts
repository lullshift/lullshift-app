// Activity bridge protocol v1, host side (ADR-012). Every message between a host (the desktop
// app, or the CLI's simulator) and an activity is a plain object `{ lullshift: 1, nonce, type,
// ... }` sent with postMessage. Activities are untrusted: the host checks every message they send
// with `parseActivityMessage` before acting on it, and drops (and counts) anything else.

export const PROTOCOL = 1;

/** Largest message an activity may send, measured as JSON. */
export const MESSAGE_MAX = 320 * 1024;
/** Largest value for one storage key; mirrors `VALUE_MAX` in `activities.rs`. */
export const VALUE_MAX = 64 * 1024;
const ID_MAX = 64;
const KEY_PATTERN = /^[A-Za-z0-9._-]{1,64}$/;
/** Summary limits (Sprint 05): items per summary, characters per label or value. */
export const SUMMARY_ITEMS_MAX = 4;
export const SUMMARY_TEXT_MAX = 24;

// Control, bidirectional, separator and invisible formatting characters: the host's set
// (`hidden_char` in crates/marketplace, `is_invisible` in agent-core) and the manifest's
// plain-text pattern. All of them run tests/fixtures/plain-text/cases.json (decision 18.7).
const NOT_PLAIN =
  // eslint-disable-next-line no-control-regex, no-misleading-character-class
  /[\u0000-\u001F\u007F-\u009F\u00AD\u034F\u061C\u115F-\u1160\u17B4-\u17B5\u180B-\u180E\u200B-\u200F\u2028-\u202E\u2060-\u2064\u2066-\u206F\u3164\uFEFF\uFFA0\uFFF9-\uFFFB\u{1BCA0}-\u{1BCA3}\u{1D173}-\u{1D17A}\u{E0000}-\u{E007F}]/u;

// More than four combining marks in a row stack over neighbouring text; the host refuses them.
const LONG_MARK_RUN =
  // eslint-disable-next-line no-misleading-character-class
  /[\u0300-\u036F\u0483-\u0489\u0591-\u05BD\u0610-\u061A\u064B-\u065F\u1AB0-\u1AFF\u1DC0-\u1DFF\u20D0-\u20FF\uFE20-\uFE2F]{5}/u;

/**
 * Plain text of 1 to `max` characters (code points), with no control or invisible characters
 * and no run of more than four combining marks.
 */
export function isPlainText(v: unknown, max: number): v is string {
  if (typeof v !== "string" || v.length === 0 || NOT_PLAIN.test(v) || LONG_MARK_RUN.test(v)) {
    return false;
  }
  return [...v].length <= max;
}

/** One stat in the paused view, such as `{ label: "Moves", value: 12 }`. */
export interface SummaryItem {
  label: string;
  value: string | number;
}

/** Activity to host. */
export type ActivityMessage =
  | { type: "ready" }
  | { type: "state"; requestId: string | null; state: unknown }
  | {
      type: "storage";
      requestId: string;
      op: "get" | "remove";
      key: string;
    }
  | {
      type: "storage";
      requestId: string;
      op: "set";
      key: string;
      value: string;
    }
  | { type: "summary"; items: SummaryItem[] };

/** Host to activity. */
export type HostMessage =
  | {
      type: "init";
      restore: unknown;
      reducedMotion: boolean;
      theme: "light" | "dark";
      /** Validated pack data for engines (ADR-016); null for every other activity. */
      content: unknown;
    }
  | { type: "pause" }
  | { type: "resume" }
  | { type: "save"; requestId: string }
  | {
      type: "storage-result";
      requestId: string;
      ok: boolean;
      value?: string | null;
      error?: string;
    };

export type DropReason = "shape" | "nonce" | "type" | "size" | "field";

export type Parsed = { ok: true; message: ActivityMessage } | { ok: false; reason: DropReason };

const isRecord = (v: unknown): v is Record<string, unknown> =>
  typeof v === "object" && v !== null && !Array.isArray(v);

const isRequestId = (v: unknown): v is string =>
  typeof v === "string" && v.length > 0 && v.length <= ID_MAX;

/** JSON size of a value, or Infinity if it can't be serialized (cycles, BigInt). */
export function jsonSize(value: unknown): number {
  try {
    const text = JSON.stringify(value);
    return text === undefined ? Infinity : text.length;
  } catch {
    return Infinity;
  }
}

/** Validates a message from the activity whose current launch nonce is `nonce`. */
export function parseActivityMessage(data: unknown, nonce: string): Parsed {
  if (!isRecord(data) || data.lullshift !== PROTOCOL) return { ok: false, reason: "shape" };
  if (data.nonce !== nonce) return { ok: false, reason: "nonce" };
  if (jsonSize(data) > MESSAGE_MAX) return { ok: false, reason: "size" };
  switch (data.type) {
    case "ready":
      return { ok: true, message: { type: "ready" } };
    case "state": {
      const requestId = data.requestId ?? null;
      if (requestId !== null && !isRequestId(requestId)) return { ok: false, reason: "field" };
      // A missing or null state would wipe the saved one: never accepted.
      if (data.state === undefined || data.state === null) return { ok: false, reason: "field" };
      return { ok: true, message: { type: "state", requestId, state: data.state } };
    }
    case "storage": {
      const { requestId, op, key, value } = data;
      if (!isRequestId(requestId) || typeof key !== "string" || !KEY_PATTERN.test(key)) {
        return { ok: false, reason: "field" };
      }
      if (op === "get" || op === "remove") {
        return { ok: true, message: { type: "storage", requestId, op, key } };
      }
      if (op === "set" && typeof value === "string" && value.length <= VALUE_MAX) {
        return { ok: true, message: { type: "storage", requestId, op, key, value } };
      }
      return { ok: false, reason: "field" };
    }
    case "summary": {
      const items = parseSummary(data.items);
      return items
        ? { ok: true, message: { type: "summary", items } }
        : { ok: false, reason: "field" };
    }
    default:
      return { ok: false, reason: "type" };
  }
}

/** A valid summary (copied, so the activity can't change it later), or null. */
export function parseSummary(items: unknown): SummaryItem[] | null {
  if (!Array.isArray(items) || items.length === 0 || items.length > SUMMARY_ITEMS_MAX) return null;
  const out: SummaryItem[] = [];
  for (const item of items as unknown[]) {
    if (!isRecord(item) || Object.keys(item).some((k) => k !== "label" && k !== "value")) {
      return null;
    }
    const { label, value } = item;
    if (!isPlainText(label, SUMMARY_TEXT_MAX)) return null;
    if (typeof value === "number") {
      if (!Number.isFinite(value)) return null;
    } else if (!isPlainText(value, SUMMARY_TEXT_MAX)) {
      return null;
    }
    out.push({ label, value });
  }
  return out;
}

/** Wraps a host message for posting. */
export function hostEnvelope(nonce: string, message: HostMessage) {
  return { lullshift: PROTOCOL, nonce, ...message };
}

/** A random launch nonce: 128 bits, hex. */
export function newNonce(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

/**
 * Token bucket: `rate` messages per second on average, bursts up to `burst`. Messages over the
 * limit are dropped, never queued, so a flooding activity can't slow the app down.
 */
export class RateLimiter {
  private readonly rate: number;
  private readonly burst: number;
  private tokens: number;
  private last: number;

  // Plain fields, not parameter properties: Node runs this file with type stripping only.
  constructor(rate: number, burst: number, now: number) {
    this.rate = rate;
    this.burst = burst;
    this.tokens = burst;
    this.last = now;
  }

  take(now: number): boolean {
    this.tokens = Math.min(this.burst, this.tokens + ((now - this.last) / 1000) * this.rate);
    this.last = now;
    if (this.tokens < 1) return false;
    this.tokens -= 1;
    return true;
  }
}
