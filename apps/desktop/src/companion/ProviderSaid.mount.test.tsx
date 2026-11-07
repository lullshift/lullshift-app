// @vitest-environment jsdom
// What the provider said about a refused request reaches the screen (H1): in the conversation for
// a failed message and in Settings for a failed connection test, as plain text in its own element
// labelled as the provider's words. A link-looking reason is not a link.

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

let answer: (command: string) => unknown = () => null;

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string) => {
    try {
      return Promise.resolve(answer(command));
    } catch (err) {
      return Promise.reject(err);
    }
  },
  Channel: class {},
}));

import type { Message, ProviderProfile, Thread } from "./api";
import { toCommandError } from "./api";
import { CompanionSettings } from "./CompanionSettings";
import { Conversation } from "./Conversation";
import { buttonIn, polyfillDialog, settle } from "./testDom";
import { click } from "./testDom";

polyfillDialog();

const EVIL = "Bad model. Click here: https://evil.example/login now";

const PROFILE: ProviderProfile = {
  id: "po",
  kind: "openai",
  label: "OpenAI",
  baseUrl: "https://api.openai.com/v1",
  host: "api.openai.com",
  model: "gpt-x",
  models: [{ model: "gpt-x", label: null, isDefault: true }],
  monthlyTokenBudget: null,
  isDefault: true,
  hasKey: true,
  keychainOk: true,
};
const THREAD: Thread = {
  id: 7,
  title: "T",
  profileId: null,
  model: null,
  createdAt: "2026-11-07T10:00:00Z",
  updatedAt: "2026-11-07T10:00:00Z",
};

function msg(id: number, role: Message["role"], text: string, more: Partial<Message>): Message {
  return {
    id,
    threadId: THREAD.id,
    role,
    text,
    attachedContext: null,
    scopeKind: "none",
    host: "api.openai.com",
    profileId: "po",
    model: "gpt-x",
    status: "sent",
    error: null,
    inputTokens: null,
    outputTokens: null,
    createdAt: "2026-11-07T10:00:00Z",
    toolCalls: [],
    toolNotices: [],
    ...more,
  };
}

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
});

describe("toCommandError", () => {
  it("keeps a well-formed provider reason and drops anything else", () => {
    const good = toCommandError({
      kind: "providerError",
      message: "m",
      providerReason: { speaker: "OpenAI", text: "why" },
    });
    expect(good.providerReason).toEqual({ speaker: "OpenAI", text: "why" });
    for (const bad of [null, "text", 5, { speaker: 1, text: "x" }, { speaker: "a", text: "  " }]) {
      expect(
        toCommandError({ kind: "providerError", message: "m", providerReason: bad })
          .providerReason ?? null,
      ).toBeNull();
    }
  });
});

describe("a failed message in the conversation", () => {
  it("shows the provider's words in their own labelled element, and renders no link", async () => {
    const stored: Message[] = [
      msg(1, "user", "hello", {
        status: "failed",
        error: {
          kind: "providerError",
          message: "The provider didn't accept the request (HTTP 400).",
          providerReason: { speaker: "OpenAI", text: EVIL },
        },
      }),
    ];
    answer = (command) => {
      if (command === "companion_messages") return stored;
      throw new Error(`unexpected ${command}`);
    };
    await act(async () => {
      root.render(
        <Conversation
          thread={THREAD}
          profiles={[PROFILE]}
          sessions={[]}
          rememberedScope={null}
          onScopeUsed={() => undefined}
          onOpenSettings={() => undefined}
          onStartActivity={() => undefined}
          onActivity={() => undefined}
        />,
      );
    });
    await settle();
    const said = host.querySelector('[data-testid="provider-said"]')!;
    expect(said).not.toBeNull();
    expect(said.textContent).toBe(`OpenAI said: ${EVIL}`);
    expect(said.querySelector(".provider-said-by")!.textContent).toBe("OpenAI said:");
    expect(said.querySelector("a, button, [role='link']")).toBeNull();
    // Nowhere in the conversation is there a link made from the provider's words.
    expect(host.querySelector(".msg-user")!.querySelectorAll("a")).toHaveLength(0);
    for (const b of host.querySelectorAll("button")) {
      expect(b.textContent).not.toContain("evil.example");
    }
    // Lullshift's own sentence is still shown, separately.
    expect(host.textContent).toContain("Not sent.");
  });

  it("shows the not-streamed note beside a reply, outside the reply text", async () => {
    const stored: Message[] = [
      msg(1, "user", "q", {}),
      msg(2, "assistant", "the whole reply", { notStreamed: true }),
    ];
    answer = (command) => {
      if (command === "companion_messages") return stored;
      throw new Error(`unexpected ${command}`);
    };
    await act(async () => {
      root.render(
        <Conversation
          thread={THREAD}
          profiles={[PROFILE]}
          sessions={[]}
          rememberedScope={null}
          onScopeUsed={() => undefined}
          onOpenSettings={() => undefined}
          onStartActivity={() => undefined}
          onActivity={() => undefined}
        />,
      );
    });
    await settle();
    expect(host.querySelector('[data-testid="not-streamed-note"]')!.textContent).toContain(
      "arrived all at once",
    );
    // The reply's own text carries no such note.
    expect(host.querySelector(".msg-assistant")!.textContent).toContain("the whole reply");
    expect(stored[1]!.text).toBe("the whole reply");
  });
});

describe("Test connection in Settings", () => {
  it("shows the provider's reason when the test fails, as text", async () => {
    answer = (command) => {
      if (command === "companion_profiles") return [PROFILE];
      if (command === "companion_prefs") return { toolsEnabled: false, lastScope: null };
      if (command === "companion_usage") return [];
      if (command === "companion_test") {
        throw {
          kind: "providerError",
          message: "The provider didn't accept the request (HTTP 400).",
          providerReason: { speaker: "OpenAI", text: EVIL },
        };
      }
      return null;
    };
    await act(async () => {
      root.render(<CompanionSettings />);
    });
    await settle();
    await click(buttonIn(host, "Edit"));
    await click(buttonIn(host, "Test connection"));
    await settle();
    const said = host.querySelector('[data-testid="provider-said"]')!;
    expect(said).not.toBeNull();
    expect(said.textContent).toBe(`OpenAI said: ${EVIL}`);
    expect(host.querySelectorAll("a")).toHaveLength(0);
  });
});
