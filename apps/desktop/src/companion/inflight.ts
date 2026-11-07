// Replies in flight, kept outside any component (ADR-020, background replies).
//
// The Rust core owns the truth: a send keeps running when you leave the conversation, and the
// reply is stored when it ends. This store only holds what the screen needs to show that request
// again when you come back: the streamed text, notices, a lookup waiting for you, and how it
// ended. It lives in memory only. After a restart the core marks an unfinished message failed
// and retryable, and nothing here pretends otherwise.
//
// Events come through the Channel of the call, whose handler writes here, never into component
// state, so leaving a thread, another view, or a hidden window loses nothing and cancels nothing.
// Several threads may stream at once; the core's busy rule is per thread.

import { useSyncExternalStore } from "react";
import { ellipsize, stripInvisible } from "../format";
import { dismissToastByKey, pushToast } from "../ui/toastQueue";
import {
  companionMessages,
  newRequestId,
  toCommandError,
  type CommandError,
  type Message,
  type StreamEvent,
} from "./api";
import { openThread } from "./events";
import { initialStream, isContextChanged, reduceStream, type StreamState } from "./logic";

/** How a request ended. `cancelled` is the user's own Cancel: no notice, nothing to open. */
export type Outcome =
  | { kind: "replied"; text: string; stored: boolean }
  | { kind: "failed"; error: CommandError; stored: boolean; contextChanged: boolean }
  | { kind: "cancelled"; stored: boolean };

export interface RequestMeta {
  threadId: number;
  /** The thread's title when the request started; used in notices, cut to one short line. */
  title: string;
  requestId: string;
  /** Your message as typed, shown until the stored copy replaces it. Null for a retry. */
  optimisticText: string | null;
  retryId: number | null;
  profileId: string;
  model: string;
  host: string | null;
  /** How many messages the thread had before this request. */
  before: number;
}

export interface InflightEntry extends RequestMeta {
  startedAt: number;
  stream: StreamState;
  status: "running" | "finished";
  /** Set once finished. The entry stays until the thread is opened (unread). */
  outcome: Outcome | null;
}

type Entries = ReadonlyMap<number, InflightEntry>;

let entries: Entries = new Map();
// The thread the user is looking at: Companion is the visible view and this one is selected.
let watched: number | null = null;
const listeners = new Set<() => void>();
const noopNow = () => Date.now();
let now = noopNow;

function notify(): void {
  for (const fn of [...listeners]) fn();
}

function commit(next: Entries): void {
  entries = next;
  notify();
}

function subscribe(fn: () => void): () => void {
  listeners.add(fn);
  return () => {
    listeners.delete(fn);
  };
}

if (typeof document !== "undefined") {
  // Coming back to a hidden window changes what "looking at it" means.
  document.addEventListener("visibilitychange", notify);
}

const windowVisible = () =>
  typeof document === "undefined" || document.visibilityState !== "hidden";

const toastKey = (threadId: number) => `companion:${threadId}`;

/** True while the user can see this conversation: selected, Companion showing, window visible. */
export function isWatching(threadId: number): boolean {
  return watched === threadId && windowVisible();
}

/** The Companion view tells the store which thread (if any) the user is looking at. */
export function setWatchedThread(threadId: number | null): void {
  if (watched === threadId) return;
  watched = threadId;
  if (threadId !== null) dismissToastByKey(toastKey(threadId));
  notify();
}

export function getEntry(threadId: number): InflightEntry | undefined {
  return entries.get(threadId);
}

export function getEntries(): Entries {
  return entries;
}

/** A short single-line title for a notice. Plain text only. */
export function noticeTitle(title: string): string {
  const oneLine = stripInvisible(title).replace(/\s+/g, " ").trim() || "Untitled conversation";
  return ellipsize(oneLine, 40);
}

function open(threadId: number): void {
  dismissToastByKey(toastKey(threadId));
  openThread(threadId);
}

function tell(entry: InflightEntry, kind: "replied" | "failed" | "approval"): void {
  const subject = noticeTitle(entry.title);
  const where = `“${subject}”`;
  const action = { label: "Open", onAction: () => open(entry.threadId) };
  const key = toastKey(entry.threadId);
  // Titles only. A reply, a prompt or an error's details never go into a notice. The title is
  // passed apart as `subject` so the notice can isolate it from the text around it.
  if (kind === "replied") {
    pushToast({ key, tone: "info", title: `Companion replied in ${where}`, subject, action });
  } else if (kind === "failed") {
    pushToast({ key, tone: "danger", title: `Reply failed in ${where}`, subject, action });
  } else {
    pushToast({
      key,
      tone: "attention",
      assertive: true,
      title: `Companion needs your approval in ${where}`,
      subject,
      action,
    });
  }
}

/** Starts tracking a request. Replaces what was left of an earlier request in that thread. */
export function beginRequest(meta: RequestMeta): void {
  const next = new Map(entries);
  next.set(meta.threadId, {
    ...meta,
    startedAt: now(),
    stream: initialStream(meta.requestId),
    status: "running",
    outcome: null,
  });
  commit(next);
}

/**
 * Applies one event of a request. Events of any other request (a late chunk of a cancelled one),
 * and events after the request ended, change nothing.
 */
