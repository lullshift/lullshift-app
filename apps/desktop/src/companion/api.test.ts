import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.hoisted(() => vi.fn());
const FakeChannel = vi.hoisted(
  () =>
    class {
      onmessage: ((e: unknown) => void) | undefined;
    },
);
vi.mock("@tauri-apps/api/core", () => ({ invoke, Channel: FakeChannel }));

import {
  companionRetry,
  companionSend,
  companionSetKey,
  companionToolDecision,
  newRequestId,
  toCommandError,
} from "./api";
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
    const err = await companionSend(
      1,
      "p",
      "m",
      "my prompt text",
      { kind: "none" },
      null,
      "r-0123456789",
      () => undefined,
    ).catch((e: unknown) => e);
    expect(JSON.stringify(err)).not.toContain("my prompt text");
  });
});

describe("companionSend", () => {
  it("passes the previewed context hash through", async () => {
    invoke.mockResolvedValue({});
    await companionSend(
      1,
      "p",
      "model-a",
      "hi",
      { kind: "none" },
      null,
      "r-0123456789",
      () => undefined,
    );
    expect(invoke).toHaveBeenLastCalledWith("companion_send", {
      threadId: 1,
      profileId: "p",
      model: "model-a",
      text: "hi",
      scope: { kind: "none" },
      expectedContextSha256: null,
      requestId: "r-0123456789",
      onEvent: expect.any(FakeChannel),
    });
    await companionSend(
      1,
      "p",
      "m",
      "hi",
      { kind: "sessionSummary", provider: "x", sessionId: "y" },
      "ab12",
      "r-0123456789",
      () => undefined,
    );
    expect(invoke.mock.lastCall?.[1]).toMatchObject({ expectedContextSha256: "ab12" });
  });
});

describe("streaming calls", () => {
  it("routes channel messages to the handler given for that call", async () => {
    invoke.mockResolvedValue({});
    const seen: unknown[] = [];
    await companionRetry(5, "p", "model-b", "r-abcdefgh", (e) => seen.push(e));
    expect(invoke.mock.lastCall?.[1]).toMatchObject({
      messageId: 5,
      profileId: "p",
      model: "model-b",
    });
    const args = invoke.mock.lastCall?.[1] as {
      onEvent: InstanceType<typeof FakeChannel>;
      requestId: string;
    };
    expect(args.requestId).toBe("r-abcdefgh");
    args.onEvent.onmessage?.({ type: "delta", requestId: "r-abcdefgh", text: "hi" });
    expect(seen).toEqual([{ type: "delta", requestId: "r-abcdefgh", text: "hi" }]);
  });

  it("answers a lookup by request and call id only", async () => {
    invoke.mockResolvedValue(undefined);
    await companionToolDecision("r-abcdefgh", "call_1", false);
    expect(invoke).toHaveBeenLastCalledWith("companion_tool_decision", {
      requestId: "r-abcdefgh",
      approvalId: "call_1",
      approve: false,
    });
  });

  it("makes request ids the backend accepts, and different each time", () => {
    const a = newRequestId();
    expect(a).toMatch(/^[A-Za-z0-9_-]{8,64}$/);
    expect(newRequestId()).not.toBe(a);
  });
});
