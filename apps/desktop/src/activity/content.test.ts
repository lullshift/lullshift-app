// The host's pack check, UI side (ADR-016): a pack starts only with data that passed Rust's
// check against its engine's schema; anything else becomes a calm, honest message.
import { describe, expect, it, vi } from "vitest";
import { describeContentError, loadContent } from "./content";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: () => Promise.reject(new Error("no Tauri in tests")),
}));

const pack = { id: "app.lullshift.content.desk-stretches", engine: "lullshift.cards@1" };
const deck = { title: "Desk Stretches", cards: [{ front: "Roll your shoulders" }] };

function fakeInvoke(result: { ok: unknown } | { err: unknown }) {
  const calls: [string, Record<string, unknown>][] = [];
  const call = <T>(cmd: string, args: Record<string, unknown>): Promise<T> => {
    calls.push([cmd, args]);
    return "ok" in result ? Promise.resolve(result.ok as T) : Promise.reject(result.err);
  };
  return { call, calls };
}

describe("loadContent", () => {
  it("never asks for content for an app: init.content is null", async () => {
    const { call, calls } = fakeInvoke({ ok: deck });
    expect(await loadContent({ id: "app.lullshift.games.x", engine: null }, call)).toEqual({
      ok: true,
      content: null,
    });
    expect(calls).toEqual([]);
  });

  it("hands a pack's checked data to the engine, asked for by the pack's own id", async () => {
    const { call, calls } = fakeInvoke({ ok: deck });
    expect(await loadContent(pack, call)).toEqual({ ok: true, content: deck });
    expect(calls).toEqual([["activity_content", { id: pack.id }]]);
  });

  it("refuses to start a pack whose data doesn't match its engine", async () => {
    const detail = "deck.json: cards[3].front is longer than 200 characters";
    const { call } = fakeInvoke({ err: { code: "data_invalid", detail } });
    const r = await loadContent(pack, call);
    expect(r.ok).toBe(false);
    if (r.ok) return;
    expect(r.problem.message).toBe(
      "This pack's data doesn't match its engine, so Lullshift didn't start it.",
    );
    expect(r.problem.detail).toBe(detail);
  });

  it("says which engine is missing", async () => {
    const { call } = fakeInvoke({
      err: { code: "engine_missing", detail: "no bundled engine provides lullshift.cards@9" },
    });
    const r = await loadContent({ ...pack, engine: "lullshift.cards@9" }, call);
    expect(r.ok || r.problem.message).toMatch(/needs the lullshift\.cards@9 engine/);
  });

  it("treats empty data, unknown errors and broken replies as not checked", async () => {
    for (const result of [
      { ok: null },
      { ok: "text" },
      { err: "internal" },
      { err: new Error("ipc down") },
      { err: { code: "something_new", detail: "<b>raw</b>" } },
    ]) {
      const r = await loadContent(pack, fakeInvoke(result).call);
      expect(r.ok, JSON.stringify(result)).toBe(false);
      if (r.ok) continue;
      expect(r.problem.message).toMatch(/couldn't be checked, so Lullshift didn't start it/);
      expect(r.problem.detail).toBe("");
    }
  });
});

describe("describeContentError", () => {
  it("has calm words for every code the host sends", () => {
    for (const code of [
      "data_invalid",
      "engine_missing",
      "engine_unusable",
      "data_missing",
      "data_not_json",
      "data_too_large",
      "unknown_activity",
      "internal",
    ]) {
      const p = describeContentError({ code, detail: "x" }, "lullshift.cards@1");
      expect(p.message, code).toMatch(/^This pack/);
      expect(p.message, code).not.toMatch(/error|exception|undefined|null/i);
    }
  });

  it("keeps the detail short", () => {
    const p = describeContentError({ code: "data_invalid", detail: "y".repeat(5000) }, null);
    expect(p.detail).toHaveLength(600);
  });
});
