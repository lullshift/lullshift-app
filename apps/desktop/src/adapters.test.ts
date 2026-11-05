import { describe, expect, it } from "vitest";
import {
  adapterErrorText,
  adapterStatusText,
  CAPABILITY_SUMMARY,
  CODEX_TRUST_NOTE,
  hookEventText,
} from "./adapters";

describe("Codex and Cursor integration text", () => {
  it("reports hooks turned off in Codex", () => {
    expect(adapterStatusText("hooks_disabled", "codex")).toMatch(/turned off in Codex/);
  });

  it("tells the user to trust Codex hooks in /hooks, including after a repair", () => {
    expect(CODEX_TRUST_NOTE).toMatch(/\/hooks/);
    expect(adapterStatusText("installed", "codex")).toMatch(/trusted the hooks in Codex/);
    expect(adapterStatusText("needs_update", "codex")).toMatch(/trust the changed hooks/);
  });

  it("does not claim Cursor can say it is waiting, and Codex only partly", () => {
    expect(CAPABILITY_SUMMARY.cursor.join(" ")).toMatch(
      /doesn't tell Lullshift when it is waiting/,
    );
    expect(CAPABILITY_SUMMARY.codex.join(" ")).toMatch(/only part/);
  });

  it("names the right agent and file in status and errors", () => {
    expect(adapterStatusText("not_installed", "cursor")).toMatch(/Cursor sessions/);
    expect(adapterErrorText("settings_invalid", "cursor")).toMatch(/Cursor hooks file/);
    expect(adapterErrorText("unsupported_path", "cursor")).not.toMatch(/Claude/);
    expect(adapterStatusText("settings_invalid", "codex")).toMatch(/won't change it/);
  });

  it("describes the Codex and Cursor hook events in plain words", () => {
    expect(hookEventText(["Interrupt", "afterAgentResponse"])).toEqual([
      "you stop a turn",
      "it replies",
    ]);
  });
});
