import { describe, expect, it } from "vitest";
import {
  MESSAGE_MAX,
  RateLimiter,
  VALUE_MAX,
  hostEnvelope,
  jsonSize,
  newNonce,
  parseActivityMessage,
} from "./protocol";

const N = "a".repeat(32);
const msg = (fields: Record<string, unknown>) => ({ lullshift: 1, nonce: N, ...fields });

describe("parseActivityMessage", () => {
  it("accepts every valid message type", () => {
    expect(parseActivityMessage(msg({ type: "ready" }), N)).toEqual({
      ok: true,
      message: { type: "ready" },
    });
    expect(parseActivityMessage(msg({ type: "state", state: { moves: 3 } }), N)).toEqual({
      ok: true,
      message: { type: "state", requestId: null, state: { moves: 3 } },
    });
    expect(parseActivityMessage(msg({ type: "state", requestId: "r1", state: 0 }), N).ok).toBe(
      true,
    );
    for (const op of ["get", "remove"]) {
      expect(
        parseActivityMessage(msg({ type: "storage", requestId: "r", op, key: "best" }), N),
      ).toMatchObject({ ok: true });
    }
    expect(
      parseActivityMessage(
        msg({ type: "storage", requestId: "r", op: "set", key: "best", value: "12" }),
        N,
      ).ok,
    ).toBe(true);
  });

  it("drops anything without the protocol marker or the right nonce", () => {
    for (const data of [null, "ready", 1, [], { type: "ready" }, { lullshift: 2, nonce: N }]) {
      expect(parseActivityMessage(data, N)).toEqual({ ok: false, reason: "shape" });
    }
    expect(parseActivityMessage({ ...msg({ type: "ready" }), nonce: "stale" }, N)).toEqual({
      ok: false,
      reason: "nonce",
    });
  });

  it("drops unknown types, including host-only ones", () => {
    for (const type of ["init", "save", "storage-result", "invoke", "__proto__", undefined]) {
      expect(parseActivityMessage(msg({ type }), N)).toEqual({ ok: false, reason: "type" });
    }
  });

  it("checks fields: request IDs, keys, values and missing state", () => {
    const bad = [
      { type: "state" },
      { type: "state", state: null },
      { type: "state", requestId: 7, state: 1 },
      { type: "state", requestId: "x".repeat(65), state: 1 },
      { type: "storage", requestId: "r", op: "get", key: "../x" },
      { type: "storage", requestId: "r", op: "get", key: "" },
      { type: "storage", requestId: "r", op: "get", key: "k".repeat(65) },
      { type: "storage", requestId: "", op: "get", key: "k" },
      { type: "storage", requestId: "r", op: "drop", key: "k" },
      { type: "storage", requestId: "r", op: "set", key: "k" },
      { type: "storage", requestId: "r", op: "set", key: "k", value: 5 },
      { type: "storage", requestId: "r", op: "set", key: "k", value: "v".repeat(VALUE_MAX + 1) },
    ];
    for (const fields of bad) {
      expect(parseActivityMessage(msg(fields), N), JSON.stringify(fields).slice(0, 80)).toEqual({
        ok: false,
        reason: "field",
      });
    }
  });

  it("drops oversize and unserializable messages", () => {
    expect(parseActivityMessage(msg({ type: "state", state: "x".repeat(MESSAGE_MAX) }), N)).toEqual(
      { ok: false, reason: "size" },
    );
    const cyclic: Record<string, unknown> = {};
    cyclic.self = cyclic;
    expect(parseActivityMessage(msg({ type: "state", state: cyclic }), N)).toEqual({
      ok: false,
      reason: "size",
    });
    expect(jsonSize(undefined)).toBe(Infinity);
  });
});

describe("helpers", () => {
  it("wraps host messages with the protocol marker and nonce", () => {
    expect(hostEnvelope(N, { type: "save", requestId: "r" })).toEqual({
      lullshift: 1,
      nonce: N,
      type: "save",
      requestId: "r",
    });
  });

  it("makes distinct 128-bit nonces", () => {
    const a = newNonce();
    expect(a).toMatch(/^[0-9a-f]{32}$/);
    expect(newNonce()).not.toBe(a);
  });

  it("rate-limits bursts and refills over time", () => {
    const r = new RateLimiter(10, 3, 0);
    expect([r.take(0), r.take(0), r.take(0), r.take(0)]).toEqual([true, true, true, false]);
    expect(r.take(50)).toBe(false);
    expect(r.take(100)).toBe(true);
    expect(r.take(10_000)).toBe(true);
  });
});
