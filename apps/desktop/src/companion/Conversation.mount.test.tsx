// @vitest-environment jsdom
// The conversation and composer of decision 36, mounted with the Tauri bridge mocked:
// Enter sends and Shift+Enter or an input method does not, the model pick applies to one message
// and then goes back to the default, every answer shows the model that wrote it, and the chosen
// options show as removable badges.

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

type Call = [command: string, args: Record<string, unknown>];
const calls: Call[] = [];
let answer: (command: string, args: Record<string, unknown>) => unknown = () => null;

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args: Record<string, unknown> = {}) => {
    calls.push([command, args]);
    try {
      return Promise.resolve(answer(command, args));
    } catch (err) {
      return Promise.reject(err instanceof Error ? err : new Error(String(err)));
    }
  },
  Channel: class {},
}));

import { SESSIONS } from "../demo/data";
import type { Message, ProviderProfile, Thread } from "./api";
import { Composer } from "./Composer";
import { Conversation } from "./Conversation";
import { buttonIn, click, key, polyfillDialog, settle, typeInto } from "./testDom";

polyfillDialog();

const A: ProviderProfile = {
  id: "pa",
  kind: "anthropic",
  label: "Provider A",
  baseUrl: "https://api.anthropic.com",
  host: "api.anthropic.com",
  model: "a1",
  models: [
    { model: "a1", label: null, isDefault: true },
    { model: "a2", label: "Two", isDefault: false },
  ],
  monthlyTokenBudget: null,
  isDefault: true,
  hasKey: true,
  keychainOk: true,
};
const B: ProviderProfile = {
  ...A,
  id: "pb",
  label: "Provider B",
  model: "b1",
  models: [{ model: "b1", label: null, isDefault: true }],
  isDefault: false,
};
const THREAD: Thread = {
  id: 7,
  title: "T",
  profileId: null,
  model: null,
  createdAt: "2026-11-07T10:00:00Z",
  updatedAt: "2026-11-07T10:00:00Z",
};

function msg(
  id: number,
  role: Message["role"],
  text: string,
  more: Partial<Message> = {},
): Message {
  return {
    id,
    threadId: THREAD.id,
    role,
    text,
    attachedContext: null,
    scopeKind: "none",
    host: "api.anthropic.com",
    profileId: "pa",
    model: "a1",
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

let stored: Message[] = [];
let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  calls.length = 0;
  stored = [
    msg(1, "user", "q"),
    msg(2, "assistant", "first answer"),
    msg(3, "assistant", "old answer", { model: null, profileId: null }),
  ];
  answer = (command, args) => {
    if (command === "companion_messages") return stored;
    if (command === "companion_send") {
      const reply = msg(stored.length + 2, "assistant", "ok", {
        profileId: String(args.profileId),
        model: String(args.model),
      });
      stored = [...stored, msg(stored.length + 1, "user", String(args.text)), reply];
      return reply;
    }
    throw new Error(`unexpected ${command}`);
  };
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
});

