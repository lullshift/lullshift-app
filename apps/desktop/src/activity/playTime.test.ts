import { describe, expect, it } from "vitest";
import { formatPlayed } from "./playTime";

describe("formatPlayed", () => {
  it("speaks in minutes and hours", () => {
    expect(formatPlayed(0)).toBe("Under a minute");
    expect(formatPlayed(59)).toBe("Under a minute");
    expect(formatPlayed(60)).toBe("1 min");
    expect(formatPlayed(59 * 60 + 59)).toBe("59 min");
    expect(formatPlayed(3600)).toBe("1 h");
    expect(formatPlayed(3600 + 5 * 60)).toBe("1 h 5 min");
    expect(formatPlayed(-30)).toBe("Under a minute");
  });
});
