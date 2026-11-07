// Import sessions (owner decision 34, ADR-024): the UI side of `import_scan`,
// `import_sessions` and `import_cancel` (apps/desktop/src-tauri/src/session_import.rs; change
// both together).
// Pure logic only, so it is unit-tested without a DOM.

import { invoke } from "@tauri-apps/api/core";
import { providerName, relativeTime, shortId, stripInvisible } from "./format";

export type Guess = "working" | "waiting" | "may_need_you" | "unknown";
export type LogsState = "read" | "not_found" | "unreadable" | "not_local";

export interface FoundSession {
  provider: string;
  sessionId: string;
  /** Last two parts of the project folder. */
  project: string | null;
  /** The whole folder as the log states it; tooltip only. */
  projectPath: string | null;
  title: string | null;
  lastActivityUtc: string;
  guess: Guess;
  /** Lullshift already has this session. */
  tracked: boolean;
}

export interface AgentLogs {
  provider: string;
  state: LogsState;
  sessions: number;
  /** A cap or the deadline cut this agent's list short. */
  incomplete: boolean;
}

export interface ScanResult {
  sessions: FoundSession[];
  agents: AgentLogs[];
  incomplete: boolean;
}

export interface ImportResult {
  imported: number;
  alreadyTracked: number;
  notFound: number;
}

export const scanLogs = () => invoke<ScanResult>("import_scan");
/** Stops a running scan (Cancel or close while reading). Harmless when nothing runs. */
export const cancelScan = () => invoke<void>("import_cancel");
export const importPicked = (picks: { provider: string; sessionId: string }[]) =>
  invoke<ImportResult>("import_sessions", { picks });

export const GUESS_LABELS: Record<Guess, string> = {
  working: "Working",
  waiting: "Waiting for you",
  may_need_you: "May need you",
  unknown: "Unknown",
};

export const keyOf = (s: Pick<FoundSession, "provider" | "sessionId">) =>
  `${s.provider}\u0000${s.sessionId}`;

/** Newest activity first; ties by agent then id, so the order never jumps. */
export function sortFound(list: readonly FoundSession[]): FoundSession[] {
  return [...list].sort(
    (a, b) =>
      Date.parse(b.lastActivityUtc) - Date.parse(a.lastActivityUtc) ||
      a.provider.localeCompare(b.provider) ||
      a.sessionId.localeCompare(b.sessionId),
  );
}

/**
 * Free-text filter over agent name, project folder (short and full), title and short session
 * id. Every word must match somewhere; case-insensitive.
 */
export function filterFound(list: readonly FoundSession[], query: string): FoundSession[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (words.length === 0) return [...list];
  return list.filter((s) => {
    const hay = [
      providerName(s.provider),
      s.provider,
      s.project ?? "",
      s.projectPath ?? "",
      s.title ?? "",
      shortId(s.sessionId),
    ]
      .join("\n")
      .toLowerCase();
    return words.every((w) => hay.includes(w));
  });
}

/** The sessions of one project folder. */
export interface FolderGroup {
  /** Stable key: the whole folder as the log states it, else the short name, else empty. */
  key: string;
  /** What the heading shows: the short folder, or "Unknown folder". */
  label: string;
  /** The whole folder; tooltip only. */
  path: string | null;
  /** Newest first. */
  sessions: FoundSession[];
  /** Newest activity in the group. */
  latest: number;
}

export const UNKNOWN_FOLDER = "Unknown folder";

/**
 * Groups sessions by project folder. Groups are ordered by their most recent session (ties by
 * folder name, so the order never jumps); sessions inside a group are newest first.
 */
export function groupByFolder(list: readonly FoundSession[]): FolderGroup[] {
  const groups = new Map<string, FolderGroup>();
  for (const s of sortFound(list)) {
    const key = s.projectPath ?? s.project ?? "";
    let g = groups.get(key);
    if (!g) {
      g = {
        key,
        label: s.project ?? s.projectPath ?? UNKNOWN_FOLDER,
        path: s.projectPath,
        sessions: [],
        latest: Date.parse(s.lastActivityUtc),
      };
      groups.set(key, g);
    }
    g.sessions.push(s);
  }
  return [...groups.values()].sort(
    (a, b) => b.latest - a.latest || a.label.localeCompare(b.label) || a.key.localeCompare(b.key),
  );
}

export type GroupTick = "none" | "some" | "all";

