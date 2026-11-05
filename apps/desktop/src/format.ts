// Display helpers: names, short IDs, relative times and the dashboard summary. Pure functions,
// so the wording is unit-tested.
import { liveState, type SessionState, type SessionView } from "./sessions";
import type { IconName } from "./ui/Icon";

/** Display name and generic glyph per agent tool (never the tool's own logo, ADR-011). */
const PROVIDERS: Record<string, { name: string; icon: IconName }> = {
  "claude-code": { name: "Claude Code", icon: "terminal" },
  codex: { name: "Codex", icon: "terminal" },
  cursor: { name: "Cursor", icon: "pointer" },
};

/** Human name for an agent provider; unknown providers show their own ID. */
export function providerName(provider: string): string {
  return PROVIDERS[provider]?.name ?? provider;
}

/** Glyph for an agent tool; unknown tools get a generic agent glyph. */
export function providerIcon(provider: string): IconName {
  return PROVIDERS[provider]?.icon ?? "agent";
}

/** First 8 characters of a session ID, enough to tell sessions apart. */
export function shortId(id: string): string {
  return id.length > 8 ? id.slice(0, 8) : id;
}

/** Titles from the intent are cut to this many characters. */
export const INTENT_TITLE_CHARS = 80;

export interface SessionName {
  /** What the card or row is called. */
  primary: string;
  /** Provider name and short ID, shown beside a real title; null when they are the name. */
  secondary: string | null;
  source: "user" | "agent" | "intent" | "tool";
}

const clean = (t: string | null | undefined) => (t ?? "").replace(/\s+/g, " ").trim();

/** Cuts to `max` characters in total, ending in an ellipsis when something was removed. */
export function ellipsize(text: string, max: number): string {
  const chars = [...text];
  return chars.length <= max
    ? text
    : `${chars
        .slice(0, max - 1)
        .join("")
        .trimEnd()}…`;
}

/**
 * What to call a session: the name the user gave it, else the agent's title, else the first
 * line of the captured intent, else the tool's name (with the short ID beside it, as before).
 */
export function sessionName(
  s: Pick<
    SessionView,
    "provider" | "providerSessionId" | "userTitle" | "agentTitle" | "intentLine"
  >,
): SessionName {
  const tool = `${providerName(s.provider)} · ${shortId(s.providerSessionId)}`;
  const user = clean(s.userTitle);
  if (user) return { primary: user, secondary: tool, source: "user" };
  const agent = clean(s.agentTitle);
  if (agent) return { primary: agent, secondary: tool, source: "agent" };
  const intent = clean(s.intentLine);
  if (intent) {
    return { primary: ellipsize(intent, INTENT_TITLE_CHARS), secondary: tool, source: "intent" };
  }
  return { primary: providerName(s.provider), secondary: null, source: "tool" };
}

/** The calm line after disconnecting a tool; empty when no session was affected. */
export function disconnectNotice(provider: string, count: number): string | null {
  if (count <= 0) return null;
  const who = providerName(provider);
  return count === 1
    ? `${who} disconnected. 1 session is no longer tracked.`
    : `${who} disconnected. ${count} sessions are no longer tracked.`;
}

/** "just now", "3 min ago", "2 hr ago", "yesterday", "4 days ago". Future times read "just now". */
export function relativeTime(iso: string, now: number): string {
  const then = Date.parse(iso);
  if (Number.isNaN(then)) return "unknown time";
  const s = Math.max(0, Math.round((now - then) / 1000));
  if (s < 45) return "just now";
  const min = Math.round(s / 60);
  if (min < 60) return `${min} min ago`;
  const hr = Math.round(min / 60);
  if (hr < 24) return `${hr} hr ago`;
  const days = Math.round(hr / 24);
  return days === 1 ? "yesterday" : `${days} days ago`;
}

export type SummaryTone = "attention" | "active" | "calm";

export interface Summary {
  tone: SummaryTone;
  title: string;
  detail: string;
}

const count = (sessions: readonly SessionView[], state: SessionState) =>
  sessions.filter((s) => liveState(s) === state).length;

