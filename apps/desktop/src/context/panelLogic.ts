// Pure decisions for the session detail panel (S04-T07): layout, grouping, labels and the text
// that Copy puts on the clipboard. No React and no Tauri here, so the wording is unit-tested.

import { groupSessions } from "../format";
import { liveState, type SessionView } from "../sessions";
import type { ItemKind, NextMoveCategory, SessionItem, SessionKey, TimelineEntry } from "./api";

/** Under this window width the panel always floats (ADR-013). */
export const FLOAT_BELOW_PX = 720;
/** Wide enough for both Live columns beside a docked panel. */
export const TWO_COLUMNS_MIN_PX = 1150;

export interface LayoutDecision {
  /** How the panel sits, or null when no session is open. */
  panel: "docked" | "floating" | null;
  /** "columns": the normal Needs you / Working columns. "grouped-list": one list, compact rows. */
  board: "columns" | "grouped-list";
}

/**
 * Pinned panels dock on the right and the board makes room; floating ones overlay an unchanged
 * board. A narrow window floats the panel without touching the saved preference.
 */
export function decideLayout(open: boolean, pinned: boolean, windowWidth: number): LayoutDecision {
  if (!open) return { panel: null, board: "columns" };
  if (!pinned || windowWidth < FLOAT_BELOW_PX) return { panel: "floating", board: "columns" };
  return {
    panel: "docked",
    board: windowWidth >= TWO_COLUMNS_MIN_PX ? "columns" : "grouped-list",
  };
}

/** The grouped list shows "Needs you" pinned on top and "Working" scrolling below it. */
export function groupedList(sorted: readonly SessionView[]) {
  const { needsYou, working } = groupSessions(sorted);
  return { pinned: needsYou, scrolling: working };
}

export const sameSession = (a: SessionKey | null, b: SessionKey | null) =>
  a !== null && b !== null && a.provider === b.provider && a.sessionId === b.sessionId;

export const viewKey = (s: SessionView): SessionKey => ({
  provider: s.provider,
  sessionId: s.providerSessionId,
});

export const keyString = (k: SessionKey) => `${k.provider}:${k.sessionId}`;

/** The small "Review" pill is shown only for sessions that need a review. */
export const showsReviewPill = (s: Pick<SessionView, "needsReview">) => s.needsReview;

/** Which of the panel's states a session is in. */
export type PanelMode = "needs_you" | "working" | "history";

export function panelMode(s: Pick<SessionView, "state" | "disconnectedUtc">): PanelMode {
  const state = liveState(s);
  if (state === "needs_attention") return "needs_you";
  if (state === "working") return "working";
  return "history";
}

const EVENT_LABELS: Record<string, string> = {
  session_started: "Session started",
  working: "Started working",
  needs_attention: "Needed you",
  turn_ended: "Finished a turn (not necessarily the task)",
  completed: "Reported completed",
  failed: "Reported failed",
  cancelled: "Was cancelled",
  session_ended: "Session ended",
  heartbeat: "Still connected",
};

const REASON_LABELS: Record<string, string> = {
  permission: "asked for permission",
  prompt_input_exit: "waiting for your input",
  idle: "waiting for you",
  idle_prompt: "waiting for you",
};

/** Plain-language timeline label; unknown events and reasons are shown as they are, never hidden. */
export function timelineLabel(e: Pick<TimelineEntry, "eventType" | "reason">): string {
  const base = EVENT_LABELS[e.eventType] ?? `Event: ${e.eventType.replaceAll("_", " ")}`;
  if (e.eventType !== "needs_attention" || !e.reason) return base;
  return `${base} (${REASON_LABELS[e.reason] ?? e.reason.replaceAll("_", " ")})`;
}

/** What the agent is asking, in words, when the adapter didn't report the text (consent off). */
export function askFallback(timeline: readonly TimelineEntry[]): string {
  const last = [...timeline].reverse().find((e) => e.eventType === "needs_attention");
  switch (last?.reason) {
    case "permission":
      return "It's asking for your permission to continue.";
    case "prompt_input_exit":
    case "idle":
    case "idle_prompt":
      return "It's waiting for your input.";
    default:
      return "It's waiting for you, but it didn't say why.";
  }
}

export const CATEGORIES: { id: NextMoveCategory; label: string }[] = [
  { id: "question", label: "Question" },
  { id: "acceptance", label: "Acceptance criterion" },
  { id: "test", label: "Test idea" },
  { id: "decision", label: "Decision" },
];

export const categoryLabel = (c: NextMoveCategory | null) =>
  CATEGORIES.find((x) => x.id === c)?.label ?? "";

/** Items of one kind in their saved order. */
export function itemsOfKind(items: readonly SessionItem[], kind: ItemKind): SessionItem[] {
  return items.filter((i) => i.kind === kind).sort((a, b) => a.position - b.position);
}

/** The "Copy checklist" text: one line per item, with a box showing whether it is done. */
export function checklistText(items: readonly SessionItem[]): string {
  return itemsOfKind(items, "checklist")
    .map((i) => `- [${i.done ? "x" : " "}] ${i.text}`)
    .join("\n");
}

/** "3 decisions, 2 checklist items, 1 Next Move item" for History; zero counts are left out. */
export function itemsSummary(items: readonly SessionItem[]): string {
  const n = (kind: ItemKind) => items.filter((i) => i.kind === kind).length;
  const part = (count: number, one: string, many: string) =>
    count === 0 ? null : `${count} ${count === 1 ? one : many}`;
  const parts = [
    part(n("decision"), "decision", "decisions"),
    part(n("checklist"), "checklist item", "checklist items"),
    part(n("next_move"), "Next Move item", "Next Move items"),
  ].filter((p): p is string => p !== null);
  return parts.length === 0 ? "No items yet." : `${parts.join(", ")}.`;
}

/** Local-time label for a timeline entry. */
export function localTime(iso: string): string {
  const t = Date.parse(iso);
  return Number.isNaN(t) ? "unknown time" : new Date(t).toLocaleString();
}
