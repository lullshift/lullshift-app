// Pure Companion logic: wording, gating and plain-text helpers. No React, no invoke.
// The Rust core is authoritative; this only labels and gates what the UI shows.

import {
  ERROR_KINDS,
  type CatalogModel,
  type CatalogProvider,
  type ErrorKind,
  type ModelCatalog,
  type ProviderKind,
  type Scope,
  type ScopeKind,
  type StreamEvent,
  type ToolCallRecord,
  type ToolNoticeReason,
  type Usage,
} from "./api";

export const UNAVAILABLE_TEXT = "Companion isn't available in this build.";

/** Seconds the provider asked us to wait, read from the backend's friendly sentence. */
export function retryAfterSeconds(message: string): number | null {
  const m = /(\d{1,6})\s*(seconds?|secs?|minutes?|mins?|s\b|m\b)/i.exec(message);
  if (!m) return null;
  const n = Number(m[1]);
  if (!Number.isFinite(n) || n <= 0) return null;
  return /^m/i.test(m[2] ?? "") ? n * 60 : n;
}

export function waitText(seconds: number): string {
  if (seconds < 90) return `${seconds} second${seconds === 1 ? "" : "s"}`;
  return `${Math.round(seconds / 60)} minutes`;
}

export interface ErrorLike {
  kind: string;
  message?: string;
}

function capitalise(s: string): string {
  return s.charAt(0).toUpperCase() + s.slice(1);
}

/**
 * Honest, fixed wording for every error kind. The text is built from the kind (and the host,
 * which comes from the user's own profile), never from anything the user typed. The backend
 * message is only read for a wait time and is never shown as is.
 */
export function errorText(err: ErrorLike, host?: string | null): string {
  const where = host?.trim() ? host.trim() : "the provider";
  const known = (ERROR_KINDS as readonly string[]).includes(err.kind) || err.kind === "unavailable";
  switch (known ? (err.kind as ErrorKind) : "internal") {
    case "offline":
      return `Can't reach ${where}. Check your connection.`;
    case "timeout":
      return `${capitalise(where)} took too long to answer. Nothing was lost; try again.`;
    case "rateLimited": {
      const s = retryAfterSeconds(err.message ?? "");
      return s
        ? `${capitalise(where)} is limiting requests. Wait about ${waitText(s)}, then try again.`
        : `${capitalise(where)} is limiting requests and didn't say how long to wait. Try again in a little while.`;
    }
    case "keyRejected":
      return `${capitalise(where)} rejected the key. Check that it is correct, still active and allowed to use this model, then save it again.`;
    case "noKey":
      return "No key is set for this provider yet. Add one in Companion settings.";
    case "keychainUnavailable":
      return "The system keychain isn't available, so Companion can't read or save a key. It won't store keys anywhere else.";
    case "budgetReached":
      return "This month's token budget for this provider is used up. Raise the budget in Companion settings, or wait for next month.";
    case "providerError":
      return `${capitalise(where)} had a problem and couldn't answer. Try again in a moment.`;
    case "invalidResponse":
      return `${capitalise(where)} sent a reply Lullshift couldn't read, so it was discarded. Check the model name and base URL.`;
    case "invalidInput":
      return "That value isn't valid. Check it and try again.";
    case "notFound":
      return "That item isn't there any more. Nothing was changed.";
    case "cancelled":
      return "Cancelled. The request may already have reached the provider, so it may still count against your usage.";
    case "busy":
      return "Another request is still running. Wait for it or cancel it first.";
    case "unavailable":
      return UNAVAILABLE_TEXT;
    case "internal":
    default:
      return "Something went wrong inside Lullshift. Nothing was sent or changed.";
  }
}

/** Errors the user fixes in Companion settings (the UI links there). */
export function needsSettings(kind: string): boolean {
  return kind === "noKey" || kind === "keyRejected" || kind === "budgetReached";
}

// ---------- Scopes ----------

export const SCOPE_KINDS: readonly ScopeKind[] = ["none", "sessionSummary", "selectedNotes"];

export function scopeLabel(kind: ScopeKind): string {
  switch (kind) {
    case "none":
      return "No context";
    case "sessionSummary":
      return "Session summary";
    case "selectedNotes":
      return "Selected notes";
  }
}