/** Tri-state of a group's checkbox over the sessions that can be ticked; "none" if none can. */
export function groupTick(group: FolderGroup, selected: ReadonlySet<string>): GroupTick {
  const keys = pickable(group.sessions).map(keyOf);
  const on = keys.filter((k) => selected.has(k)).length;
  if (on === 0) return "none";
  return on === keys.length ? "all" : "some";
}

/** Ticks every pickable session of the group, or unticks them if all are ticked already. */
export function toggleGroup(selected: ReadonlySet<string>, group: FolderGroup): Set<string> {
  return toggleAllShown(selected, group.sessions);
}

/** Sessions that can be ticked (not already in Lullshift). */
export const pickable = (list: readonly FoundSession[]) => list.filter((s) => !s.tracked);

/**
 * "Select all shown": ticks every pickable session shown, or, if all of them are already ticked,
 * unticks them. Ticks on sessions hidden by the filter are kept.
 */
export function toggleAllShown(
  selected: ReadonlySet<string>,
  shown: readonly FoundSession[],
): Set<string> {
  const keys = pickable(shown).map(keyOf);
  const next = new Set(selected);
  const allOn = keys.length > 0 && keys.every((k) => next.has(k));
  for (const k of keys) {
    if (allOn) next.delete(k);
    else next.add(k);
  }
  return next;
}

export function toggleOne(selected: ReadonlySet<string>, s: FoundSession): Set<string> {
  const next = new Set(selected);
  if (s.tracked) return next;
  const k = keyOf(s);
  if (next.has(k)) next.delete(k);
  else next.add(k);
  return next;
}

/** The picked sessions, in list order, never one Lullshift already has. */
export function picksFrom(list: readonly FoundSession[], selected: ReadonlySet<string>) {
  return list
    .filter((s) => !s.tracked && selected.has(keyOf(s)))
    .map((s) => ({ provider: s.provider, sessionId: s.sessionId }));
}

/** The polite count line, e.g. "3 of 12 shown selected". */
export function countText(selectedCount: number, shownCount: number, totalCount: number): string {
  const sel = selectedCount === 1 ? "1 selected" : `${selectedCount} selected`;
  const shown =
    shownCount === totalCount
      ? `${totalCount} ${totalCount === 1 ? "session" : "sessions"} found`
      : `${shownCount} of ${totalCount} shown`;
  return `${shown}, ${sel}.`;
}

/** Honest notes about agents whose logs were not there, not readable or not fully read. */
export function agentNotes(agents: readonly AgentLogs[]): string[] {
  const out: string[] = [];
  for (const a of agents) {
    const name = providerName(a.provider);
    if (a.state === "not_found") out.push(`No ${name} logs found on this computer.`);
    else if (a.state === "unreadable") out.push(`${name} logs are there but couldn't be read.`);
    else if (a.state === "not_local")
      out.push(`${name} logs are not on a local drive, so Lullshift doesn't read them.`);
    else if (a.incomplete)
      out.push(`Only the newest ${name} sessions were read; older ones may be missing.`);
  }
  return out;
}

/** A row's accessible name: everything the row shows, in reading order. */
export function rowLabel(s: FoundSession, now: number): string {
  return [
    providerName(s.provider),
    s.title ? stripInvisible(s.title) : "Untitled session",
    s.project ? `in ${stripInvisible(s.project)}` : null,
    `last active ${relativeTime(s.lastActivityUtc, now)}`,
    GUESS_LABELS[s.guess],
    s.tracked ? "already in Lullshift" : null,
  ]
    .filter(Boolean)
    .join(", ");
}

export function resultText(r: ImportResult): string {
  const parts = [r.imported === 1 ? "Imported 1 session." : `Imported ${r.imported} sessions.`];
  if (r.alreadyTracked > 0)
    parts.push(
      `${r.alreadyTracked} ${r.alreadyTracked === 1 ? "was" : "were"} already in Lullshift.`,
    );
  if (r.notFound > 0)
    parts.push(
      `${r.notFound} ${r.notFound === 1 ? "is" : "are"} no longer in the agent's logs and ${r.notFound === 1 ? "was" : "were"} skipped.`,
    );
  return parts.join(" ");
}

export function errorText(code: unknown): string {
  switch (code) {
    case "busy":
      return "Lullshift is already reading the logs. Try again in a moment.";
    case "cancelled":
      return "Reading the logs was stopped. Nothing was imported.";
    case "store_unavailable":
      return "Local data is unavailable, so nothing was imported.";
    case "invalid_input":
      return "That selection couldn't be imported. Nothing was changed.";
    default:
      return "The agents' logs couldn't be read. Nothing was changed.";
  }
}
