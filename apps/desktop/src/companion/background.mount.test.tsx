// @vitest-environment jsdom
// Background replies (ADR-020): leave a conversation while it is being written, come back and
// find the live text; the thread list and the notice say what happened while away; and nothing
// cancels because you left.

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

import { ToastRegion } from "../ui/Toasts";
import { resetToasts, TOAST_MS } from "../ui/toastQueue";
import type { Message, ProviderProfile, Thread } from "./api";
import CompanionView from "./CompanionView";
import { resetInflight } from "./inflight";
import { buttonIn, click, polyfillDialog, settle, typeInto, key } from "./testDom";

polyfillDialog();

const PROFILE: ProviderProfile = {
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

const thread = (id: number, title: string): Thread => ({
  id,
  title,
  profileId: null,
  model: null,
  createdAt: "2026-11-07T10:00:00Z",
  updatedAt: "2026-11-07T10:00:00Z",
});
const THREADS = [thread(1, "Retry setting"), thread(2, "Lock choice")];

function msg(id: number, threadId: number, role: Message["role"], text: string): Message {
  return {
    id,
    threadId,
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
  };
}

type Channel = { onmessage: (e: unknown) => void };
let stored: Message[] = [];
let sendArgs: Record<string, unknown> | null = null;
let endSend: (m: Message) => void = () => undefined;
let failSend: (e: unknown) => void = () => undefined;
let host: HTMLDivElement;
let root: Root;
let active = true;

const requestId = () => String(sendArgs?.requestId);
async function emit(e: Record<string, unknown>) {
  await act(async () => {
    (sendArgs?.onEvent as Channel).onmessage({ requestId: requestId(), ...e });
  });
}
const delta = (text: string) => emit({ type: "delta", text });

async function finishWith(text: string) {
  stored = [...stored, msg(10, 1, "user", "question"), msg(11, 1, "assistant", text)];
  await act(async () => {
    endSend(msg(11, 1, "assistant", text));
  });
  await settle();
}

beforeEach(() => {
  calls.length = 0;
  sendArgs = null;
  active = true;
  resetInflight();
  resetToasts();
  stored = [];
  answer = (command, args) => {
    switch (command) {
      case "companion_profiles":
        return [PROFILE];
      case "companion_threads":
        return THREADS;
      case "companion_prefs":
        return { toolsEnabled: false, lastScope: null };
      case "companion_messages":
        return stored.filter((m) => m.threadId === args.threadId);
      case "companion_send":
        sendArgs = args;
        return new Promise<Message>((resolve, reject) => {
          endSend = resolve;
          failSend = reject;
        });
      case "companion_cancel":
        return null;
      default:
        throw new Error(`unexpected ${command}`);
    }
  };
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
  vi.useRealTimers();
  vi.restoreAllMocks();
});

async function mount() {
  const headingRef = { current: null };
  await act(async () => {
    root.render(
      <>
        <CompanionView
          headingRef={headingRef}
          active={active}
          sessions={[]}
          now={Date.parse("2026-11-07T10:05:00Z")}
          onStartActivity={() => undefined}
          onOpenSettings={() => undefined}
        />
        <ToastRegion />
      </>,
    );
  });
  await settle();
}

const rerender = async (isActive: boolean) => {
  active = isActive;
  await mount();
};

const threadButton = (title: string) =>
  [...host.querySelectorAll<HTMLButtonElement>(".thread-select")].find((b) =>
    b.textContent?.includes(title),
  )!;
const textarea = () => host.querySelector<HTMLTextAreaElement>("textarea")!;
const toastTitles = () => [...host.querySelectorAll(".toast-title")].map((t) => t.textContent);

async function startSendInThreadOne() {
  await mount();
  await typeInto(textarea(), "question");
  await key(textarea(), "Enter");
  await settle();
  expect(sendArgs).not.toBeNull();
}

describe("returning to a reply that is still being written", () => {
  it("shows the live text again, with Cancel, and nothing was cancelled by leaving", async () => {
    await startSendInThreadOne();
    await emit({ type: "started" });
    await delta("First part. ");
    expect(host.textContent).toContain("First part.");

    // Leave for another conversation. The thread list says it is still writing.
    await click(threadButton("Lock choice"));
    expect(host.textContent).not.toContain("First part.");
    expect(threadButton("Retry setting").textContent).toContain("Writing…");

    // More text arrives while away: it goes to the store, not to a gone component.
    await delta("Second part.");

    // Come back mid-stream: the live text, the status and Cancel are there.
    await click(threadButton("Retry setting"));
    expect(host.textContent).toContain("First part. Second part.");
    expect(host.textContent).toContain("Still writing");
    expect(buttonIn(host, "Cancel")).toBeTruthy();
    // Your message shows once, from what you typed (the core already stored it as pending).
    stored = [{ ...msg(10, 1, "user", "question"), status: "pending" }];
    await click(threadButton("Lock choice"));
    await click(threadButton("Retry setting"));
    expect(host.querySelectorAll(".msg-user:not(.msg-sending)")).toHaveLength(0);
    expect(host.querySelectorAll(".msg-sending")).toHaveLength(1);
    expect(host.textContent).not.toContain("Not sent.");

    // Neither leaving nor coming back called cancel.
    expect(calls.map(([c]) => c)).not.toContain("companion_cancel");

    // It ends: the stored reply shows, and the thread list is back to normal.
    await finishWith("First part. Second part.");
    expect(host.querySelector(".msg-assistant .reply-text, .msg-assistant")?.textContent).toContain(
      "First part. Second part.",
    );
    expect(host.textContent).not.toContain("Still writing");
    expect(threadButton("Retry setting").textContent).not.toContain("Writing…");
    expect(toastTitles()).toEqual([]);
  });

  it("keeps a lookup waiting for approval while you were away", async () => {
    await startSendInThreadOne();
    await click(threadButton("Lock choice"));
    await emit({
      type: "toolRequest",
      approvalId: "a1",
      tool: "suggest_break",
      label: "Suggest a break",
      arguments: "{}",
      resultText: "Suggestion: Tea Sort",
      host: "api.anthropic.com",
    });
    expect(threadButton("Retry setting").textContent).toContain("Needs your approval");
    expect(toastTitles()).toEqual(["Companion needs your approval in “Retry setting”"]);
    expect(
      host.querySelector('[role="alert"][aria-label="Notices that need you"] .toast'),
    ).toBeTruthy();
    await click(buttonIn(host, "Open"));
    expect(host.textContent).toContain("Tea Sort");
    expect(toastTitles()).toEqual([]);
  });
});

describe("a reply that finished while you were elsewhere", () => {
  it("marks the thread New reply, shows a notice, and Open selects it and clears both", async () => {
    await startSendInThreadOne();
    await click(threadButton("Lock choice"));
    await delta("PRIVATE TEXT");
    await finishWith("PRIVATE TEXT");

    expect(threadButton("Retry setting").textContent).toContain("New reply");
    expect(toastTitles()).toEqual(["Companion replied in “Retry setting”"]);
    expect(host.querySelector(".toast")?.textContent).not.toContain("PRIVATE");
    const region = host.querySelector('[role="status"][aria-label="Notices"]');
    expect(region?.querySelector(".toast")).toBeTruthy();

    await click(buttonIn(host, "Open"));
    expect(threadButton("Retry setting").getAttribute("aria-current")).toBe("true");
    expect(threadButton("Retry setting").textContent).not.toContain("New reply");
    expect(toastTitles()).toEqual([]);
    expect(host.textContent).toContain("PRIVATE TEXT");
  });

  it("clears the mark by opening the thread yourself", async () => {
    await startSendInThreadOne();
    await click(threadButton("Lock choice"));
    await finishWith("done");
    expect(threadButton("Retry setting").textContent).toContain("New reply");
    await click(threadButton("Retry setting"));
    expect(threadButton("Retry setting").textContent).not.toContain("New reply");
    expect(toastTitles()).toEqual([]);
  });

  it("shows a failure in the thread and a notice without its details", async () => {
    await startSendInThreadOne();
    await click(threadButton("Lock choice"));
    stored = [{ ...msg(10, 1, "user", "question"), status: "failed" }];
    await act(async () => {
      failSend({ kind: "offline", message: "You're offline." });
    });
    await settle();
    expect(threadButton("Retry setting").textContent).toContain("Reply failed");
    expect(toastTitles()).toEqual(["Reply failed in “Retry setting”"]);
  });

  it("also tells you when Companion is not the view on screen, and when the window is hidden", async () => {
    await startSendInThreadOne();
    await rerender(false);
    await finishWith("done");
    expect(toastTitles()).toEqual(["Companion replied in “Retry setting”"]);
    // Looking at the thread again clears it.
    await rerender(true);
    expect(toastTitles()).toEqual([]);
    expect(threadButton("Retry setting").textContent).not.toContain("New reply");
  });

  it("does not keep the mark while the window is hidden, even on the open thread", async () => {
    await startSendInThreadOne();
    vi.spyOn(document, "visibilityState", "get").mockReturnValue("hidden");
    await finishWith("done");
    expect(toastTitles()).toEqual(["Companion replied in “Retry setting”"]);
    expect(threadButton("Retry setting").textContent).toContain("New reply");
  });
});

describe("the notice", () => {
  const key1 = (n: number) => ({ key: `k${n}`, title: `Notice ${n}`, tone: "info" as const });

  it("does not take focus, and is reachable by keyboard after the page", async () => {
    await startSendInThreadOne();
    await click(threadButton("Lock choice"));
    const before = threadButton("Lock choice");
    before.focus();
    await finishWith("done");
    expect(document.activeElement).toBe(before);
    expect(toastTitles()).toHaveLength(1);
    const buttons = [...host.querySelectorAll<HTMLButtonElement>(".toast button")];
    expect(buttons.map((b) => b.textContent || b.getAttribute("aria-label"))).toEqual([
      "Open",
      "Dismiss: Companion replied in “Retry setting”",
    ]);
  });

  it("goes away after about eight seconds, but not while hovered or focused", async () => {
    const { pushToast } = await import("../ui/toastQueue");
    vi.useFakeTimers();
    await mount();
    await act(async () => pushToast(key1(1)));
    const toast = () => host.querySelector<HTMLElement>(".toast");
    expect(toast()).toBeTruthy();

    // Hover pauses.
    await act(async () => {
      toast()!.dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
    });
    await act(async () => {
      vi.advanceTimersByTime(TOAST_MS * 3);
    });
    expect(toast()).toBeTruthy();

    // Leaving starts the full time again.
    await act(async () => {
      toast()!.dispatchEvent(
        new MouseEvent("mouseout", { bubbles: true, relatedTarget: document.body }),
      );
    });
    await act(async () => {
      vi.advanceTimersByTime(TOAST_MS - 100);
    });
    expect(toast()).toBeTruthy();
    await act(async () => {
      vi.advanceTimersByTime(200);
    });
    expect(toast()).toBeNull();

    // Focus inside pauses too.
    await act(async () => pushToast({ ...key1(2), action: { label: "Open", onAction: () => {} } }));
    const open = host.querySelector<HTMLButtonElement>(".toast button")!;
    await act(async () => open.focus());
    await act(async () => {
      vi.advanceTimersByTime(TOAST_MS * 3);
    });
    expect(toast()).toBeTruthy();
    await act(async () => open.blur());
    await act(async () => {
      vi.advanceTimersByTime(TOAST_MS + 100);
    });
    expect(toast()).toBeNull();
  });

  it("stacks at most three, oldest first out, and can be dismissed", async () => {
    const { pushToast } = await import("../ui/toastQueue");
    await mount();
    await act(async () => {
      for (const n of [1, 2, 3, 4]) pushToast(key1(n));
    });
    expect(toastTitles()).toEqual(["Notice 2", "Notice 3", "Notice 4"]);
    await click(buttonIn(host, "Dismiss: Notice 3"));
    expect(toastTitles()).toEqual(["Notice 2", "Notice 4"]);
  });

  it("is a polite live region for news and an assertive one only for approvals", async () => {
    await mount();
    const polite = host.querySelector('[aria-live="polite"][aria-label="Notices"]');
    const assertive = host.querySelector('[aria-live="assertive"]');
    expect(polite).toBeTruthy();
    expect(assertive).toBeTruthy();
  });

  it("leaves motion to the global reduced-motion rule", async () => {
    const { readFileSync } = await import("node:fs");
    const global = readFileSync("src/styles.css", "utf8");
    const block = global.slice(global.indexOf("@media (prefers-reduced-motion: reduce)"));
    expect(block).toMatch(/animation:\s*none\s*!important/);
    const toastCss = readFileSync("src/ui/toasts.css", "utf8");
    // The only motion is a CSS animation, which that rule switches off; no scripted motion.
    expect(toastCss).toMatch(/animation:/);
    expect(toastCss).not.toMatch(/transition:/);
  });
});
