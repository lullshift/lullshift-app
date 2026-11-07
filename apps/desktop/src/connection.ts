// Packaged-app context and connection-file problems (decision 33, ADR-010 amendment).
// The Rust core finds them; this module words them.

/** A Windows package: its family name and the readable part of it. */
export interface PackageRef {
  family: string;
  name: string;
}

/** `packaged_context`: the package Lullshift itself runs inside, if any. */
export interface PackagedContext {
  package: PackageRef | null;
}

export type ConnFile = "endpoint" | "token";
export type FailureReason = "rejected" | "unreachable";
export type ScanStatus = "complete" | "partial" | "skipped" | "unavailable";

export interface StaleCopy {
  package: PackageRef;
  files: ConnFile[];
}

export interface HookFailure {
  adapter: string;
  reason: FailureReason;
  /** RFC 3339. */
  utc: string;
  /** The package whose private copy held the marker; null for Lullshift's own data folder. */
  package: PackageRef | null;
}

/** `connection_health`. */
export interface ConnectionHealth {
  package: PackageRef | null;
  scan: ScanStatus;
  stale: StaleCopy[];
  failures: HookFailure[];
}

/** `connection_fix_stale`. */
export interface FixReport {
  deleted: ConnFile[];
  current: ConnFile[];
  failed: ConnFile[];
}

const AGENT: Record<string, string> = {
  "claude-code": "Claude Code",
  codex: "Codex",
  cursor: "Cursor",
  opencode: "OpenCode",
};

export function agentName(adapter: string): string {
  return AGENT[adapter] ?? "An agent";
}

/** Warning while Lullshift itself runs inside another app's package. */
export function packagedWarning(pkg: PackageRef): string {
  return `Lullshift was started from inside another app (${pkg.name}), so Windows keeps its connection files where agents outside that app can't see them, and agents may not reach it. Quit Lullshift from the tray, then start it from the Start menu or File Explorer.`;
}

/** Headline for a package that holds out-of-date copies of the connection files. */
export function staleHeadline(copy: StaleCopy): string {
  return `Agents running inside ${copy.package.name} can't reach Lullshift: that app has out-of-date copies of Lullshift's connection files.`;
}

/** What is out of date, in plain words. Never the token's content. */
export function staleDetail(files: readonly ConnFile[]): string {
  const parts: string[] = [];
  if (files.includes("endpoint")) parts.push("its address file points to an old address");
  if (files.includes("token")) parts.push("its access key is out of date");
  return parts.length === 0 ? "" : `In that app's private storage, ${parts.join(" and ")}.`;
}

/** What Fix does and doesn't do. */
export function fixExplanation(pkg: PackageRef): string {
  return `Fix deletes only those out-of-date copies, so agents inside ${pkg.name} use Lullshift's current files. Sessions already open there reach Lullshift from their next event; if one stays missing, restart it. Anything recorded while Lullshift itself ran inside ${pkg.name}, including that copy of its database, stays where it is; Fix doesn't touch it.`;
}

/** Local time of day, as in "13:05". */
export function clockTime(utc: string): string {
  const d = new Date(utc);
  if (Number.isNaN(d.getTime())) return "an unknown time";
  return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

/** Whether the app runs on Windows, where packaged apps and their private storage exist. */
export function onWindows(): boolean {
  return typeof navigator !== "undefined" && /Windows/i.test(navigator.userAgent);
}

/**
 * "Claude Code tried to reach Lullshift at 13:05 but couldn't (refused)." plus the likely cause.
 * The advice about starting Lullshift from the Start menu is for Windows only.
 */
export function failureText(
  f: HookFailure,
  time: (utc: string) => string = clockTime,
  windows: boolean = onWindows(),
): string {
  const head = `${agentName(f.adapter)} tried to reach Lullshift at ${time(f.utc)} but couldn't (${
    f.reason === "rejected" ? "refused" : "unreachable"
  }).`;
  const cause =
    f.reason === "rejected"
      ? "It used an out-of-date access key, so Lullshift refused its event."
      : "It used an address where Lullshift isn't listening, usually an old one.";
  const where = f.package
    ? ` It ran inside ${f.package.name}, which keeps its own copies of Lullshift's connection files.`
    : windows
      ? " If you started Lullshift from inside another app, quit it and start it from the Start menu or File Explorer."
      : "";
  return `${head} ${cause}${where}`;
}

/** Result of Fix, for a banner. */
export function fixResultText(
  pkg: PackageRef,
  r: FixReport,
): { tone: "success" | "info" | "danger"; text: string } {
  if (r.failed.length > 0) {
    return {
      tone: "danger",
      text: `Couldn't remove every out-of-date copy from ${pkg.name}. ${
        r.deleted.length > 0 ? "Some were removed. " : ""
      }Nothing else was changed. Close ${pkg.name} and try again.`,
    };
  }
  if (r.deleted.length > 0) {
    return {
      tone: "success",
      text: `Removed the out-of-date copies from ${pkg.name}. Agents there reach Lullshift from their next event; restart a session if it stays missing.`,
    };
  }
  return {
    tone: "info",
    text: `${pkg.name}'s copies are already up to date. Nothing was removed.`,
  };
}

const FIX_ERRORS: Record<string, string> = {
  packaged:
    "Lullshift is running inside another app, so it can't fix this from here. Start it from the Start menu or File Explorer first.",
  not_found: "Those copies are gone already. Nothing was removed.",
  invalid_package: "That app's storage couldn't be identified. Nothing was removed.",
  unsupported: "This only applies on Windows. Nothing was removed.",
};

export function fixErrorText(code: string): string {
  return FIX_ERRORS[code] ?? "Something went wrong. Nothing was removed.";
}

/** Whether a failure note should offer Fix: its package has stale copies right now. */
export function staleFor(health: ConnectionHealth, pkg: PackageRef | null): StaleCopy | null {
  if (!pkg) return null;
  return health.stale.find((s) => s.package.family === pkg.family) ?? null;
}

/** A note when the scan couldn't look everywhere, so a clean result isn't overstated. */
export function scanNote(scan: ScanStatus): string | null {
  switch (scan) {
    case "partial":
      return "Lullshift checked only some apps' private storage for out-of-date connection files (there were too many to check quickly).";
    case "unavailable":
      return "Lullshift couldn't check other apps' private storage for out-of-date connection files.";
    default:
      return null;
  }
}