export function applyEvent(threadId: number, event: StreamEvent): void {
  const entry = entries.get(threadId);
  if (!entry || entry.status !== "running" || entry.requestId !== event.requestId) return;
  const stream = reduceStream(entry.stream, event);
  if (stream === entry.stream) return;
  const next = new Map(entries);
  const updated = { ...entry, stream };
  next.set(threadId, updated);
  commit(next);
  if (!entry.stream.pending && stream.pending && !isWatching(threadId)) tell(updated, "approval");
  // The lookup was answered or the reply moved on: its notice is stale.
  if (entry.stream.pending && !stream.pending) dismissToastByKey(toastKey(threadId));
}

/** Ends a request. Tells the user if they are not looking at that thread. */
export function finishRequest(threadId: number, requestId: string, outcome: Outcome): void {
  const entry = entries.get(threadId);
  if (!entry || entry.requestId !== requestId || entry.status !== "running") return;
  const finished: InflightEntry = { ...entry, status: "finished", outcome };
  const next = new Map(entries);
  next.set(threadId, finished);
  commit(next);
  dismissToastByKey(toastKey(threadId));
  if (!isWatching(threadId) && outcome.kind !== "cancelled") {
    tell(finished, outcome.kind === "replied" ? "replied" : "failed");
  }
}

/** The user has seen how it ended (the thread is open): clears the unread mark and its notice. */
export function acknowledge(threadId: number, requestId?: string): void {
  const entry = entries.get(threadId);
  dismissToastByKey(toastKey(threadId));
  if (!entry || entry.status !== "finished") return;
  if (requestId !== undefined && entry.requestId !== requestId) return;
  const next = new Map(entries);
  next.delete(threadId);
  commit(next);
}

/** The conversation was deleted. */
export function dropThread(threadId: number): void {
  dismissToastByKey(toastKey(threadId));
  if (!entries.has(threadId)) return;
  const next = new Map(entries);
  next.delete(threadId);
  commit(next);
}

/** Replies finished while away, plus lookups waiting for you: what the nav badge counts. */
export function backgroundCount(e: Entries = entries): number {
  let n = 0;
  for (const entry of e.values()) {
    // The thread being looked at needs no badge.
    if (isWatching(entry.threadId)) continue;
    if (entry.status === "finished") {
      if (entry.outcome && entry.outcome.kind !== "cancelled") n += 1;
    } else if (entry.stream.pending) {
      n += 1;
    }
  }
  return n;
}

export type ThreadMark = "writing" | "approval" | "new" | "failed" | null;

/** What the thread list shows beside a conversation. */
export function threadMark(entry: InflightEntry | undefined): ThreadMark {
  if (!entry) return null;
  if (entry.status === "running") return entry.stream.pending ? "approval" : "writing";
  if (entry.outcome?.kind === "replied") return "new";
  if (entry.outcome?.kind === "failed") return "failed";
  return null;
}

async function storedCount(threadId: number): Promise<number | null> {
  try {
    return (await companionMessages(threadId)).length;
  } catch {
    return null;
  }
}

/**
 * Runs one send or retry to its end, independent of any screen. Never throws: the result is the
 * outcome, which is also stored for whoever shows this thread next. Cancelling the thread is the
 * user's Cancel button only; leaving the thread never reaches here.
 */
export async function runRequest(
  meta: Omit<RequestMeta, "requestId">,
  work: (requestId: string, onEvent: (e: StreamEvent) => void) => Promise<Message>,
): Promise<Outcome> {
  // Every event carries this id; anything with another id is ignored by the store.
  const requestId = newRequestId();
  beginRequest({ ...meta, requestId });
  let outcome: Outcome;
  try {
    const reply = await work(requestId, (e) => applyEvent(meta.threadId, e));
    const count = await storedCount(meta.threadId);
    outcome = {
      kind: "replied",
      text: reply.text,
      stored: count === null ? true : count > meta.before,
    };
  } catch (err) {
    const error = toCommandError(err);
    const count = await storedCount(meta.threadId);
    const stored = meta.retryId !== null || (count !== null ? count > meta.before : false);
    outcome =
      error.kind === "cancelled"
        ? { kind: "cancelled", stored }
        : {
            kind: "failed",
            error,
            stored,
            contextChanged: meta.retryId === null && isContextChanged(error),
          };
  }
  finishRequest(meta.threadId, requestId, outcome);
  return outcome;
}

export function useInflight(): Entries {
  return useSyncExternalStore(subscribe, getEntries, getEntries);
}

export function useThreadInflight(threadId: number): InflightEntry | undefined {
  return useSyncExternalStore(
    subscribe,
    () => entries.get(threadId),
    () => entries.get(threadId),
  );
}

export function useBackgroundCount(): number {
  return useSyncExternalStore(
    subscribe,
    () => backgroundCount(),
    () => backgroundCount(),
  );
}

export function useIsWatching(threadId: number): boolean {
  return useSyncExternalStore(
    subscribe,
    () => isWatching(threadId),
    () => isWatching(threadId),
  );
}

/** For tests: forget everything and use this clock. */
export function resetInflight(clock: () => number = noopNow): void {
  entries = new Map();
  watched = null;
  now = clock;
  notify();
}
