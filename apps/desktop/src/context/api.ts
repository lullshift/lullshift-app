// Context Keeper and Next Move commands (S04, ADR-013). This file is the contract between the
// UI and the Rust commands in `src-tauri/src/context.rs`; change both together.
//
// Every command is granted to the main window only. Errors are short codes (strings), never
// raw task text: "not_found", "invalid_input", "too_many", "store_unavailable", "internal".

import { invoke } from "@tauri-apps/api/core";

export type ItemKind = "decision" | "checklist" | "next_move";
/** Next Move items only; null for decisions and checklist items. */
export type NextMoveCategory = "question" | "acceptance" | "test" | "decision";
export type TextSource = "user" | "agent";
export type RetentionDays = 7 | 30 | 90 | 365;
export type ExportFormat = "markdown" | "json";

export interface SessionKey {
  provider: string;
  sessionId: string;
}

export interface SessionItem {
  id: number;
  kind: ItemKind;
  category: NextMoveCategory | null;
  /** At most 2,000 characters. */
  text: string;
  done: boolean;
  /** Order within the session and kind, from 0. */
  position: number;
  createdUtc: string;
}

export interface SessionContext {
  /** The user's own intent if set, otherwise the captured one; null if neither. */
  intent: { text: string; source: TextSource } | null;
  /** Captured text only (ADR-013); null when capture is off or nothing was captured. */
  latestRequest: { text: string; capturedUtc: string } | null;
  ask: { text: string; capturedUtc: string } | null;
  /** The user's notes; empty string when none. At most 20,000 characters. */
  notes: string;
  reviewedUtc: string | null;
  /** True if any captured (agent) text is stored for this session. */
  hasCaptured: boolean;
}

export interface TimelineEntry {
  timestampUtc: string;
  /** Agent event type (ADR-008), e.g. "needs_attention", "turn_ended". */
  eventType: string;
  /** Enum-like hint such as "permission" or "prompt_input_exit"; never task text. */
  reason: string | null;
}

export interface SessionDetail {
  context: SessionContext;
  items: SessionItem[];
  links: SessionKey[];
  /** Oldest first, at most 500 entries (the newest). */
  timeline: TimelineEntry[];
  /** How long events are kept, in days. */
  retentionDays: RetentionDays;
}

export interface ContextPrefs {
  retentionDays: RetentionDays;
  /** Opt-in capture per adapter id (ADR-013). Missing means off. */
  capture: Record<string, boolean>;
}

export interface UiPrefs {
  detailPanelPinned: boolean;
}

const k = (s: SessionKey) => ({ provider: s.provider, sessionId: s.sessionId });

export const sessionDetail = (s: SessionKey) => invoke<SessionDetail>("session_detail", k(s));

export const setSessionNotes = (s: SessionKey, notes: string) =>
  invoke<null>("set_session_notes", { ...k(s), notes });

/** An empty string removes the user's intent (the captured one, if any, shows again). */
export const setSessionIntent = (s: SessionKey, intent: string) =>
  invoke<null>("set_session_intent", { ...k(s), intent });

export const addSessionItem = (
  s: SessionKey,
  kind: ItemKind,
  category: NextMoveCategory | null,
  text: string,
) => invoke<SessionItem>("add_session_item", { ...k(s), kind, category, text });

export const updateSessionItem = (id: number, change: { text?: string; done?: boolean }) =>
  invoke<null>("update_session_item", { id, text: change.text ?? null, done: change.done ?? null });

export const deleteSessionItem = (id: number) => invoke<null>("delete_session_item", { id });

/** Moves an item to `position` within its session and kind; the others shift. */
export const moveSessionItem = (id: number, position: number) =>
  invoke<null>("move_session_item", { id, position });

export const linkSessions = (s: SessionKey, other: SessionKey) =>
  invoke<null>("link_sessions", {
    ...k(s),
    otherProvider: other.provider,
    otherSessionId: other.sessionId,
  });

export const unlinkSessions = (s: SessionKey, other: SessionKey) =>
  invoke<null>("unlink_sessions", {
    ...k(s),
    otherProvider: other.provider,
    otherSessionId: other.sessionId,
  });

export const markReviewed = (s: SessionKey) => invoke<null>("mark_reviewed", k(s));

export const clearCapturedContext = (s: SessionKey) => invoke<null>("clear_captured_context", k(s));

export const clearAllCapturedContext = () => invoke<null>("clear_all_captured_context");

/** Opens a native save dialog; resolves "cancelled" if the user closes it. */
export const exportSession = (s: SessionKey, format: ExportFormat) =>
  invoke<"saved" | "cancelled">("export_session", { ...k(s), format });

/** Deletes the session, its events, context, items and links. Can't be undone. */
export const deleteSession = (s: SessionKey) => invoke<null>("delete_session", k(s));

export const contextPrefs = () => invoke<ContextPrefs>("context_prefs");
export const setContextPrefs = (prefs: ContextPrefs) =>
  invoke<null>("set_context_prefs", { prefs });

export const uiPrefs = () => invoke<UiPrefs>("ui_prefs");
export const setUiPrefs = (prefs: UiPrefs) => invoke<null>("set_ui_prefs", { prefs });
