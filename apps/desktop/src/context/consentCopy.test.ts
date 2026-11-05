import { describe, expect, it } from "vitest";
import { CONSENT_ITEMS, CONSENT_SWITCH_DESCRIPTION } from "./consentCopy";

describe("task context consent copy", () => {
  const text = CONSENT_ITEMS.join("\n");

  it("names each captured field and its limit", () => {
    expect(text).toMatch(/first prompt of each session/);
    expect(text).toMatch(/2,000/);
    expect(text).toMatch(/question/);
    expect(text).toMatch(/only the tool name/);
    expect(text).toMatch(/300/);
  });

  it("says secrets are hidden as a safety net, not a guarantee", () => {
    expect(text).toMatch(/secrets are hidden/);
    expect(text).toMatch(/not a guarantee/);
  });

  it("names what is never kept, and where it stays", () => {
    for (const w of ["tool inputs", "commands", "files", "paths", "transcripts"]) {
      expect(text).toContain(w);
    }
    expect(text).toMatch(/on this device until you clear it or delete the session/);
  });

  it("describes the switch without promising more than it keeps", () => {
    expect(CONSENT_SWITCH_DESCRIPTION).toMatch(/first prompt of each session/);
  });
});
