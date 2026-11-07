import { describe, expect, it } from "vitest";
import type { ProviderProfile } from "./api";
import {
  answerBadgeText,
  composerKeyAction,
  defaultChoice,
  effectiveChoice,
  isConfigured,
  modelIdProblem,
  retryChoice,
} from "./modelChoice";

const p = (id: string, isDefault: boolean, models: string[], def = models[0]): ProviderProfile => ({
  id,
  kind: "openai",
  label: `L-${id}`,
  baseUrl: "https://api.openai.com",
  host: "api.openai.com",
  model: def ?? "",
  models: models.map((m) => ({
    model: m,
    label: m === "x2" ? "Second" : null,
    isDefault: m === def,
  })),
  monthlyTokenBudget: null,
  isDefault,
  hasKey: true,
  keychainOk: true,
});

const PROFILES = [p("a", false, ["a1"]), p("b", true, ["x1", "x2"], "x2")];

describe("model choice", () => {
  it("defaults to the default provider's default model", () => {
    expect(defaultChoice(PROFILES)).toEqual({ profileId: "b", model: "x2" });
    expect(defaultChoice([p("a", false, ["a1"])])).toEqual({ profileId: "a", model: "a1" });
    expect(defaultChoice([])).toBeNull();
  });

  it("keeps a pick only while it is configured", () => {
    const pick = { profileId: "a", model: "a1" };
    expect(effectiveChoice(PROFILES, pick)).toEqual(pick);
    expect(isConfigured(PROFILES, { profileId: "a", model: "gone" })).toBe(false);
    expect(effectiveChoice(PROFILES, { profileId: "zz", model: "a1" })).toEqual({
      profileId: "b",
      model: "x2",
    });
  });

  it("retries with the message's own pair if it still exists", () => {
    const cur = { profileId: "b", model: "x2" };
    expect(retryChoice(PROFILES, { profileId: "a", model: "a1" }, cur)).toEqual({
      profileId: "a",
      model: "a1",
    });
    expect(retryChoice(PROFILES, { profileId: null, model: null }, cur)).toEqual(cur);
  });

  it("labels answers with model and provider, or says the model is unknown", () => {
    expect(answerBadgeText({ profileId: "b", model: "x2" }, PROFILES)).toBe("Second · L-b");
    expect(answerBadgeText({ profileId: "gone", model: "m" }, PROFILES)).toBe("m");
    expect(answerBadgeText({ profileId: null, model: null }, PROFILES)).toBe("Model unknown");
  });

  it("checks typed model names with the core's rule", () => {
    expect(modelIdProblem("claude-x/1:2@b+c_d.e")).toBeNull();
    expect(modelIdProblem(" ")).not.toBeNull();
    expect(modelIdProblem("a b")).not.toBeNull();
    expect(modelIdProblem("x".repeat(201))).not.toBeNull();
  });
});

describe("composer keys", () => {
  const k = (more: Partial<Parameters<typeof composerKeyAction>[0]>) =>
    composerKeyAction({
      key: "Enter",
      shiftKey: false,
      ctrlKey: false,
      metaKey: false,
      isComposing: false,
      ...more,
    });
  it("Enter sends, Shift+Enter is a new line, composing never sends", () => {
    expect(k({})).toBe("send");
    expect(k({ ctrlKey: true })).toBe("send");
    expect(k({ metaKey: true })).toBe("send");
    expect(k({ shiftKey: true })).toBe("newline");
    expect(k({ isComposing: true })).toBe("none");
    expect(k({ keyCode: 229 })).toBe("none");
    expect(k({ key: "a" })).toBe("none");
  });
});