/** The one-line answer to "do my agents need me?" shown at the top of the Sessions view. */
export function summarize(sessions: readonly SessionView[]): Summary {
  const needs = sessions.filter((s) => liveState(s) === "needs_attention");
  const working = count(sessions, "working");
  // Sessions that are no longer tracked are not "quiet", they are simply not counted.
  const unknown = sessions.filter((s) => s.reason !== "observed" && !s.disconnectedUtc).length;
  const unknownNote =
    unknown > 0
      ? ` ${unknown === 1 ? "One session hasn't" : `${unknown} sessions haven't`} reported recently.`
      : "";

  const [first] = needs;
  if (first) {
    return {
      tone: "attention",
      title:
        needs.length === 1
          ? `${providerName(first.provider)} needs you`
          : `${needs.length} sessions need you`,
      detail:
        working > 0
          ? `${working} more ${working === 1 ? "is" : "are"} still working.`
          : "Everything else is quiet.",
    };
  }
  if (working > 0) {
    return {
      tone: "active",
      title: working === 1 ? "1 agent working" : `${working} agents working`,
      detail: `Take a breath. Lullshift will tell you when one needs you.${unknownNote}`,
    };
  }
  return {
    tone: "calm",
    title: "All quiet",
    detail: `No agent is working right now.${unknownNote}`,
  };
}

export interface SessionGroups {
  needsYou: SessionView[];
  working: SessionView[];
  recent: SessionView[];
}

/** Splits already-sorted sessions into the three groups shown on the dashboard. */
export function groupSessions(sorted: readonly SessionView[]): SessionGroups {
  return {
    needsYou: sorted.filter((s) => liveState(s) === "needs_attention"),
    working: sorted.filter((s) => liveState(s) === "working"),
    recent: sorted.filter((s) => {
      const state = liveState(s);
      return state !== "needs_attention" && state !== "working";
    }),
  };
}

/** Tools offered as filter chips: every known tool, then any other tool seen in the sessions. */
export function toolChoices(sessions: readonly SessionView[]): string[] {
  const tools = Object.keys(PROVIDERS);
  for (const s of sessions) if (!tools.includes(s.provider)) tools.push(s.provider);
  return tools;
}

export interface SessionFilter {
  /** Free text matched against the title, the session ID and the tool name, ignoring case. */
  query: string;
  /** A provider ID, or "all". */
  tool: string;
}

/** Applies the search box and tool chips shared by the Live and History tabs. */
export function filterSessions(
  sessions: readonly SessionView[],
  { query, tool }: SessionFilter,
): SessionView[] {
  const q = query.trim().toLowerCase();
  return sessions.filter(
    (s) =>
      (tool === "all" || s.provider === tool) &&
      (q === "" ||
        s.providerSessionId.toLowerCase().includes(q) ||
        providerName(s.provider).toLowerCase().includes(q) ||
        sessionName(s).primary.toLowerCase().includes(q)),
  );
}

export type HistorySortKey = "tool" | "session" | "state" | "lastSeen";

export interface HistorySort {
  key: HistorySortKey;
  descending: boolean;
}

/** Sorts the History table; ties keep their incoming order. Unparseable times sort as oldest. */
export function sortHistory(
  sessions: readonly SessionView[],
  { key, descending }: HistorySort,
): SessionView[] {
  const value = (s: SessionView): string | number => {
    switch (key) {
      case "tool":
        return providerName(s.provider).toLowerCase();
      case "session": {
        // Named sessions sort by the name that is shown, the others by their ID.
        const name = sessionName(s);
        return (name.source === "tool" ? s.providerSessionId : name.primary).toLowerCase();
      }
      case "state":
        return liveState(s);
      case "lastSeen": {
        const t = Date.parse(s.lastReceivedUtc);
        return Number.isNaN(t) ? -Infinity : t;
      }
    }
  };
  const sign = descending ? -1 : 1;
  return sessions
    .map((s, i) => ({ s, i, v: value(s) }))
    .sort((a, b) => (a.v < b.v ? -sign : a.v > b.v ? sign : a.i - b.i))
    .map(({ s }) => s);
}