export function scopeHelp(kind: ScopeKind): string {
  switch (kind) {
    case "none":
      return "Only the words you type are sent.";
    case "sessionSummary":
      return "Adds one session's title, status, intent and latest ask.";
    case "selectedNotes":
      return "Adds only the notes and checklist items you tick from one session.";
  }
}

/** Summary line for the collapsible block under a user message. */
export function sharedLabel(kind: ScopeKind, host: string | null): string {
  const to = host?.trim() || "the provider";
  return kind === "none" ? "Nothing shared" : `Shared with ${to}: ${scopeLabel(kind)}`;
}

export interface ScopeDraft {
  kind: ScopeKind;
  provider: string;
  sessionId: string;
  notes: boolean;
  itemIds: readonly number[];
}

export const EMPTY_SCOPE: ScopeDraft = {
  kind: "none",
  provider: "",
  sessionId: "",
  notes: false,
  itemIds: [],
};

/** The scope to send, or null while the draft is incomplete (no session, or nothing ticked). */
export function buildScope(d: ScopeDraft): Scope | null {
  if (d.kind === "none") return { kind: "none" };
  if (!d.provider || !d.sessionId) return null;
  if (d.kind === "sessionSummary") {
    return { kind: "sessionSummary", provider: d.provider, sessionId: d.sessionId };
  }
  if (!d.notes && d.itemIds.length === 0) return null;
  return {
    kind: "selectedNotes",
    provider: d.provider,
    sessionId: d.sessionId,
    notes: d.notes,
    itemIds: [...d.itemIds].sort((a, b) => a - b),
  };
}

/** A stable key for comparing scopes. */
export function scopeKey(s: Scope): string {
  switch (s.kind) {
    case "none":
      return "none";
    case "sessionSummary":
      return `summary|${s.provider}|${s.sessionId}`;
    case "selectedNotes": {
      const ids = [...s.itemIds].sort((a, b) => a - b).join(",");
      return `notes|${s.provider}|${s.sessionId}|${s.notes ? 1 : 0}|${ids}`;
    }
  }
}

/** What the user has confirmed: the scope the preview was built for, and the exact text seen. */
export interface ConfirmedPreview {
  scope: Scope;
  text: string;
  /** Hash of `text` from the backend preview, passed back on send. Null for scope none. */
  sha256: string | null;
}

export interface SendGate {
  text: string;
  scope: Scope | null;
  confirmed: ConfirmedPreview | null;
  hasProfile: boolean;
  sending: boolean;
}

/**
 * Whether Send may go ahead. Any scope other than none needs a preview the user confirmed for
 * exactly this scope; changing the scope afterwards invalidates it.
 */
export function canSend(g: SendGate): boolean {
  if (g.sending || !g.hasProfile || !g.scope) return false;
  if (g.text.trim().length === 0) return false;
  if (g.scope.kind === "none") return true;
  return g.confirmed !== null && scopeKey(g.confirmed.scope) === scopeKey(g.scope);
}

/** True when pressing Send should open the preview first. */
export function needsPreview(scope: Scope | null, confirmed: ConfirmedPreview | null): boolean {
  if (!scope || scope.kind === "none") return false;
  return !(confirmed && scopeKey(confirmed.scope) === scopeKey(scope));
}

export function charCountText(chars: number): string {
  return `${chars.toLocaleString("en-US")} character${chars === 1 ? "" : "s"}`;
}

// ---------- Plain text ----------

/**
 * Splits text into display lines, keeping blank lines. No link detection and no markup of any
 * kind: the UI renders each line as a React text node, so URLs stay text.
 */
export function plainLines(text: string): string[] {
  return text.replace(/\r\n?/g, "\n").split("\n");
}

// ---------- Budget ----------

export type BudgetState = "none" | "ok" | "near" | "reached";

export interface BudgetView {
  used: number;
  budget: number | null;
  state: BudgetState;
  /** 0 to 1, or null with no budget. */
  fraction: number | null;
  line: string;
}

export function formatTokens(n: number): string {
  if (!Number.isFinite(n) || n < 0) return "unknown";
  return Math.round(n).toLocaleString("en-US");
}

