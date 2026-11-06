// Companion commands (Sprint 09, ADR-020). This file mirrors docs/COMPANION_COMMANDS.md; change
// both together with the Rust commands.
//
// Privacy rules for this file (CLAUDE.md rule 7): a key goes through `setKey` once and is never
// stored, logged or put in an error. Errors are reduced to `{ kind, message }`, where `message`
// is the backend's fixed friendly sentence, and nothing the caller passed in is ever copied
// into an error.

import { Channel, invoke } from "@tauri-apps/api/core";

export type ErrorKind =
  | "offline"
  | "timeout"
  | "rateLimited"
  | "keyRejected"
  | "noKey"
  | "keychainUnavailable"
  | "budgetReached"
  | "providerError"
  | "invalidResponse"
  | "invalidInput"
  | "notFound"
  | "cancelled"
  | "busy"
  | "internal"
  /** UI only: the backend does not have this command (older or partial build). */
  | "unavailable";

export interface CommandError {
  kind: ErrorKind;
  message: string;
}

export const ERROR_KINDS: readonly ErrorKind[] = [
  "offline",
  "timeout",
  "rateLimited",
  "keyRejected",
  "noKey",
  "keychainUnavailable",
  "budgetReached",
  "providerError",
  "invalidResponse",
  "invalidInput",
  "notFound",
  "cancelled",
  "busy",
  "internal",
];

export type ProviderKind = "anthropic" | "openai" | "openaiCompatible";

export interface ProviderProfile {
  id: string;
  kind: ProviderKind;
  label: string;
  baseUrl: string;
  /** Shown as "Sends to <host>". */
  host: string;
  model: string;
  monthlyTokenBudget: number | null;
  isDefault: boolean;
  /** Asked of the keychain; false when the keychain can't be read. */
  hasKey: boolean;
  keychainOk: boolean;
}

export interface ProfileInput {
  id?: string;
  kind: ProviderKind;
  label: string;
  baseUrl?: string;
  model: string;
  monthlyTokenBudget?: number | null;
  isDefault: boolean;
}

export type Scope =
  | { kind: "none" }
  | { kind: "sessionSummary"; provider: string; sessionId: string }
  | {
      kind: "selectedNotes";
      provider: string;
      sessionId: string;
      notes: boolean;
      itemIds: number[];
    };

export type ScopeKind = Scope["kind"];

export interface ContextPreview {
  scope: Scope;
  /** Exactly what will be attached. */
  text: string;
  chars: number;
  /** Lowercase hex SHA-256 of `text`; pass it to `companionSend`. Null for scope none. */
  sha256: string | null;
}

export interface MessageError {
  kind: string;
  message: string;
}

export interface Message {
  id: number;
  threadId: number;
  role: "user" | "assistant";
  text: string;
  /** The text that was sent with it. */
  attachedContext: string | null;
  scopeKind: ScopeKind;
  /** The host it was sent to (your messages) or came from (replies). */
  host: string | null;
  status: "sent" | "failed" | "pending";
  error: MessageError | null;
  inputTokens: number | null;
  outputTokens: number | null;
  createdAt: string;
  /** Lookups made while answering a message of yours, oldest first. */
  toolCalls: ToolCallRecord[];
  /** Lookups the model asked for that Lullshift did not run, oldest first. */
  toolNotices: ToolNoticeRecord[];
}

export type ToolNoticeReason = "unknown" | "malformed" | "limit";

/** A lookup that was not run. Never carries the model's arguments. */
export interface ToolNoticeRecord {
  id: number;
  round: number;
  reason: ToolNoticeReason;
  /** The requested tool name, only if it was a short safe identifier. */
  tool: string | null;
  createdAt: string;
}

export type ToolName = "session_overview" | "suggest_break" | "activity_help";

/** A suggested activity. Starting it is always the user's click. */
export interface BreakCard {
  activityId: string;
  name: string;
  minutes: number;
}

export interface ToolCallRecord {
  id: number;
  round: number;
  tool: ToolName | string;
  label: string;
  arguments: string;
  /** Exactly what was sent to the provider. */
  resultText: string;
  status: "approved" | "declined";
  card: BreakCard | null;
  host: string | null;
  createdAt: string;
}

/** Events of one streaming call. Each carries the request id it belongs to. */
export type StreamEvent =
  | { type: "started"; requestId: string }
  | { type: "round"; requestId: string; round: number }
  | { type: "delta"; requestId: string; text: string }
  | {
      type: "toolRequest";
      requestId: string;
      approvalId: string;
      tool: ToolName | string;
      label: string;
      arguments: string;
      /** Exactly what will be sent if you approve. */
      resultText: string;
      host: string;
    }
  | { type: "toolSkipped"; requestId: string; tool: string; text: string }
  | { type: "toolResolved"; requestId: string; approvalId: string; approved: boolean }
  | {
      type: "toolNotice";
      requestId: string;
      reason: ToolNoticeReason;
      /** Only a short safe identifier; absent otherwise. */
      tool?: string | null;
    };

