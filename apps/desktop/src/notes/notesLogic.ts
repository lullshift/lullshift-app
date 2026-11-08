// Pure decisions for the Notes view (ADR-026 section 7): joining the overview with the live
// sessions, search, filters and counts. No React and no Tauri, so the wording is unit-tested.

import { sessionName } from "../format";
import type { NotesOverview, NotesSession, SessionKey } from "../context/api";
import { liveState, sessionStateView, type SessionView, type StateView } from "../sessions";

export type KindFilter = "all" | "open" | "done" | "notes" | "intents";
export type StateFilter = "all" | "needs" | "working" | "idle" | "ended";

export const KIND_FILTERS: { id: KindFilter; label: string }[] = [
  { id: "all", label: "All" },
  { id: "open", label: "Open items" },
  { id: "done", label: "Done items" },
  { id: "notes", label: "Notes" },
  { id: "intents", label: "Intents" },
];

export const STATE_FILTERS: { id: StateFilter; label: string }[] = [
  { id: "all", label: "Any state" },
  { id: "needs", label: "Needs you" },
  { id: "working", label: "Working" },
  { id: "idle", label: "Idle" },
  { id: "ended", label: "Ended" },
];

export interface NotesGroup {
  key: SessionKey;
  entry: NotesSession;
  /** The live session; null when the dashboard no longer lists it (state is then unknown). */
  session: SessionView | null;
  title: string;
  /** "Claude Code · abc12345", shown beside the title. */
  tool: string;
  state: StateView;
  stateFilter: StateFilter | "unknown";
}

/** Which state filter a session belongs to; untracked, imported or unlisted ones are "unknown". */
export function stateFilterOf(s: SessionView | null): StateFilter | "unknown" {
  if (!s) return "unknown";
  switch (liveState(s)) {
    case "needs_attention":
      return "needs";
    case "working":
      return "working";
    case "idle":
      return "idle";
    case "completed":
    case "failed":
    case "cancelled":
    case "ended":
      return "ended";
    default:
      return "unknown";
  }
}

const UNKNOWN_STATE: StateView = { label: "Unknown", tone: "neutral" };

/** Joins the overview with the dashboard's sessions. The overview's order (newest first) holds. */
export function buildGroups(
  overview: NotesOverview,
  sessions: readonly SessionView[],
): NotesGroup[] {
  const byKey = new Map(sessions.map((s) => [`${s.provider}:${s.providerSessionId}`, s]));
  return overview.sessions.map((entry) => {
    const session = byKey.get(`${entry.provider}:${entry.sessionId}`) ?? null;
    const name = sessionName({
      provider: entry.provider,
      providerSessionId: entry.sessionId,
      userTitle: entry.userTitle,
      agentTitle: entry.agentTitle,
      intentLine: session?.intentLine ?? null,
    });
    return {
      key: { provider: entry.provider, sessionId: entry.sessionId },
      entry,
      session,
      title: name.primary,
      tool: name.secondary ?? name.primary,
      state: session ? sessionStateView(session) : UNKNOWN_STATE,
      stateFilter: stateFilterOf(session),
    };
  });
}

export interface NotesQuery {
  text: string;
  kind: KindFilter;
  state: StateFilter;
}

/** What one group shows under the current filters. */
export interface VisibleGroup {
  group: NotesGroup;
  showIntent: boolean;
  showNotes: boolean;
  /** Ids of the checklist items shown. */
  itemIds: number[];
}

export const groupId = (g: NotesGroup) => `${g.key.provider}:${g.key.sessionId}`;

const fold = (s: string) => s.toLocaleLowerCase();

/**
 * Applies the state filter, the kind chip and the search. A search matches the intent, any item,
 * the notes, or the session's title, tool and project; matching the session itself keeps all of
 * its text visible, matching only some text shows just that text.
 */
export function filterGroups(
  groups: readonly NotesGroup[],
  q: NotesQuery,
  /** The text box being typed in stays visible even if the edit stops it matching the search. */
  keep: { key: string; field: "intent" | "notes" } | null = null,
): VisibleGroup[] {
  const needle = fold(q.text.trim());
  const out: VisibleGroup[] = [];
  for (const group of groups) {
    if (q.state !== "all" && group.stateFilter !== q.state) continue;
    const { entry } = group;
    const wantsIntent = (q.kind === "all" || q.kind === "intents") && !!entry.intent;
    const wantsNotes = (q.kind === "all" || q.kind === "notes") && entry.notes !== "";
    let items = entry.checklist.filter((i) =>
      q.kind === "all" ? true : q.kind === "open" ? !i.done : q.kind === "done" ? i.done : false,
    );
    let showIntent = wantsIntent;
    let showNotes = wantsNotes;
    if (needle) {
      const sessionHit = fold(
        [
          group.title,
          group.tool,
          entry.userTitle ?? "",
          entry.agentTitle ?? "",
          group.session?.intentLine ?? "",
        ].join(" "),
      ).includes(needle);
      if (!sessionHit) {
        showIntent = showIntent && fold(entry.intent ?? "").includes(needle);
        showNotes = showNotes && fold(entry.notes).includes(needle);
        items = items.filter((i) => fold(i.text).includes(needle));
      }
    }
    const kept = keep !== null && keep.key === groupId(group);
    if (kept && keep.field === "intent") showIntent = true;
    if (kept && keep.field === "notes") showNotes = true;
    if (!showIntent && !showNotes && items.length === 0) continue;
    out.push({ group, showIntent, showNotes, itemIds: items.map((i) => i.id) });
  }
  return out;
}

/** The heading line of the results: what is shown, in words. */
export function resultsLine(shown: number, total: number, filtered: boolean): string {
  if (total === 0) return "";
  if (!filtered) return `${total} ${total === 1 ? "session" : "sessions"} with notes`;
  return `${shown} of ${total} ${total === 1 ? "session" : "sessions"}`;
}
