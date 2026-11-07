// @vitest-environment jsdom
// The in-flight store (ADR-020, background replies): requests run and receive events with no
// screen mounted, several threads at once, late chunks are ignored, and notices say only where.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

let stored = 0;
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn((command: string) =>
    command === "companion_messages"
      ? Promise.resolve(Array.from({ length: stored }, (_, i) => ({ id: i })))
      : Promise.resolve(null),
  ),
  Channel: class {},
}));

import { getToasts, resetToasts } from "../ui/toastQueue";
import type { Message, StreamEvent } from "./api";
import { onOpenThread } from "./events";
import {
  acknowledge,
  applyEvent,
  backgroundCount,
  beginRequest,
  dropThread,
  finishRequest,
  getEntry,
  isWatching,
  noticeTitle,
  resetInflight,
  runRequest,
  setWatchedThread,
  threadMark,
  type RequestMeta,
} from "./inflight";

const meta = (threadId: number, requestId: string, title = `Thread ${threadId}`): RequestMeta => ({
  threadId,
  title,
  requestId,
  optimisticText: "hello",
  retryId: null,
  profileId: "p",
  model: "m",
  host: "api.example.com",
  before: 0,
});

const delta = (requestId: string, text: string): StreamEvent => ({
  type: "delta",
  requestId,
  text,
});

const reply = (text: string) => ({ text }) as Message;

/** A send whose events and end the test controls. */
function controlled() {
  let emit: (e: StreamEvent) => void = () => undefined;
  let requestId = "";
  let end: (m: Message) => void = () => undefined;
  let fail: (e: unknown) => void = () => undefined;
  let markStarted: () => void = () => undefined;
  const started = new Promise<void>((resolve) => {
    markStarted = resolve;
  });
  const work = (id: string, onEvent: (e: StreamEvent) => void) => {
    requestId = id;
    emit = onEvent;
    markStarted();
    return new Promise<Message>((res, rej) => {
      end = res;
      fail = rej;
    });
  };
  return {
    work,
    started,
    emit: (e: Record<string, unknown>) => emit({ ...e, requestId } as unknown as StreamEvent),
    id: () => requestId,
    end: (text: string) => end(reply(text)),
    fail: (e: unknown) => fail(e),
  };
}

const tick = () => new Promise((r) => setTimeout(r, 0));

beforeEach(() => {
  stored = 0;
  resetInflight();
  resetToasts();
});
afterEach(() => {
  vi.restoreAllMocks();
});