export function budgetView(
  usage: Pick<Usage, "inputTokens" | "outputTokens"> | null,
  budget: number | null,
): BudgetView {
  const used = usage ? usage.inputTokens + usage.outputTokens : 0;
  if (budget === null || budget <= 0) {
    return {
      used,
      budget: null,
      state: "none",
      fraction: null,
      line: `${formatTokens(used)} tokens used this month. No budget set.`,
    };
  }
  const fraction = Math.min(1, used / budget);
  const state: BudgetState = used >= budget ? "reached" : fraction >= 0.8 ? "near" : "ok";
  const tail =
    state === "reached"
      ? " Budget reached: sending is blocked."
      : state === "near"
        ? " Close to the budget."
        : "";
  return {
    used,
    budget,
    state,
    fraction,
    line: `${formatTokens(used)} of ${formatTokens(budget)} tokens used this month.${tail}`,
  };
}

/** Parses the budget field: empty means no budget; "invalid" means reject. */
export function parseBudget(text: string): number | null | "invalid" {
  const t = text.trim().replace(/[,_\s]/g, "");
  if (t === "") return null;
  if (!/^\d{1,12}$/.test(t)) return "invalid";
  const n = Number(t);
  return n > 0 ? n : "invalid";
}

// ---------- Providers ----------

export function providerKindLabel(kind: ProviderKind): string {
  switch (kind) {
    case "anthropic":
      return "Anthropic";
    case "openai":
      return "OpenAI";
    case "openaiCompatible":
      return "OpenAI-compatible";
  }
}

export const MODEL_SUGGESTIONS: Record<ProviderKind, readonly string[]> = {
  anthropic: ["claude-sonnet-5-5", "claude-opus-5-5", "claude-haiku-5-5"],
  openai: ["gpt-4o", "gpt-4o-mini"],
  openaiCompatible: [],
};

export const MODEL_HINT =
  "The model name must match the provider's own name for it, exactly. Suggestions may be out of date; Lullshift can't check the name until you send or test.";

export function baseUrlHint(kind: ProviderKind): string {
  return kind === "openaiCompatible"
    ? "Include the version part, for example https://openrouter.ai/api/v1 or http://localhost:11434/v1. Use https://; http:// is accepted only for a server on this computer (localhost, 127.0.0.1 or ::1)."
    : "Fixed for this provider.";
}

const LOOPBACK = new Set(["localhost", "127.0.0.1", "[::1]", "::1"]);

/**
 * A quick check for the base URL field, to catch mistakes before saving. The backend validates
 * again and is authoritative. Returns a problem sentence, or null if it looks fine.
 */
export function baseUrlProblem(raw: string): string | null {
  const text = raw.trim();
  if (!text) return "Enter the server's base URL.";
  let u: URL;
  try {
    u = new URL(text);
  } catch {
    return "That doesn't look like a URL. Start with https://.";
  }
  if (u.username || u.password) return "Don't put a name or key in the URL.";
  if (u.search || u.hash) return "Leave out anything after a ? or #.";
  if (u.protocol === "https:") return null;
  if (u.protocol === "http:") {
    return LOOPBACK.has(u.hostname)
      ? null
      : "http:// is only allowed for a server on this computer. Use https://.";
  }
  return "Use https://.";
}

/** The "Sends to <host>" line. */
export function sendsToText(host: string | null | undefined): string {
  return host?.trim() ? `Sends to ${host.trim()}` : "No provider set up";
}

export const KEY_SAVED_TEXT = "Key saved in your system keychain";
export const KEY_MISSING_TEXT = "No key set";
export const KEYCHAIN_MISSING_TEXT =
  "The system keychain isn't available on this computer. Companion won't store keys anywhere else, so it can't be used until the keychain works.";

export function keyStatusText(p: { hasKey: boolean; keychainOk: boolean }): string {
  if (!p.keychainOk) return "Keychain unavailable";
  return p.hasKey ? KEY_SAVED_TEXT : KEY_MISSING_TEXT;
}