async function mountConversation() {
  await act(async () => {
    root.render(
      <Conversation
        thread={THREAD}
        profiles={[A, B]}
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
}

const textarea = () => host.querySelector<HTMLTextAreaElement>("textarea")!;
const sends = () => calls.filter(([c]) => c === "companion_send").map(([, a]) => a);
const modelButton = () => host.querySelector<HTMLButtonElement>(".composer-model")!;

describe("answers and the model badge", () => {
  it("shows which model and provider wrote each answer, and says so when unknown", async () => {
    await mountConversation();
    const badges = [...host.querySelectorAll(".msg-model")].map((b) => b.textContent);
    expect(badges).toEqual(["Answered by a1 · Provider A", "Answered by Model unknown"]);
    // Your own messages carry no badge.
    expect(host.querySelectorAll(".msg-user .msg-model")).toHaveLength(0);
  });
});

describe("Enter and Shift+Enter", () => {
  it("sends on Enter and Ctrl+Enter, never on Shift+Enter or while composing", async () => {
    await mountConversation();
    await typeInto(textarea(), "hello");
    expect(await key(textarea(), "Enter", { shiftKey: true })).toBe(true);
    await key(textarea(), "Enter", { isComposing: true });
    await key(textarea(), "Enter", { keyCode: 229 });
    expect(sends()).toHaveLength(0);
    expect(await key(textarea(), "Enter")).toBe(false);
    await settle();
    expect(sends()).toHaveLength(1);
    expect(sends()[0]).toMatchObject({ text: "hello", profileId: "pa", model: "a1" });
    expect(textarea().value).toBe("");
    await typeInto(textarea(), "again");
    await key(textarea(), "Enter", { ctrlKey: true });
    await settle();
    expect(sends()).toHaveLength(2);
    // The key help is reachable by assistive tech and as a tooltip, but not printed as a line.
    const described = document.getElementById(textarea().getAttribute("aria-describedby")!)!;
    expect(described.textContent).toBe("Enter sends. Shift+Enter starts a new line.");
    expect(described.classList.contains("visually-hidden")).toBe(true);
    expect(textarea().title).toBe("Enter sends. Shift+Enter starts a new line.");
  });
});

describe("the model is picked per message", () => {
  it("uses the pick for one send, then goes back to the default", async () => {
    await mountConversation();
    expect(modelButton().textContent).toContain("a1 · Provider A");
    await click(modelButton());
    const dialog = host.querySelector("dialog[open]")!;
    expect(dialog.textContent).toContain("Provider A");
    expect(dialog.textContent).toContain("Provider B");
    const radios = [...dialog.querySelectorAll<HTMLInputElement>('input[type="radio"]')];
    expect(radios).toHaveLength(3);
    await click(radios[1]!);
    await click(buttonIn(dialog, "Done"));
    expect(modelButton().textContent).toContain("Two · Provider A");
    expect(modelButton().textContent).toContain("this message only");

    await typeInto(textarea(), "use two");
    await key(textarea(), "Enter");
    await settle();
    expect(sends()[0]).toMatchObject({ profileId: "pa", model: "a2" });
    // Back to the default for the next message.
    expect(modelButton().textContent).toContain("a1 · Provider A");
    expect(modelButton().textContent).not.toContain("this message only");
    // The new answer's badge names the model that wrote it.
    const badges = [...host.querySelectorAll(".msg-model")].map((b) => b.textContent);
    expect(badges.at(-1)).toBe("Answered by Two · Provider A");

    await typeInto(textarea(), "next");
    await key(textarea(), "Enter");
    await settle();
    expect(sends()[1]).toMatchObject({ profileId: "pa", model: "a1" });
  });
});

describe("conversation options as badges", () => {
  it("shows the chosen context as badges that reopen the options or remove them", async () => {
    const session = SESSIONS[0]!.view;
    await act(async () => {
      root.render(
        <Composer
          profiles={[A]}
          choice={{ profileId: "pa", model: "a1" }}
          onPickChoice={() => undefined}
          sessions={[session]}
          sending={false}
          historyCount={0}
          rememberedScope={null}
          onSend={() => Promise.resolve({ stored: true, contextChanged: false })}
          onCancel={() => undefined}
          onOpenSettings={() => undefined}
        />,
      );
    });
    await settle();
    expect(host.querySelector(".option-badges")).toBeNull();
    await click(buttonIn(host, "Options…"));
    const options = [...host.querySelectorAll("dialog")].find((d) =>
      d.textContent?.includes("Conversation options"),
    )!;
    expect(options.hasAttribute("open")).toBe(true);
    const summary = [...options.querySelectorAll<HTMLInputElement>('input[type="radio"]')][1]!;
    await click(summary);
    const select = options.querySelector<HTMLSelectElement>("select")!;
    await act(async () => {
      select.value = "0";
      select.dispatchEvent(new Event("change", { bubbles: true }));
    });
    await click(buttonIn(options, "Done"));
    expect(options.hasAttribute("open")).toBe(false);

    const badge = host.querySelector<HTMLButtonElement>(".option-badge-text")!;
    expect(badge.textContent).toMatch(/^Session summary: /);
    // Clicking the badge reopens the options.
    await click(badge);
    expect(options.hasAttribute("open")).toBe(true);
    await click(buttonIn(options, "Done"));
    // × removes it, with its own accessible name.
    const remove = host.querySelector<HTMLButtonElement>(".option-badge-remove")!;
    expect(remove.getAttribute("aria-label")).toMatch(/^Remove Session summary: /);
    await click(remove);
    expect(host.querySelector(".option-badges")).toBeNull();
    expect(host.textContent).not.toContain("This message sends only what you typed.");
  });
});

describe("a chat-style layout", () => {
  it("keeps the message box outside the scrolling message list", async () => {
    await mountConversation();
    const scroller = host.querySelector(".messages")!;
    expect(scroller).not.toBeNull();
    expect(scroller.contains(textarea())).toBe(false);
    expect(scroller.contains(host.querySelector(".composer"))).toBe(false);
    const conversation = host.querySelector(".conversation")!;
    expect(conversation.contains(scroller) && conversation.contains(textarea())).toBe(true);
  });

  it("right-aligns your messages as bubbles and leaves replies as plain full-width text", async () => {
    await mountConversation();
    const mine = host.querySelector(".msg-user")!;
    expect(mine.querySelector(".msg-bubble")).not.toBeNull();
    const theirs = host.querySelectorAll(".msg-assistant");
    expect(theirs).toHaveLength(2);
    for (const t of theirs) expect(t.querySelector(".msg-bubble")).toBeNull();
    // Names stay available to assistive tech without printing a big label.
    expect(mine.querySelector(".msg-who")?.textContent).toBe("You");
    expect(mine.querySelector(".msg-who")?.classList.contains("visually-hidden")).toBe(true);
    expect(theirs[0]!.querySelector(".msg-who")?.textContent).toBe("Companion");
    expect(theirs[0]!.querySelector(".msg-avatar img")).not.toBeNull();
    expect(mine.querySelector(".msg-avatar svg")).not.toBeNull();
  });

  it("prints no always-on help lines and no destination badge in the composer", async () => {
    await mountConversation();
    for (const sel of [".composer-hint", ".composer-history"]) {
      expect(host.querySelector(sel)).toBeNull();
    }
    const visible = host.querySelector(".composer")!.cloneNode(true) as HTMLElement;
    visible.querySelectorAll(".visually-hidden, dialog").forEach((n) => n.remove());
    const text = visible.textContent ?? "";
    expect(text).not.toContain("Enter sends");
    expect(text).not.toContain("earlier message");
    expect(text).not.toContain("Sends to");
    expect(text).not.toContain("only what you typed");
    // The destination is still on the model chip, as a tooltip.
    expect(modelButton().title).toBe("Sends to api.anthropic.com");
  });

  it("says how much history goes along in the options dialog", async () => {
    await mountConversation();
    await click(buttonIn(host, "Options…"));
    const options = [...host.querySelectorAll("dialog")].find((d) =>
      d.textContent?.includes("Conversation options"),
    )!;
    expect(options.querySelector(".options-history")?.textContent).toMatch(/earlier message/);
  });

  it("shows a quiet chip only on messages that shared context", async () => {
    stored = [
      msg(1, "user", "plain"),
      msg(2, "assistant", "ok"),
      msg(3, "user", "with context", {
        scopeKind: "sessionSummary",
        attachedContext: "Session: demo",
      }),
      msg(4, "assistant", "ok again"),
    ];
    await mountConversation();
    const chips = host.querySelectorAll(".msg-shared");
    expect(chips).toHaveLength(1);
    expect(chips[0]!.closest(".msg-user")?.textContent).toContain("with context");
    expect(chips[0]!.querySelector("summary")?.textContent).toMatch(/Shared/);
    expect(chips[0]!.textContent).toContain("Session: demo");
    expect(host.textContent).not.toContain("Nothing shared");
  });
});
