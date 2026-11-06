import { describe, expect, it, vi } from "vitest";
import { focusIfLost } from "./focus";

describe("focusIfLost", () => {
  const body = {};
  it("focuses the fallback when focus dropped to the page body", () => {
    const el = { focus: vi.fn() };
    expect(focusIfLost(() => el, { activeElement: body, body })).toBe(true);
    expect(el.focus).toHaveBeenCalledOnce();
  });
  it("also covers no active element at all", () => {
    const el = { focus: vi.fn() };
    expect(focusIfLost(() => el, { activeElement: null, body })).toBe(true);
  });
  it("leaves focus alone when it is somewhere on purpose", () => {
    const el = { focus: vi.fn() };
    expect(focusIfLost(() => el, { activeElement: {}, body })).toBe(false);
    expect(el.focus).not.toHaveBeenCalled();
  });
  it("does nothing when there is no fallback", () => {
    expect(focusIfLost(() => null, { activeElement: body, body })).toBe(false);
  });
});