describe("a request with no screen", () => {
  it("keeps receiving events and ends with an outcome nobody is looking at", async () => {
    const c = controlled();
    const done = runRequest(meta(1, "x"), c.work);
    await c.started;
    c.emit({ type: "delta", text: "Hel" });
    c.emit({ type: "delta", text: "lo" });
    expect(getEntry(1)?.stream.text).toBe("Hello");
    expect(getEntry(1)?.status).toBe("running");
    stored = 2;
    c.end("Hello");
    const outcome = await done;
    expect(outcome).toMatchObject({ kind: "replied", stored: true });
    expect(getEntry(1)?.status).toBe("finished");
    // Unread until the thread is opened.
    expect(threadMark(getEntry(1))).toBe("new");
    expect(backgroundCount()).toBe(1);
  });

  it("streams two threads at once without mixing them", async () => {
    const a = controlled();
    const b = controlled();
    const pa = runRequest(meta(1, "x"), a.work);
    const pb = runRequest(meta(2, "x"), b.work);
    await Promise.all([a.started, b.started]);
    a.emit({ type: "delta", text: "alpha" });
    b.emit({ type: "delta", text: "beta" });
    a.emit({ type: "delta", text: "!" });
    expect(getEntry(1)?.stream.text).toBe("alpha!");
    expect(getEntry(2)?.stream.text).toBe("beta");
    expect(threadMark(getEntry(1))).toBe("writing");
    expect(threadMark(getEntry(2))).toBe("writing");
    a.end("alpha!");
    await pa;
    expect(getEntry(1)?.status).toBe("finished");
    expect(getEntry(2)?.status).toBe("running");
    b.end("beta");
    await pb;
    expect(backgroundCount()).toBe(2);
  });

  it("ignores late chunks of a request that was cancelled", async () => {
    const first = controlled();
    const p1 = runRequest(meta(1, "x"), first.work);
    await first.started;
    const firstId = first.id();
    first.fail({ kind: "cancelled", message: "Cancelled." });
    expect(await p1).toMatchObject({ kind: "cancelled" });
    // A new request in the same thread starts; the cancelled one's chunks arrive afterwards.
    const second = controlled();
    const p2 = runRequest(meta(1, "x"), second.work);
    await second.started;
    applyEvent(1, delta(firstId, "LATE"));
    second.emit({ type: "delta", text: "fresh" });
    expect(getEntry(1)?.stream.text).toBe("fresh");
    second.end("fresh");
    await p2;
    // And chunks after the end change nothing.
    applyEvent(1, delta(second.id(), "AFTER"));
    expect(getEntry(1)?.stream.text).toBe("fresh");
  });

  it("does not let an old request's end clobber a newer one", async () => {
    beginRequest(meta(1, "old"));
    beginRequest(meta(1, "new"));
    finishRequest(1, "old", { kind: "replied", text: "t", stored: true });
    expect(getEntry(1)?.status).toBe("running");
    expect(getEntry(1)?.requestId).toBe("new");
  });

  it("clears the outcome when the thread is opened, and only then", async () => {
    beginRequest(meta(1, "r"));
    finishRequest(1, "r", { kind: "replied", text: "t", stored: true });
    expect(getEntry(1)?.status).toBe("finished");
    acknowledge(1, "some-other-request");
    expect(getEntry(1)).toBeDefined();
    acknowledge(1, "r");
    expect(getEntry(1)).toBeUndefined();
    expect(backgroundCount()).toBe(0);
    // A running request is not cleared by opening the thread.
    beginRequest(meta(2, "s"));
    acknowledge(2);
    expect(getEntry(2)?.status).toBe("running");
  });

  it("marks a failed request and keeps the error for the thread to show", async () => {
    const c = controlled();
    const done = runRequest(meta(1, "x"), c.work);
    await c.started;
    c.fail({ kind: "offline", message: "You're offline." });
    const outcome = await done;
    expect(outcome).toMatchObject({ kind: "failed", stored: false, contextChanged: false });
    expect(threadMark(getEntry(1))).toBe("failed");
  });

  it("forgets a deleted conversation", () => {
    beginRequest(meta(1, "r"));
    dropThread(1);
    expect(getEntry(1)).toBeUndefined();
  });
});

describe("a lookup waiting for approval", () => {
  const request = (id: string): StreamEvent => ({
    type: "toolRequest",
    requestId: id,
    approvalId: "a1",
    tool: "suggest_break",
    label: "Suggest a break",
    arguments: "{}",
    resultText: "Suggestion: Tea Sort",
    host: "api.example.com",
  });

  it("shows on the thread and counts for the badge until it is answered", () => {
    beginRequest(meta(1, "r"));
    expect(backgroundCount()).toBe(0);
    applyEvent(1, request("r"));
    expect(threadMark(getEntry(1))).toBe("approval");
    expect(backgroundCount()).toBe(1);
    applyEvent(1, { type: "toolResolved", requestId: "r", approvalId: "a1", approved: true });
    expect(threadMark(getEntry(1))).toBe("writing");
    expect(backgroundCount()).toBe(0);
  });
});

