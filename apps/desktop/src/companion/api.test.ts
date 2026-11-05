import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import { companionSend, companionSetKey, toCommandError } from "./api";
import { errorText } from "./logic";

const SECRET = "sk-test-SECRET-0123456789";

beforeEach(() => {
  invoke.mockReset();
});

describe("companionSetKey", () => {
  it("sends the key once to companion_set_key and nowhere else", async () => {
    invoke.mockResolvedValue(undefined);
    const result = await companionSetKey("p1", SECRET);
    expect(result).toBeUndefined();
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith("companion_set_key", { profileId: "p1", key: SECRET });
  });

  it("keeps the key out of a rejection, whatever the backend or Tauri says", async () => {
    const hostile = [
      { kind: "keyRejected", message: `bad key ${SECRET}` },
      `command failed for ${SECRET}`,
      new Error(`boom ${SECRET}`),
      { kind: "nonsense", message: SECRET },
    ];
    for (const rejection of hostile) {
      invoke.mockRejectedValueOnce(rejection);
      const err = await companionSetKey("p1", SECRET).catch((e: unknown) => e);
      // Whatever the backend's message holds, the UI never shows it.
      expect(errorText(err as { kind: string; message: string }, "h")).not.toContain(SECRET);
      // Errors that did not come from the contract carry no text at all.
      if (!(
        typeof rejection === "object" &&
        "kind" in rejection &&
        rejection.kind === "keyRejected"
      )) {
        expect(JSON.stringify(err)).not.toContain(SECRET);
      }
    }
  });
});

describe("toCommandError", () => {
  it("keeps contract errors", () => {
    expect(toCommandError({ kind: "offline", message: "Can't reach it." })).toEqual({
      kind: "offline",
      message: "Can't reach it.",
    });
  });

  it("maps a missing command to unavailable, without text", () => {
    expect(toCommandError("Command companion_threads not found")).toEqual({
      kind: "unavailable",
      message: "",
    });
  });

  it("maps anything else to internal, without text", () => {
    expect(toCommandError(`weird ${SECRET}`)).toEqual({ kind: "internal", message: "" });
    expect(toCommandError(null)).toEqual({ kind: "internal", message: "" });
  });

  it("does not copy message text from a failed send", async () => {
    invoke.mockRejectedValueOnce(`failed: my prompt text ${SECRET}`);
    const err = await companionSend(1, "p", "my prompt text", { kind: "none" }, null).catch(
      (e: unknown) => e,
    );
    expect(JSON.stringify(err)).not.toContain("my prompt text");
  });
});

describe("companionSend", () => {
  it("passes the previewed context hash through", async () => {
    invoke.mockResolvedValue({});
    await companionSend(1, "p", "hi", { kind: "none" }, null);
    expect(invoke).toHaveBeenLastCalledWith("companion_send", {
      threadId: 1,
      profileId: "p",
      text: "hi",
      scope: { kind: "none" },
      expectedContextSha256: null,
    });
    await companionSend(
      1,
      "p",
      "hi",
      { kind: "sessionSummary", provider: "x", sessionId: "y" },
      "ab12",
    );
    expect(invoke.mock.lastCall?.[1]).toMatchObject({ expectedContextSha256: "ab12" });
  });
});
