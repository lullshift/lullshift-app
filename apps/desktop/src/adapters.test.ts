import { describe, expect, it } from "vitest";
import {
  adapterErrorText,
  adapterStatusText,
  CAPABILITY_SUMMARY,
  CODEX_TRUST_NOTE,
  EXPERIMENTAL_AGENTS,
  experimentalState,
  SUPPORTED_AGENTS,
  allAgentsConnected,
  emptySessionsCopy,
  statusCommand,
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
    expect(
      hookEventText([
        "Interrupt",
        "afterAgentResponse",
        "postToolUseFailure",
        "subagentStop",
        "preCompact",
      ]),
    ).toEqual([
      "you stop a turn",
      "it replies",
      "a step fails",
      "a helper agent finishes",
      "it tidies its context",
    ]);
  });
});

describe("OpenCode (experimental) text", () => {
  it("is unknown, not connected, until an event has arrived", () => {
    for (const seen of [false, null, undefined]) {
      expect(experimentalState({ status: "installed", eventSeen: seen })).toBe("unknown");
      expect(adapterStatusText("installed", "opencode", seen)).toMatch(/Status unknown/);
      expect(adapterStatusText("installed", "opencode", seen)).not.toMatch(/^Connected/);
    }
    expect(experimentalState({ status: "installed", eventSeen: true })).toBe("connected");
    expect(adapterStatusText("installed", "opencode", true)).toMatch(/^Connected/);
    expect(experimentalState({ status: "not_installed", eventSeen: true })).toBe("not_installed");
  });

  it("says it needs OpenCode 2.x and names the plugin file in errors", () => {
    expect(CAPABILITY_SUMMARY.opencode.join(" ")).toMatch(/OpenCode 2\.x/);
    expect(CAPABILITY_SUMMARY.opencode.join(" ")).toMatch(/permission/);
    expect(adapterErrorText("settings_invalid", "opencode")).toMatch(/plugins folder/);
    expect(adapterErrorText("settings_invalid", "opencode")).not.toMatch(/Claude|hooks file/);
    expect(adapterStatusText("settings_invalid", "opencode")).toMatch(/didn't write it/);
    expect(adapterStatusText("needs_update", "opencode")).toMatch(/plugin file/);
  });

  it("explains its events in plain words", () => {
    expect(hookEventText(["session.created", "permission.asked", "turn_ended", "failed"])).toEqual([
      "a session starts",
      "it asks for permission",
      "it finishes a turn",
      "a turn fails",
    ]);
  });

  it("is not part of the welcome's everything-connected check", () => {
    expect(SUPPORTED_AGENTS).not.toContain("opencode");
    expect(EXPERIMENTAL_AGENTS).toEqual(["opencode"]);
    expect(SUPPORTED_AGENTS.map(statusCommand)).not.toContain("opencode_status");
    expect(statusCommand("opencode")).toBe("opencode_status");
  });
});

describe("allAgentsConnected", () => {
  it("is true only when every supported agent is installed", () => {
    expect(allAgentsConnected(["installed", "installed", "installed"])).toBe(true);
    expect(allAgentsConnected(["installed", "not_installed", "installed"])).toBe(false);
    expect(allAgentsConnected(["installed", "needs_update", "installed"])).toBe(false);
  });
  it("treats unknown or missing status as not connected", () => {
    expect(allAgentsConnected(["installed", null, "installed"])).toBe(false);
    expect(allAgentsConnected(["installed", "installed"])).toBe(false);
    expect(allAgentsConnected([])).toBe(false);
  });
  it("asks the right command for each agent", () => {
    expect(SUPPORTED_AGENTS.map(statusCommand)).toEqual([
      "claude_code_status",
      "codex_status",
      "cursor_status",
    ]);
  });
});

describe("emptySessionsCopy", () => {
  it("offers to connect when nothing is connected or the status is unknown", () => {
    for (const connected of [null, []]) {
      const c = emptySessionsCopy(connected);
      expect(c.title).toBe("No agent sessions yet");
      expect(c.button).toBe("Connect an agent");
      expect(c.text).toContain("Claude Code, Codex or Cursor");
    }
  });

  it("names one connected agent and asks for a new session", () => {
    const c = emptySessionsCopy(["Claude Code"]);
    expect(c.title).toBe("No sessions yet");
    expect(c.text).toMatch(/^Claude Code is connected\. Start a new session/);
    expect(c.text).toContain("already open when you connected");
    expect(c.button).toBe("Manage integrations");
  });

  it("lists several connected agents", () => {
    expect(emptySessionsCopy(["Claude Code", "Codex"]).text).toMatch(
      /^Claude Code and Codex are connected/,
    );
    expect(emptySessionsCopy(["Claude Code", "Codex", "Cursor"]).text).toMatch(
      /^Claude Code, Codex and Cursor are connected/,
    );
  });
});