export interface CompanionPrefs {
  /** "Let the Companion look things up". Off by default. */
  toolsEnabled: boolean;
  /** The context choice of the last message sent. */
  lastScope: Scope | null;
}

export interface Thread {
  id: number;
  title: string;
  profileId: string | null;
  model: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface Usage {
  profileId: string;
  month: string;
  inputTokens: number;
  outputTokens: number;
  budget: number | null;
}

/** Turns whatever `invoke` rejected with into a `CommandError`. Never throws, never echoes input. */
export function toCommandError(err: unknown): CommandError {
  if (err && typeof err === "object" && "kind" in err) {
    const e = err as { kind?: unknown; message?: unknown };
    if (typeof e.kind === "string" && (ERROR_KINDS as readonly string[]).includes(e.kind)) {
      return {
        kind: e.kind as ErrorKind,
        message: typeof e.message === "string" ? e.message.slice(0, 300) : "",
      };
    }
  }
  const text = typeof err === "string" ? err : err instanceof Error ? err.message : "";
  // Tauri reports a missing or unpermitted command as a plain string. The text is only
  // pattern-matched here, never copied into the error.
  if (/not found|not allowed|unknown command|not registered/i.test(text)) {
    return { kind: "unavailable", message: "" };
  }
  return { kind: "internal", message: "" };
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (err) {
    throw toCommandError(err);
  }
}

export const companionProfiles = () => call<ProviderProfile[]>("companion_profiles");

export const companionSaveProfile = (profile: ProfileInput) =>
  call<ProviderProfile>("companion_save_profile", { profile });

export const companionDeleteProfile = (id: string) =>
  call<void>("companion_delete_profile", { id });

/**
 * Sends the key to the keychain, once. The value is passed straight through: this function keeps
 * no copy, and the rejection never contains it. Callers must clear their own copy right after
 * calling.
 */
export const companionSetKey = (profileId: string, key: string) =>
  call<void>("companion_set_key", { profileId, key });

export const companionClearKey = (profileId: string) =>
  call<void>("companion_clear_key", { profileId });

export const companionTest = (profileId: string) =>
  call<{ ok: true; model: string }>("companion_test", { profileId });

export const companionThreads = () => call<Thread[]>("companion_threads");

export const companionNewThread = (title?: string) =>
  call<Thread>("companion_new_thread", { title });

export const companionRenameThread = (id: number, title: string) =>
  call<void>("companion_rename_thread", { id, title });

export const companionDeleteThread = (id: number) => call<void>("companion_delete_thread", { id });

export const companionMessages = (threadId: number) =>
  call<Message[]>("companion_messages", { threadId });

export const companionPreviewContext = (scope: Scope) =>
  call<ContextPreview>("companion_preview_context", { scope });

/** A fresh id for one send or retry. Events for any other id are ignored. */
export function newRequestId(): string {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  return "r-" + Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

function eventChannel(onEvent: (e: StreamEvent) => void): Channel<StreamEvent> {
  const ch = new Channel<StreamEvent>();
  ch.onmessage = onEvent;
  return ch;
}

/**
 * Streams the reply: `onEvent` gets text and lookup requests while the call runs, and the
 * returned promise ends with the stored assistant message (or rejects).
 */
export const companionSend = (
  threadId: number,
  profileId: string,
  text: string,
  scope: Scope,
  expectedContextSha256: string | null,
  requestId: string,
  onEvent: (e: StreamEvent) => void,
) =>
  call<Message>("companion_send", {
    threadId,
    profileId,
    text,
    scope,
    expectedContextSha256,
    requestId,
    onEvent: eventChannel(onEvent),
  });

export const companionRetry = (
  messageId: number,
  requestId: string,
  onEvent: (e: StreamEvent) => void,
) => call<Message>("companion_retry", { messageId, requestId, onEvent: eventChannel(onEvent) });

/** Answers a lookup the model asked for. Nothing is sent to the provider before this. */
export const companionToolDecision = (requestId: string, approvalId: string, approve: boolean) =>
  call<void>("companion_tool_decision", { requestId, approvalId, approve });

export const companionPrefs = () => call<CompanionPrefs>("companion_prefs");

export const companionSetToolsEnabled = (enabled: boolean) =>
  call<CompanionPrefs>("companion_set_tools_enabled", { enabled });

export const companionCancel = (threadId: number) => call<void>("companion_cancel", { threadId });

export const companionUsage = () => call<Usage[]>("companion_usage");
