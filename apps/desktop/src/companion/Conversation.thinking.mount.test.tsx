// @vitest-environment jsdom
// The "Thinking" section of a Companion reply (ADR-020, Thinking), mounted with the Tauri bridge
// mocked: it streams live above the reply, is open while the model thinks and collapsed once the
// reply starts, survives leaving and returning to the conversation, and is shown for stored
// replies after a reload. Model output is plain text, never elements.

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

import type { Message, ProviderProfile, StreamEvent, Thread } from "./api";
import { Conversation } from "./Conversation";
import { resetInflight } from "./inflight";
import { resetChoices } from "./thinkingChoice";
import { click, key, polyfillDialog, settle, typeInto } from "./testDom";

polyfillDialog();

const A: ProviderProfile = {
  id: "pa",
  kind: "anthropic",
  label: "Provider A",
  baseUrl: "https://api.anthropic.com",
  host: "api.anthropic.com",
  model: "a1",
  models: [{ model: "a1", label: null, isDefault: true }],
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
  createdAt: "2026-11-08T10:00:00Z",
  updatedAt: "2026-11-08T10:00:00Z",
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
    createdAt: "2026-11-08T10:00:00Z",
    toolCalls: [],
    toolNotices: [],
    ...more,
  };
}

let stored: Message[] = [];
let emit: (e: Omit<StreamEvent, "requestId"> & { text?: string }) => void = () => undefined;
let finish: () => void = () => undefined;
let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  resetInflight();
  resetChoices();
  calls.length = 0;
  stored = [];
  answer = (command, args) => {
    if (command === "companion_messages") return stored;
    if (command === "companion_send") {
      const channel = args.onEvent as { onmessage: (e: StreamEvent) => void };
      const requestId = String(args.requestId);
      emit = (e) => channel.onmessage({ ...e, requestId } as StreamEvent);
      return new Promise<Message>((resolve) => {
        finish = () => {
          const reply = msg(2, "assistant", "The answer.", {
            thinking: "Weighing it up.",
            thinkingMs: 4000,
          });
          stored = [msg(1, "user", "q"), reply];
          resolve(reply);
        };
      });
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

async function mount() {
  await act(async () => {
    root.render(
      <Conversation
        thread={THREAD}
        profiles={[A]}
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

const head = () => host.querySelector<HTMLButtonElement>(".thinking-head");
const body = () => host.querySelector<HTMLElement>(".thinking-body");

async function send() {
  const ta = host.querySelector<HTMLTextAreaElement>("textarea")!;
  await typeInto(ta, "q");
  await key(ta, "Enter");
  await settle();
}

describe("live thinking", () => {
  it("streams above the reply, is open while thinking and collapses when the reply starts", async () => {
    await mount();
    await send();
    await act(async () => emit({ type: "thinking", text: "Pondering " }));
    await act(async () => emit({ type: "thinking", text: "the options." }));
    expect(head()!.getAttribute("aria-expanded")).toBe("true");
    expect(head()!.textContent).toBe("Thinking…");
    expect(body()!.textContent).toBe("Pondering the options.");
    expect(body()!.hidden).toBe(false);

    await act(async () => emit({ type: "delta", text: "Hello" }));
    expect(head()!.getAttribute("aria-expanded")).toBe("false");
    expect(head()!.textContent).toMatch(/^Thought for \d+ s$/);
    expect(body()!.hidden).toBe(true);
    // The thinking is not part of the reply, and comes before it.
    const li = host.querySelector(".msg-streaming")!;
    expect(li.querySelector(".reply-text")!.textContent).toContain("Hello");
    expect(li.querySelector(".reply-text")!.textContent).not.toContain("Pondering");
    expect(li.querySelector(".thinking")!.nextElementSibling!.className).toContain("reply-text");

    // Opening it by hand is kept when the reply finishes.
    await click(head()!);
    expect(head()!.getAttribute("aria-expanded")).toBe("true");
    await act(async () => emit({ type: "delta", text: " world" }));
    expect(head()!.getAttribute("aria-expanded")).toBe("true");
  });

  it("carries the open choice from the live block to the stored reply", async () => {
    await mount();
    await send();
    await act(async () => emit({ type: "thinking", text: "Hmm." }));
    await act(async () => emit({ type: "delta", text: "Hi" }));
    expect(head()!.getAttribute("aria-expanded")).toBe("false");
    await click(head()!); // the user opens it
    await act(async () => {
      finish();
    });
    await settle();
    // The stored reply (different component, different key) keeps it open.
    expect(host.querySelector(".msg-streaming")).toBeNull();
    expect(head()!.getAttribute("aria-expanded")).toBe("true");
    expect(body()!.hidden).toBe(false);
  });

  it("is not 'Thinking…' while a lookup waits for you, or after reply text came", async () => {
    await mount();
    await send();
    await act(async () => emit({ type: "thinking", text: "Need data." }));
    expect(head()!.textContent).toBe("Thinking…");
    await act(async () =>
      emit({
        type: "toolRequest",
        approvalId: "a1",
        tool: "recent_sessions",
        label: "Recent sessions",
        arguments: "{}",
        resultText: "x",
        host: "api.anthropic.com",
      } as never),
    );
    expect(head()!.textContent).toMatch(/^Thought for \d+ s$/);
  });

  it("keeps the live thinking when you leave the conversation and come back", async () => {
    await mount();
    await send();
    await act(async () => emit({ type: "thinking", text: "Still thinking." }));
    act(() => root.unmount());
    root = createRoot(host);
    await mount();
    expect(body()!.textContent).toBe("Still thinking.");
    expect(head()!.getAttribute("aria-expanded")).toBe("true");
    await act(async () => {
      finish();
    });
    await settle();
  });

  it("shows thinking as text, never elements", async () => {
    await mount();
    await send();
    await act(async () =>
      emit({ type: "thinking", text: "<img src=x onerror=alert(1)> <b>bold</b>‮" }),
    );
    expect(body()!.querySelector("img, b")).toBeNull();
    expect(body()!.textContent).toBe("<img src=x onerror=alert(1)> <b>bold</b>");
  });
});

describe("stored thinking", () => {
  it("shows a collapsed section above the reply after a reload, and a keyboard toggle", async () => {
    stored = [
      msg(1, "user", "q"),
      msg(2, "assistant", "The answer.", { thinking: "Because.\nSecond.", thinkingMs: 12_000 }),
      msg(3, "assistant", "No thinking here."),
    ];
    await mount();
    expect(host.querySelectorAll(".thinking")).toHaveLength(1);
    expect(head()!.textContent).toBe("Thought for 12 s");
    expect(head()!.getAttribute("aria-expanded")).toBe("false");
    expect(body()!.hidden).toBe(true);
    expect(body()!.textContent).toBe("Because.\nSecond.");
    const li = head()!.closest("li")!;
    expect(
      li.querySelector(".thinking")!.compareDocumentPosition(li.querySelector(".reply-text")!),
    ).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
    expect(li.querySelector(".reply-text")!.textContent).toContain("The answer.");
    expect(li.querySelector(".reply-text")!.textContent).not.toContain("Because");
    // A real button: aria-controls points at the body, a click (Enter and Space send one) toggles.
    expect(head()!.getAttribute("aria-controls")).toBe(body()!.id);
    await click(head()!);
    expect(head()!.getAttribute("aria-expanded")).toBe("true");
    expect(body()!.hidden).toBe(false);
    await click(head()!);
    expect(body()!.hidden).toBe(true);
  });
});