describe("notices", () => {
  it("tells you where a reply landed, with the title only, when you are elsewhere", async () => {
    const c = controlled();
    const done = runRequest(meta(1, "x", "Retry   setting\nnaming"), c.work);
    await c.started;
    c.emit({ type: "delta", text: "A SECRET REPLY" });
    c.end("A SECRET REPLY");
    await done;
    const toasts = getToasts();
    expect(toasts).toHaveLength(1);
    expect(toasts[0]).toMatchObject({ tone: "info" });
    expect(toasts[0]!.assertive).toBeUndefined();
    expect(toasts[0]!.title).toBe("Companion replied in “Retry setting naming”");
    expect(toasts[0]!.title).not.toContain("SECRET");
    expect(toasts[0]!.action?.label).toBe("Open");
  });

  it("says nothing while you are looking at that thread", async () => {
    setWatchedThread(1);
    expect(isWatching(1)).toBe(true);
    const c = controlled();
    const done = runRequest(meta(1, "x"), c.work);
    await c.started;
    c.end("hi");
    await done;
    expect(getToasts()).toHaveLength(0);
  });

  it("speaks up when the window is hidden even on the open thread", async () => {
    setWatchedThread(1);
    vi.spyOn(document, "visibilityState", "get").mockReturnValue("hidden");
    expect(isWatching(1)).toBe(false);
    const c = controlled();
    const done = runRequest(meta(1, "x"), c.work);
    await c.started;
    c.end("hi");
    await done;
    expect(getToasts()).toHaveLength(1);
  });

  it("says nothing about your own Cancel", async () => {
    const c = controlled();
    const done = runRequest(meta(1, "x"), c.work);
    await c.started;
    c.fail({ kind: "cancelled", message: "Cancelled." });
    await done;
    expect(getToasts()).toHaveLength(0);
  });

  it("reports a failure without its details", async () => {
    const c = controlled();
    const done = runRequest(meta(1, "x"), c.work);
    await c.started;
    c.fail({ kind: "providerError", message: "secret detail from the provider" });
    await done;
    expect(getToasts()[0]).toMatchObject({ tone: "danger" });
    expect(getToasts()[0]!.title).toBe("Reply failed in “Thread 1”");
  });

  it("asks for an approval assertively, and the notice is replaced by the result", async () => {
    const c = controlled();
    const done = runRequest(meta(1, "x"), c.work);
    await c.started;
    c.emit({
      type: "toolRequest",
      approvalId: "a1",
      tool: "suggest_break",
      label: "Suggest a break",
      arguments: "{}",
      resultText: "Suggestion: Tea Sort",
      host: "h",
    });
    expect(getToasts()).toHaveLength(1);
    expect(getToasts()[0]).toMatchObject({ assertive: true, tone: "attention" });
    expect(getToasts()[0]!.title).toBe("Companion needs your approval in “Thread 1”");
    expect(getToasts()[0]!.title).not.toContain("Tea Sort");
    c.emit({ type: "toolResolved", approvalId: "a1", approved: true });
    expect(getToasts()).toHaveLength(0);
    c.end("done");
    await done;
    expect(getToasts()).toHaveLength(1);
    expect(getToasts()[0]!.assertive).toBeUndefined();
  });

  it("opens the thread from the notice and drops the notice", async () => {
    const opened: number[] = [];
    const off = onOpenThread((id) => opened.push(id));
    const c = controlled();
    const done = runRequest(meta(4, "x"), c.work);
    await c.started;
    c.end("hi");
    await done;
    getToasts()[0]!.action!.onAction();
    expect(opened).toEqual([4]);
    expect(getToasts()).toHaveLength(0);
    off();
  });

  it("removes a thread's notice when you start looking at that thread", async () => {
    const c = controlled();
    const done = runRequest(meta(1, "x"), c.work);
    await c.started;
    c.end("hi");
    await done;
    expect(getToasts()).toHaveLength(1);
    setWatchedThread(1);
    expect(getToasts()).toHaveLength(0);
  });

  it("cuts long titles and never needs more than a short line", () => {
    expect(noticeTitle("x".repeat(100)).length).toBeLessThanOrEqual(40);
    expect(noticeTitle("   ")).toBe("Untitled conversation");
  });
});

describe("nothing cancels by leaving", () => {
  it("never calls the cancel command for navigation or unwatching", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    const c = controlled();
    const done = runRequest(meta(1, "x"), c.work);
    await c.started;
    setWatchedThread(1);
    setWatchedThread(2);
    setWatchedThread(null);
    await tick();
    const called = vi.mocked(invoke).mock.calls.map(([name]) => name);
    expect(called).not.toContain("companion_cancel");
    expect(getEntry(1)?.status).toBe("running");
    c.end("ok");
    await done;
  });
});