/** Checks the shape of the key field. The returned sentence never contains the key. */
export function keyInputProblem(key: string): string | null {
  const t = key.trim();
  if (t.length === 0) return "Paste your key first.";
  if (t.length < 8 || t.length > 512) return "Keys are 8 to 512 characters long.";
  if (/[^\x20-\x7e]/.test(t)) return "Keys use plain letters, numbers and symbols only.";
  return null;
}

// ---------- Context drift ----------

/** Shown when the backend refuses a send because the attached text changed after the preview. */
export const CONTEXT_CHANGED_NOTE = "The context changed since you previewed it. Review it again.";

/** The backend's `invalidInput` for a stale preview hash (docs/COMPANION_COMMANDS.md). */
export function isContextChanged(err: ErrorLike): boolean {
  return err.kind === "invalidInput" && (err.message ?? "").includes("context changed");
}

// ---------- History ----------

// Mirrors crates/companion/src/scope.rs (MAX_HISTORY_MESSAGES, MAX_HISTORY_CHARS).
export const MAX_HISTORY_MESSAGES = 20;
export const MAX_HISTORY_CHARS = 60_000;

/**
 * How many earlier messages the next send repeats to the provider: delivered messages that went
 * to (or came from) this same host, newest 20 within 60,000 characters. Messages sent to another
 * host are never replayed, so they are not counted. The backend decides what is really sent; this
 * is the same rule, for the line under the composer.
 */
export function historyCount(
  messages: readonly { status: string; host: string | null; text: string }[],
  host: string | null | undefined,
): number {
  const h = host?.trim();
  if (!h) return 0;
  let total = 0;
  let count = 0;
  for (let i = messages.length - 1; i >= 0 && count < MAX_HISTORY_MESSAGES; i--) {
    const m = messages[i];
    if (!m || m.status !== "sent" || m.host !== h) continue;
    total += [...m.text].length;
    if (total > MAX_HISTORY_CHARS) break;
    count++;
  }
  return count;
}

export function historyText(count: number, host: string | null | undefined): string {
  const to = host?.trim() || "the provider";
  if (count === 0) return "No earlier messages are sent with this one.";
  return `Also sends ${count} earlier message${count === 1 ? "" : "s"} from this conversation to ${to}.`;
}

// ---------- Mounting ----------

/**
 * The Companion view reads the keychain when it mounts (key status per profile), so it is only
 * mounted once the user has opened it, and stays mounted after that so a reply in flight is not
 * lost when they switch views. Nothing touches the keychain at launch.
 */
export function shouldMountCompanion(view: string, openedBefore: boolean): boolean {
  return openedBefore || view === "companion";
}

// ---------- Streaming ----------

export interface PendingTool {
  approvalId: string;
  tool: string;
  label: string;
  arguments: string;
  /** Exactly what will be sent to the provider if the user approves. */
  resultText: string;
  host: string;
}

/** What the screen shows while one request runs. Built only from events of that request. */
export interface StreamState {
  requestId: string;
  text: string;
  round: number;
  /** A lookup waiting for Approve or Decline. */
  pending: PendingTool | null;
  /** Lookups that got a fixed sentence and no data (not offered, or over the limit). */
  skipped: string[];
  /** Lookups answered during this request. */
  answered: { label: string; approved: boolean }[];
  /** Lookups that were not run (unknown, malformed or over the limit): a notice, no button. */
  notices: { reason: ToolNoticeReason; tool: string | null }[];
}

export function initialStream(requestId: string): StreamState {
  return { requestId, text: "", round: 1, pending: null, skipped: [], answered: [], notices: [] };
}

/**
 * Applies one stream event. Events for any other request id are ignored, which is what keeps a
 * late chunk from a cancelled request out of a newer one in the same conversation.
 */
