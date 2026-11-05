// Activity bridge protocol v1 (ADR-012). Every message between the host (this app) and an
// activity is a plain object `{ lullshift: 1, nonce, type, ... }` sent with postMessage.
// Activities are untrusted: the host checks every message they send with `parseActivityMessage`
// before acting on it, and drops (and counts) anything else.

export const PROTOCOL = 1;

/** Largest message an activity may send, measured as JSON. */
export const MESSAGE_MAX = 320 * 1024;
/** Largest value for one storage key; mirrors `VALUE_MAX` in `activities.rs`. */
export const VALUE_MAX = 64 * 1024;
const ID_MAX = 64;
const KEY_PATTERN = /^[A-Za-z0-9._-]{1,64}$/;

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
    };

/** Host to activity. */
export type HostMessage =
  | {
      type: "init";
      restore: unknown;
      reducedMotion: boolean;
      theme: "light" | "dark";
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
    default:
      return { ok: false, reason: "type" };
  }
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
  private tokens: number;
  private last: number;

  constructor(
    private readonly rate: number,
    private readonly burst: number,
    now: number,
  ) {
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
