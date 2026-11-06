// Pure presentation logic for the marketplace screens. Everything a marketplace says (names,
// descriptions, URLs, permissions) is untrusted plain text: it is truncated here and rendered by
// React as text only, never as HTML, a link or a URL we open.

import type {
  CommandError,
  ErrorKind,
  InstallPreview,
  ManageItem,
  ManageProblem,
  MarketplaceEntry,
  UpdatePreview,
} from "./api";

export const UNAVAILABLE_TEXT = "Marketplaces aren't available in this build.";
export const THIRD_PARTY_CODE_TEXT =
  "Code activities from other marketplaces can't be installed in this version.";
export const SAVED_STATE_TEXT = "Saved progress from an earlier install of this id will be used.";
export const PRIVATE_HOST_LABEL = "This marketplace is on my own computer or network";
export const PRIVATE_HOST_TEXT =
  "This address is on your own computer or local network. If that's what you expect, tick the box below and try again.";
export const PRIVATE_HOST_NOTE =
  "Only tick this for a Git server you or your organisation runs. Lullshift will connect to it when you press Add, Refresh or Change address.";
export const UNVERIFIED_TEXT =
  "Unverified: anyone can publish a marketplace. Lullshift checked the files but not who made them.";

const ERROR_TEXT: Record<ErrorKind, string> = {
  offline: "You appear to be offline. Check your connection and try again.",
  authDenied:
    "Git couldn't access this repository with your current Git sign-in. Check you can clone it with git, then try again.",
  notFound: "That repository or item wasn't found. Check the address and try again.",
  timeout: "Git took too long to answer. Try again in a moment.",
  gitMissing: "Git isn't installed or can't be found. Install Git, then try again.",
  invalidUrl: "That doesn't look like a Git address Lullshift can use.",
  invalidIndex: "This repository doesn't have a valid Lullshift marketplace.",
  invalidPack: "This activity's files didn't pass Lullshift's checks, so it can't be installed.",
  thirdPartyCodeBlocked: THIRD_PARTY_CODE_TEXT,
  alreadyExists: "That marketplace is already added.",
  builtinProtected: "The built-in marketplace and its activities can't be removed.",
  busy: "Another operation on this marketplace is still running. Try again shortly.",
  tampered: "This activity's files changed after install, so Lullshift won't run it.",
  privateHost: PRIVATE_HOST_TEXT,
  moved: "This repository has moved. Lullshift didn't follow the redirect.",
  internal: "Something went wrong. Nothing was changed.",
  unavailable: UNAVAILABLE_TEXT,
};

/** Kinds where the backend's own short sentence adds useful detail. */
const DETAILED: ReadonlySet<ErrorKind> = new Set(["invalidUrl", "invalidIndex", "invalidPack"]);

/** Kinds whose backend sentence is fixed text that says more than ours, so it replaces ours. */
const REPLACED: ReadonlySet<ErrorKind> = new Set(["alreadyExists", "moved"]);

export function truncate(text: string, max: number): string {
  const chars = Array.from(text);
  return chars.length <= max ? text : `${chars.slice(0, max - 1).join("")}…`;
}

/** The sentence to show for a failed command. */
export function errorText(err: Pick<CommandError, "kind"> & { message?: string }): string {
  const base = ERROR_TEXT[err.kind] ?? ERROR_TEXT.internal;
  const detail = err.message?.trim();
  if (detail && REPLACED.has(err.kind)) return truncate(detail, 240);
  if (detail && DETAILED.has(err.kind)) {
    const sentence = detail.charAt(0).toUpperCase() + detail.slice(1);
    const ended = /[.!?]$/.test(sentence) ? sentence : `${sentence}.`;
    return `${base} ${truncate(ended, 160)}`;
  }
  return base;
}

const PERMISSION_TEXT: Record<string, string> = {
  "activity.storage": "Saves its progress on this device, so you can resume where you left off.",
};

/** A plain-language line for a permission; unknown ones are flagged, never hidden. */
export function permissionExplanation(permission: string): string {
  return (
    PERMISSION_TEXT[permission] ??
    "An unrecognised permission. Lullshift doesn't know what it allows, so be careful."
  );
}

export function permissionLabel(permission: string): string {
  return truncate(permission, 60);
}