export function reduceStream(s: StreamState, e: StreamEvent): StreamState {
  if (e.requestId !== s.requestId) return s;
  switch (e.type) {
    case "started":
      return s;
    case "round":
      // The next provider call starts a fresh text; what came before belonged to the call before.
      return { ...s, round: e.round, text: "", pending: null };
    case "delta":
      return { ...s, text: s.text + e.text };
    case "toolRequest":
      return {
        ...s,
        pending: {
          approvalId: e.approvalId,
          tool: e.tool,
          label: e.label,
          arguments: e.arguments,
          resultText: e.resultText,
          host: e.host,
        },
      };
    case "toolSkipped":
      return { ...s, skipped: [...s.skipped, e.text] };
    case "toolNotice":
      return {
        ...s,
        notices: [...s.notices, { reason: e.reason, tool: safeToolName(e.tool) }],
      };
    case "toolResolved": {
      // Only the lookup that is waiting can be resolved.
      if (s.pending?.approvalId !== e.approvalId) return s;
      return {
        ...s,
        pending: null,
        answered: [...s.answered, { label: s.pending.label, approved: e.approved }],
      };
    }
  }
}

/**
 * The one short sentence for the screen-reader live region. It changes only when the phase
 * changes (waiting, replying, waiting for you), never per token, so a reply is not read out as
 * it arrives. The finished reply is announced once, by the conversation.
 */
export function streamStatus(s: StreamState | null, host: string | null): string {
  if (!s) return "";
  const where = host?.trim() || "the provider";
  if (s.pending) {
    return `${where} asked to look something up: ${s.pending.label}. It waits for your decision.`;
  }
  if (s.text.length === 0 && s.notices.length > 0) {
    // One sentence, changing only when a notice is added; the notice text itself is in the flow.
    return `${where} asked for something it can't have. Nothing was sent.`;
  }
  return s.text.length === 0 ? `Waiting for ${where}…` : "Companion is replying…";
}

// ---------- Remembered context ----------

/** The composer's draft for a scope that was chosen before. */
export function draftFromScope(scope: Scope | null): ScopeDraft {
  if (!scope || scope.kind === "none") return EMPTY_SCOPE;
  if (scope.kind === "sessionSummary") {
    return {
      ...EMPTY_SCOPE,
      kind: "sessionSummary",
      provider: scope.provider,
      sessionId: scope.sessionId,
    };
  }
  return {
    kind: "selectedNotes",
    provider: scope.provider,
    sessionId: scope.sessionId,
    notes: scope.notes,
    itemIds: [...scope.itemIds],
  };
}

export interface RestoredScope {
  draft: ScopeDraft;
  /** Said under the composer when the last choice could not be used. */
  note: string | null;
}

/**
 * What the composer starts with. The last choice is used only if it can still be previewed;
 * otherwise the composer falls back to No context and says so. Either way the preview is shown
 * and confirmed before every send.
 */
export function restoreScope(remembered: Scope | null, available: boolean): RestoredScope {
  if (!remembered || remembered.kind === "none") return { draft: EMPTY_SCOPE, note: null };
  if (!available) {
    return {
      draft: EMPTY_SCOPE,
      note: `The context you used last time (${scopeLabel(remembered.kind).toLowerCase()}) isn't available any more, so this message starts with No context.`,
    };
  }
  return { draft: draftFromScope(remembered), note: null };
}

export function rememberedText(kind: ScopeKind): string {
  return `Started with your last choice: ${scopeLabel(kind).toLowerCase()}. You will still see exactly what is sent before it goes.`;
}

// ---------- Lookups (tools) ----------

export const TOOLS_SETTING_LABEL = "Let the Companion look things up";
export const TOOLS_SETTING_HELP =
  "Off by default. When on, the provider may ask for three read-only lookups: a summary of your agent sessions (tool, state and time in state; a session label only for tools where you allowed task text: your name for it, the agent's title or the first line of the captured prompt), a break suggestion, and an installed activity's help. You see the exact result and choose Approve or Decline for every single lookup, and nothing is sent until you do. Lookups never change anything and never start anything. They add a little to each request, so they count toward your token budget. Some servers don't support them; turn this off if replies fail.";

/** A tool name is shown only if it is a short safe identifier; anything else is dropped. */
export function safeToolName(name: string | null | undefined): string | null {
  return typeof name === "string" && /^[a-z][a-z0-9_-]{0,31}$/.test(name) ? name : null;
}

/**
 * The plain-text notice for a lookup that was not run (owner decision 27.6). Fixed wording per
 * reason; the only variable part is a safe tool name, never anything the model wrote.
 */
