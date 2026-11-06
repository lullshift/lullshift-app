import { describe, expect, it } from "vitest";
import {
  CONNECT_EXPLAINER,
  CONNECT_LABEL,
  WELCOME_LINES,
  suggestFirstActivity,
} from "./welcomeCopy";

const a = (id: string, category: string, savedUtc: string | null = null) => ({
  id,
  name: id,
  category,
  savedUtc,
});

describe("first activity suggestion", () => {
  it("prefers a break activity", () => {
    expect(suggestFirstActivity([a("g", "arcade"), a("b", "break")])?.id).toBe("b");
  });
  it("skips activities already started while others remain", () => {
    expect(
      suggestFirstActivity([a("b", "break", "2026-11-01T00:00:00Z"), a("g", "arcade")])?.id,
    ).toBe("g");
  });
  it("falls back to a started one, and to nothing for an empty library", () => {
    expect(suggestFirstActivity([a("b", "break", "2026-11-01T00:00:00Z")])?.id).toBe("b");
    expect(suggestFirstActivity([])).toBeNull();
  });
});

describe("welcome wording stays true to what the app does", () => {
  it("says no account, local and offline", () => {
    const text = WELCOME_LINES.join(" ").toLowerCase();
    expect(text).toContain("no account");
    expect(text).toContain("this computer");
    expect(text).toContain("offline");
  });
  it("names the backup and the two confirmations", () => {
    expect(CONNECT_EXPLAINER).toContain("hooks or settings file");
    expect(CONNECT_LABEL).toBe("Connect an agent");
    expect(CONNECT_EXPLAINER.toLowerCase()).toContain("backup");
    expect(CONNECT_EXPLAINER).toContain("system dialog");
    expect(CONNECT_EXPLAINER.toLowerCase()).toContain("optional");
  });
});
