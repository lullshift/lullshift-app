import { describe, expect, it } from "vitest";
import { parseOpenMain } from "./openMain";

describe("parseOpenMain", () => {
  it("accepts a plain view, an activity and a session", () => {
    expect(parseOpenMain({ view: "home" })).toEqual({ view: "home" });
    expect(parseOpenMain({ view: "play", activityId: "bubble-wrap" })).toEqual({
      view: "play",
      activityId: "bubble-wrap",
    });
    expect(
      parseOpenMain({ view: "sessions", sessionKey: { provider: "codex", sessionId: "s1" } }),
    ).toEqual({ view: "sessions", sessionKey: { provider: "codex", sessionId: "s1" } });
  });

  it("ignores anything else", () => {
    for (const bad of [
      null,
      "play",
      42,
      {},
      { view: "settings" },
      { view: "Play" },
      { view: "play", activityId: "" },
      { view: "play", activityId: 3 },
      { view: "sessions", sessionKey: null },
      { view: "sessions", sessionKey: { provider: "codex" } },
    ]) {
      expect(parseOpenMain(bad)).toBeNull();
    }
  });

  it("drops fields it does not know", () => {
    expect(parseOpenMain({ view: "home", extra: "<script>" })).toEqual({ view: "home" });
  });
});