export function toolNoticeText(reason: ToolNoticeReason, tool: string | null | undefined): string {
  const name = safeToolName(tool);
  const what = name ? ` (${name})` : "";
  switch (reason) {
    case "unknown":
      return `The Companion asked for something it can't have (an unknown lookup${what}). Nothing was sent.`;
    case "malformed":
      return `The Companion asked for something it can't have (a lookup${what} that was not valid). Nothing was sent.`;
    case "limit":
      return `The Companion asked for something it can't have (more lookups${what} than one message allows). Nothing was sent.`;
  }
}

export function toolDecisionNote(rec: Pick<ToolCallRecord, "status" | "host">): string {
  const to = rec.host?.trim() || "the provider";
  return rec.status === "approved"
    ? `Approved. This exact text was sent to ${to}.`
    : `Declined. Only the words "The user declined." were sent to ${to}.`;
}

export function approvalPrompt(p: Pick<PendingTool, "label" | "host">): string {
  return `${p.host} asked for a lookup: ${p.label}. This exact text will be sent to it if you approve.`;
}

// ---- the models.dev model list (decision 32) ----------------------------------------------

export const CATALOG_LOAD_LABEL = "Load model list from models.dev";
export const CATALOG_SENDS_TEXT =
  "Sends one request to models.dev. Your keys and messages are not sent.";
export const CATALOG_DISCLAIMER =
  "models.dev is a community list and may be out of date. Being listed doesn't mean your key can use a model: Test connection checks.";
/** Most model rows drawn at once; the search narrows the rest. */
export const CATALOG_MAX_ROWS = 200;

/** The models.dev provider shown for a fixed provider kind; compatible servers pick one. */
export const CATALOG_PROVIDER: Record<ProviderKind, string | null> = {
  anthropic: "anthropic",
  openai: "openai",
  openaiCompatible: null,
};

export type CatalogSort = "newest" | "cheapest" | "name";

export const CATALOG_SORTS: readonly { value: CatalogSort; label: string }[] = [
  { value: "newest", label: "Newest first" },
  { value: "cheapest", label: "Cheapest input first" },
  { value: "name", label: "Name" },
];

export interface CatalogFilters {
  text: string;
  tools: boolean;
  reasoning: boolean;
  hideOld: boolean;
  sort: CatalogSort;
}

export const DEFAULT_CATALOG_FILTERS: CatalogFilters = {
  text: "",
  tools: false,
  reasoning: false,
  hideOld: false,
  sort: "newest",
};

/** Milliseconds (UTC) of a "YYYY", "YYYY-MM" or "YYYY-MM-DD" date; null if it isn't one. */
export function releaseTime(date: string | null | undefined): number | null {
  const m = /^(\d{4})(?:-(\d{2})(?:-(\d{2}))?)?$/.exec(date ?? "");
  if (!m) return null;
  const month = m[2] ? Number(m[2]) : 1;
  const day = m[3] ? Number(m[3]) : 1;
  if (month < 1 || month > 12 || day < 1 || day > 31) return null;
  return Date.UTC(Number(m[1]), month - 1, day);
}

/**
 * True when the model was released more than a year before `now`. A model without a known
 * date is never hidden: its age can't be known.
 */
export function isOlderThanAYear(date: string | null | undefined, now: Date): boolean {
  const t = releaseTime(date);
  if (t === null) return false;
  const cutoff = Date.UTC(now.getUTCFullYear() - 1, now.getUTCMonth(), now.getUTCDate());
  return t < cutoff;
}

function byName(a: CatalogModel, b: CatalogModel): number {
  return a.name.localeCompare(b.name) || a.id.localeCompare(b.id);
}

/** Missing values sort last in either direction. */
function nullsLast(a: number | null, b: number | null, dir: 1 | -1): number {
  if (a === null && b === null) return 0;
  if (a === null) return 1;
  if (b === null) return -1;
  return (a - b) * dir;
}

