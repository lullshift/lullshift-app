// Home's status, in words (ADR-026 section 2). Pure, so the wording is unit-tested. It never
// claims a task is done: a quiet agent is "quiet", not "finished".

import { providerName } from "../format";
import { isImported, isTracked, liveState, type SessionView } from "../sessions";
import type { Face } from "../ui/Mascot";

export type HomeTone = "attention" | "active" | "calm" | "unknown";

export interface HomeStatus {
  tone: HomeTone;
  face: Face;
  title: string;
  line: string;
}

export interface StatusInput {
  /** The first dashboard has not arrived yet. */
  loading: boolean;
  /** The dashboard could not be read. */
  coreError: boolean;
  /** There is a dashboard to show, possibly the last good one. */
  hasDashboard: boolean;
  /** Health problems from `healthProblems` (data store or agent connection). */
  problems: readonly string[];
  /** Every session, most important first. */
  sessions: readonly SessionView[];
}

/** Sessions Home treats as live: tracked, not just imported, and not finished. */
export function liveSessions(sessions: readonly SessionView[]): SessionView[] {
  return sessions.filter((s) => {
    if (!isTracked(s) || isImported(s)) return false;
    return (
      s.state === "needs_attention" ||
      s.state === "working" ||
      s.state === "idle" ||
      s.state === "unknown"
    );
  });
}

const plural = (n: number, one: string, many: string) => (n === 1 ? one : many);

export function homeStatus({
  loading,
  coreError,
  hasDashboard,
  problems,
  sessions,
}: StatusInput): HomeStatus {
  if (!hasDashboard) {
    return loading || !coreError
      ? { tone: "unknown", face: "watching", title: "Checking your agents", line: "One moment." }
      : {
          tone: "unknown",
          face: "watching",
          title: "Agent status is unknown",
          line: "Lullshift couldn't read its own data. Nothing was changed. Try again.",
        };
  }
  const needs = sessions.filter((s) => liveState(s) === "needs_attention");
  const working = sessions.filter((s) => liveState(s) === "working");
  const notReporting = sessions.filter(
    (s) => s.reason !== "observed" && isTracked(s) && !isImported(s) && s.state !== "ended",
  ).length;
  const note =
    notReporting > 0
      ? ` ${notReporting === 1 ? "One session hasn't" : `${notReporting} sessions haven't`} reported recently.`
      : "";

  const [first] = needs;
  if (first) {
    return {
      tone: "attention",
      face: "awake",
      title:
        needs.length === 1
          ? `${providerName(first.provider)} needs you`
          : `${needs.length} sessions need you`,
      line:
        working.length > 0
          ? `${working.length} more ${plural(working.length, "is", "are")} still working.`
          : "Everything else is quiet.",
    };
  }
  if (problems.length > 0) {
    return {
      tone: "unknown",
      face: "watching",
      title: "Agent status is unknown",
      line: "Lullshift isn't getting updates from your agents, so what you see may be out of date.",
    };
  }
  if (working.length > 0) {
    return {
      tone: "active",
      face: "watching",
      title: plural(working.length, "Your agent is working", "Your agents are working"),
      line: `Nothing needs you. Take a few minutes for yourself.${note}`,
    };
  }
  if (sessions.length === 0) {
    return {
      tone: "calm",
      face: "asleep",
      title: "No agents yet",
      line: "Connect one and it will show up here. You can still play or rest.",
    };
  }
  return {
    tone: "calm",
    face: "asleep",
    title: "All quiet",
    line: `No agent is working right now.${note}`,
  };
}
