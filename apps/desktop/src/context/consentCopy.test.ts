import { describe, expect, it } from "vitest";
import { SUPPORTED_AGENTS } from "../adapters";
import { CONSENT_SCOPE_NOTE, consentItems, consentSwitchDescription } from "./consentCopy";

const NEWLINE = String.fromCharCode(10);
const textOf = (agent: (typeof SUPPORTED_AGENTS)[number]) => consentItems(agent).join(NEWLINE);

describe("task context consent copy", () => {
  it("names each captured Claude Code field and its limit", () => {
    const claude = textOf("claude-code");
    expect(claude).toMatch(/first prompt of each session/);
    expect(claude).toMatch(/2,000/);
    expect(claude).toMatch(/question/);
    expect(claude).toMatch(/only the tool name/);
    expect(claude).toMatch(/300/);
  });

  it("keeps Codex and Cursor to the first prompt only", () => {
    for (const agent of ["codex", "cursor"] as const) {
      const text = textOf(agent);
      expect(text).toMatch(/first prompt of each session/);
      expect(text).toMatch(/2,000/);
      expect(text).not.toMatch(/asks you/i);
      expect(text).toMatch(/Nothing else/);
      expect(text).toContain("email");
    }
  });

  it("says secrets are hidden as a safety net, not a guarantee, for every agent", () => {
    for (const agent of SUPPORTED_AGENTS) {
      const text = textOf(agent);
      expect(text).toMatch(/secrets are hidden/);
      expect(text).toMatch(/not a guarantee/);
      expect(text).toMatch(/on this device until you clear it or delete the session/);
    }
  });

  it("names what is never kept", () => {
    for (const agent of SUPPORTED_AGENTS) {
      const text = textOf(agent);
      for (const w of ["tool inputs", "commands", "files", "paths", "transcripts"]) {
        expect(text).toContain(w);
      }
    }
  });

  it("describes each switch without promising more than it keeps", () => {
    for (const agent of SUPPORTED_AGENTS) {
      expect(consentSwitchDescription(agent)).toMatch(/first prompt of each/);
      expect(consentSwitchDescription(agent)).toMatch(/Off by default/);
    }
  });

  it("names all three agents in the scope note", () => {
    for (const name of ["Claude Code", "Codex", "Cursor"]) {
      expect(CONSENT_SCOPE_NOTE).toContain(name);
    }
  });
});