/** The models that pass the filters, in the chosen order. Works on memory only. */
export function filterModels(
  models: readonly CatalogModel[],
  f: CatalogFilters,
  now: Date,
): CatalogModel[] {
  const words = f.text.toLowerCase().split(/\s+/).filter(Boolean);
  const kept = models.filter((m) => {
    if (f.tools && !m.toolCall) return false;
    if (f.reasoning && !m.reasoning) return false;
    if (f.hideOld && isOlderThanAYear(m.releaseDate, now)) return false;
    if (words.length === 0) return true;
    const hay = `${m.name} ${m.id} ${m.family ?? ""}`.toLowerCase();
    return words.every((w) => hay.includes(w));
  });
  const order = (a: CatalogModel, b: CatalogModel): number => {
    switch (f.sort) {
      case "newest":
        return (
          nullsLast(releaseTime(a.releaseDate), releaseTime(b.releaseDate), -1) || byName(a, b)
        );
      case "cheapest":
        return nullsLast(a.costInput, b.costInput, 1) || byName(a, b);
      case "name":
        return byName(a, b);
    }
  };
  return kept.sort(order);
}

/** "$3", "$0.15", "$0.075", "$0". */
export function formatUsd(n: number): string {
  if (n === 0) return "$0";
  if (n >= 1) return `$${Number(n.toFixed(2))}`;
  return `$${Number(n.toPrecision(2))}`;
}

/** Input and output price per million tokens, or "price unknown". */
export function costText(m: Pick<CatalogModel, "costInput" | "costOutput">): string {
  if (m.costInput === null && m.costOutput === null) return "price unknown";
  const input = m.costInput === null ? "input price unknown" : `${formatUsd(m.costInput)} in`;
  const output = m.costOutput === null ? "output price unknown" : `${formatUsd(m.costOutput)} out`;
  return `${input} / ${output} per 1M tokens`;
}

/** "200K context", "1M context", "context size unknown". */
export function contextText(tokens: number | null): string {
  if (tokens === null) return "context size unknown";
  if (tokens >= 1_000_000) return `${Number((tokens / 1_000_000).toFixed(1))}M context`;
  if (tokens >= 1_000) return `${Math.round(tokens / 1_000)}K context`;
  return `${tokens} tokens context`;
}

/** One line of facts for a model row. */
export function modelFacts(m: CatalogModel): string {
  const parts = [contextText(m.contextTokens), costText(m)];
  if (m.toolCall) parts.push("supports tools");
  if (m.reasoning) parts.push("reasoning");
  parts.push(m.releaseDate ? `released ${m.releaseDate}` : "release date unknown");
  return parts.join(" · ");
}

/** "1 model", "3 models". */
export function countOf(n: number, noun: string): string {
  return `${n} ${noun}${n === 1 ? "" : "s"}`;
}

export function catalogCountText(shown: number, total: number): string {
  if (total === 0) return "This provider has no chat models in the list.";
  if (shown === 0) return `No models match these filters (out of ${total}).`;
  if (shown === total) return `${total} model${total === 1 ? "" : "s"}`;
  return `${shown} of ${total} models`;
}

function hhmm(iso: string): string | null {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return null;
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

/** "Loaded at 10:42", or "Checked at 10:45: no changes" after a refresh that found none. */
export function loadedAtText(c: Pick<ModelCatalog, "fetchedAt" | "unchanged">): string {
  const at = hhmm(c.fetchedAt);
  if (!at) return "Loaded in this app run";
  return c.unchanged ? `Checked at ${at}: no changes` : `Loaded at ${at}`;
}

/** Providers whose name or id contains the query, by name. */
export function filterProviders(
  providers: readonly CatalogProvider[],
  query: string,
): CatalogProvider[] {
  const q = query.trim().toLowerCase();
  return providers
    .filter((p) => !q || p.name.toLowerCase().includes(q) || p.id.toLowerCase().includes(q))
    .sort((a, b) => a.name.localeCompare(b.name) || a.id.localeCompare(b.id));
}

/**
 * The backend's catalog errors are fixed sentences that already name models.dev; this keeps
 * them and words the cases that have no sentence.
 */
export function catalogErrorText(err: ErrorLike): string {
  if (err.kind === "unavailable") return "The model list isn't available in this build.";
  const message = err.message?.trim() ?? "";
  if (err.kind === "internal" || !message) {
    return "Something went wrong inside Lullshift while reading the model list. Try again.";
  }
  return message;
}