export function shortCommit(commit: string | null | undefined): string {
  return commit ? commit.slice(0, 7) : "unknown";
}

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "unknown size";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KiB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MiB`;
}

export function kindLabel(kind: "app" | "content" | null): string {
  if (kind === "app") return "Code activity";
  if (kind === "content") return "Content pack";
  return "Unknown type";
}

export function entryTitle(e: MarketplaceEntry): string {
  return truncate(e.name?.trim() || e.activityId, 80);
}

export function entryStatus(e: MarketplaceEntry): {
  installed: string | null;
  blocked: string | null;
} {
  const installedBits = e.installed
    ? `Installed${e.installed.version ? `, version ${truncate(e.installed.version, 20)}` : ""}${
        e.installed.enabled ? "" : " (off)"
      }`
    : null;
  return {
    installed: installedBits,
    blocked: e.blocked === "thirdPartyCode" ? THIRD_PARTY_CODE_TEXT : null,
  };
}

export function canInstall(e: MarketplaceEntry): boolean {
  return e.blocked === null && e.installed === null;
}

export type UrlCheck =
  { ok: true; value: string; hint: string | null } | { ok: false; reason: string };

const OWNER_REPO = /^[A-Za-z0-9][A-Za-z0-9._-]*\/[A-Za-z0-9][A-Za-z0-9._-]*$/;
// eslint-disable-next-line no-control-regex
const SPACE_OR_CONTROL = /[\s\u0000-\u001f\u007f]/;

/**
 * Light checks before we send the input to the backend (which validates for real). An
 * `owner/repo` shorthand becomes a GitHub HTTPS address; credentials in a URL are refused here so
 * a pasted token is never even sent.
 */
export function checkGitInput(raw: string): UrlCheck {
  const input = raw.trim();
  if (!input) return { ok: false, reason: "Enter a Git address or owner/repo." };
  if (SPACE_OR_CONTROL.test(input)) {
    return { ok: false, reason: "The address can't contain spaces or control characters." };
  }
  if (OWNER_REPO.test(input)) {
    const repo = input.replace(/\.git$/i, "");
    const value = `https://github.com/${repo}`;
    return { ok: true, value, hint: `Lullshift will use ${value}` };
  }
  if (/^[a-z][a-z0-9+.-]*:\/\/[^/]*@/i.test(input)) {
    return {
      ok: false,
      reason:
        "Remove the user name or token from the address. Lullshift never stores passwords or tokens; it uses your existing Git sign-in.",
    };
  }
  if (!/^https:\/\//i.test(input) && !/^git@[^:]+:.+/.test(input) && !/^ssh:\/\//i.test(input)) {
    return { ok: false, reason: "Use an https:// address, an SSH address, or owner/repo." };
  }
  return { ok: true, value: input, hint: null };
}

const LOCAL_SUFFIXES = new Set([
  "localhost",
  "local",
  "internal",
  "lan",
  "localdomain",
  "intranet",
  "corp",
  "home",
  "private",
  "arpa",
]);

/** The host part of an address the user typed, lowercased, or null if there isn't one. */
export function hostOf(value: string): string | null {
  const m =
    /^(?:https|ssh):\/\/(?:[^@/]*@)?([^/:]+)(?::\d+)?\//i.exec(value) ??
    /^git@([^:/]+):/i.exec(value);
  return m?.[1] ? m[1].toLowerCase() : null;
}

/**
 * True if the address names this computer or a private network the way the backend reads it: an
 * IPv4 address, `localhost`, a one-label name or a local suffix such as `.local` or `.lan`. Such
 * an address needs the "on my own computer or network" confirmation. A public-looking name can
 * still resolve to a private address; the backend checks that and answers `privateHost`.
 */
export function looksPrivate(value: string): boolean {
  const host = hostOf(value.trim());
  if (!host) return false;
  const labels = host.split(".");
  const last = labels[labels.length - 1] ?? "";
  if (/^\d/.test(last)) return true;
  return labels.length < 2 || last.length < 2 || LOCAL_SUFFIXES.has(last);
}

export function problemText(problem: ManageProblem): string {
  return problem === "tampered"
    ? "Its files changed after install, so Lullshift won't run it. Uninstall it, or install it again from its marketplace."
    : "The engine it needs isn't available in this version of Lullshift, so it can't run.";
}

export function manageSubtitle(item: ManageItem): string {
  const source = item.builtin ? "Built in" : `From ${truncate(item.marketplaceId, 40)}`;
  const version = item.version ? `version ${truncate(item.version, 20)}` : null;
  return [kindLabel(item.kind), source, version].filter(Boolean).join(" · ");
}

export function canCheckUpdate(item: ManageItem): boolean {
  return !item.builtin && item.problem !== "tampered";
}

/** How a marketplace's last fetch reads: honest about "never". */
export function fetchedLine(
  commit: string | null,
  at: string | null,
  relative: (iso: string) => string,
): string {
  if (!at && !commit) return "Not fetched yet";
  const when = at ? `Fetched ${relative(at)}` : "Fetch time unknown";
  return commit ? `${when} · commit ${shortCommit(commit)}` : when;
}

export interface UpdateView {
  unchanged: boolean;
  headline: string;
  versionLine: string;
  commitLine: string;
  /** New access the user must approve. */
  added: string[];
  removed: string[];
  /** Set when the update comes from a different repository than the install did. */
  sourceChange: { old: string; new: string } | null;
  warnings: string[];
}

/** What the update review shows, derived from the preview. */
export function updateView(p: UpdatePreview): UpdateView {
  const warnings: string[] = [];
  if (p.kindChanged) warnings.push("The type of activity changed (content pack or code activity).");
  if (p.engineChanged) warnings.push("It now runs on a different engine.");
  if (p.addedPermissions.length > 0)
    warnings.push("It asks for permissions it didn't have before.");
  if (p.sourceChanged) warnings.push("It now comes from a different source.");
  return {
    unchanged: p.unchanged,
    headline: p.unchanged ? "You have the latest version" : "An update is available",
    versionLine: `${truncate(p.from.version, 20)} to ${truncate(p.version, 20)}`,
    commitLine: `${shortCommit(p.from.commit)} to ${shortCommit(p.commit)}`,
    added: p.addedPermissions,
    removed: p.removedPermissions,
    warnings,
    sourceChange: p.sourceChanged
      ? { old: truncate(p.sourceChanged.old, 300), new: truncate(p.sourceChanged.new, 300) }
      : null,
  };
}

/** The line that says saved progress will be reused, or null. */
export function savedStateLine(p: InstallPreview): string | null {
  return p.hasSavedState ? SAVED_STATE_TEXT : null;
}

export function installSummary(p: InstallPreview): string {
  return `${p.fileCount} ${p.fileCount === 1 ? "file" : "files"}, ${formatBytes(p.sizeBytes)}`;
}
