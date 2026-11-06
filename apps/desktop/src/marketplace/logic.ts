// Pure presentation logic for the marketplace screens. Everything a marketplace says (names,
// descriptions, URLs, permissions) is untrusted plain text: it is truncated here and rendered by
// React as text only, never as HTML, a link or a URL we open.

import type {
  CodeRisk,
  CommandError,
  ErrorKind,
  InstallPreview,
  ManageItem,
  ManageProblem,
  MarketplaceEntry,
  UpdatePreview,
} from "./api";

export const UNAVAILABLE_TEXT = "Marketplaces aren't available in this build.";
export const UNREADABLE_TEXT =
  "Lullshift couldn't read its files just now. Another program, such as a virus scanner, may be using them. Nothing was changed; try again in a moment.";
/** The short label for code from a third-party marketplace, wherever it is listed. */
export const THIRD_PARTY_PROGRAM_LABEL = "Third-party program";
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
  codeRiskNotAccepted:
    "Lullshift didn't install this program, because the box under the warning wasn't ticked. Nothing was changed; review it again.",
  alreadyExists: "That marketplace is already added.",
  builtinProtected: "The built-in marketplace and its activities can't be removed.",
  busy: "Another operation on this marketplace is still running. Try again shortly.",
  tampered: "This activity's files changed after install, so Lullshift won't run it.",
  unreadable: UNREADABLE_TEXT,
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
  /** "Third-party program" for code from this (non-built-in) marketplace, else null. */
  program: string | null;
} {
  const installedBits = e.installed
    ? `Installed${e.installed.version ? `, version ${truncate(e.installed.version, 20)}` : ""}${
        e.installed.enabled ? "" : " (off)"
      }`
    : null;
  return {
    installed: installedBits,
    program: e.thirdPartyCode === true ? THIRD_PARTY_PROGRAM_LABEL : null,
  };
}

export function canInstall(e: MarketplaceEntry): boolean {
  return e.installed === null;
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
  switch (problem) {
    case "tampered":
      return "Its files changed on disk after install, so Lullshift turned it off and won't run it. Install it again from its marketplace, or uninstall it.";
    case "unreadable":
      return UNREADABLE_TEXT;
    case "missingEngine":
      return "The engine it needs isn't available in this version of Lullshift, so it can't run.";
  }
}

/** The short label shown before a problem's explanation in Manage. */
export function problemLabel(problem: ManageProblem): string {
  switch (problem) {
    case "tampered":
      return "Damaged, reinstall it. ";
    case "unreadable":
      return "Couldn't be read just now. ";
    case "missingEngine":
      return "Engine missing. ";
  }
}

export const NOT_FOUND_AT_NEW_ADDRESS_TEXT =
  "Its marketplace moved, and this version wasn't found at the new address. It keeps working as installed, with your saved progress; it may not get updates.";

/** The summary after a change of address. */
export function addressChangedText(id: string, notFound: string[]): string {
  const name = truncate(id, 60);
  if (notFound.length === 0) {
    return `${name} now uses its new address. Installed activities keep their versions and saved progress.`;
  }
  const list = notFound.map((a) => truncate(a, 60)).join(", ");
  return `${name} now uses its new address. Not found there, so kept exactly as installed: ${list}.`;
}

/** What the change-address dialog may submit: a checked, different address. */
export function changeAddressCheck(current: string, raw: string): UrlCheck {
  const c = checkGitInput(raw);
  if (!c.ok) return c;
  if (c.value === current) {
    return { ok: false, reason: "That is already this marketplace's address." };
  }
  return c;
}

export function manageSubtitle(item: ManageItem): string {
  const source = item.builtin ? "Built in" : `From ${truncate(item.marketplaceId, 40)}`;
  const version = item.version ? `version ${truncate(item.version, 20)}` : null;
  const kind =
    !item.builtin && item.kind === "app" ? THIRD_PARTY_PROGRAM_LABEL : kindLabel(item.kind);
  return [kind, source, version].filter(Boolean).join(" · ");
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
  return p.hasSavedState && !asksAboutSavedState(p) ? SAVED_STATE_TEXT : null;
}

export const FOREIGN_STATE_TEXT =
  "This device has saved progress for an activity with this id that came from a different marketplace or repository.";
export const FOREIGN_STATE_DELETE = "Delete that saved progress (recommended)";
export const FOREIGN_STATE_KEEP = "Keep it for this install";
export const FOREIGN_STATE_KEEP_WARNING =
  "Only keep it if you trust both sources. The new activity will be able to read everything the old one saved.";

/** Whether the review must ask about saved progress from another source (decision 18.6). */
export function asksAboutSavedState(p: InstallPreview): boolean {
  return p.savedState === "foreign";
}

export function installSummary(p: InstallPreview): string {
  return `${p.fileCount} ${p.fileCount === 1 ? "file" : "files"}, ${formatBytes(p.sizeBytes)}`;
}

// ---- third-party code: risk warning and consent (owner decision 27.5) ---------------------

export const CODE_RISK_HEADING = "This is someone else's program";
export const CODE_RISK_CONSENT_LABEL = "I understand and want to install this program";
export const CODE_RISK_TICK_HINT = "Tick the box above to continue.";
export const CODE_RISK_WORST_CASE =
  "Worst case: if the sandbox has a gap Lullshift doesn't know about, this program could try to send out what it shows you or what you type into it. Don't type passwords, keys or other private information into it.";
export const CODE_RISK_NO_GUARANTEE =
  "Lullshift can't guarantee that its sandbox has no gaps. Only install programs from people you trust.";

const PLATFORM_NAME: Record<CodeRisk["platform"], string> = {
  windows: "Windows",
  macos: "macOS",
  linux: "Linux",
};

/** The verification line: honest about what is and isn't known on this computer. */
export function codeRiskVerification(risk: CodeRisk): string {
  const platform = PLATFORM_NAME[risk.platform] ?? "this system";
  switch (risk.status) {
    case "windowsNotReadBack":
      return "On Windows, Lullshift's sandbox checks passed on a test computer. WebRTC, a way a web page could send data out, is blocked by settings Lullshift gives the browser engine when it starts. Lullshift can't read those settings back, so it can't confirm the block on this computer.";
    case "unverified":
      return `On ${platform}, Lullshift turned off WebRTC, a way a web page could send data out, and confirmed the setting. But Lullshift's full sandbox checks haven't been run on ${platform} yet, so the sandbox is not verified on this platform.`;
    case "blockFailed":
      return "Lullshift couldn't turn off WebRTC on this computer. WebRTC is a way a web page could send data out, so this program may be able to send data out.";
    case "notChecked":
    default:
      return "Lullshift hasn't confirmed yet that WebRTC, a way a web page could send data out, is blocked on this computer.";
  }
}

export interface CodeRiskView {
  heading: string;
  /** What the program can and can't do, in order. */
  points: string[];
  verification: string;
  /** True when the block is known to have failed: show it with more weight. */
  failed: boolean;
  worstCase: string;
  noGuarantee: string;
  consentLabel: string;
}

/**
 * Whether the review is for third-party code and so needs the consent box. A host that sent no
 * `codeRisk` for third-party code still gets the warning (it refuses the install without
 * consent anyway).
 */
export function needsCodeConsent(p: InstallPreview): boolean {
  return Boolean(p.codeRisk) || (p.kind === "app" && p.thirdParty);
}

/** The warning for a third-party code review, or null when the review needs none. */
export function codeRiskView(p: InstallPreview): CodeRiskView | null {
  if (!needsCodeConsent(p)) return null;
  // Without a status from the host, say only that nothing was confirmed.
  const risk: CodeRisk = p.codeRisk ?? { platform: "linux", status: "notChecked" };
  const perms =
    p.permissions.length === 0
      ? "It asks for no permissions."
      : `It asks for: ${p.permissions.map((x) => permissionLabel(x)).join(", ")}.`;
  return {
    heading: CODE_RISK_HEADING,
    points: [
      "It runs inside Lullshift's sandbox, the same one the built-in activities use.",
      "By design it has no access to your files, the network, your clipboard or the rest of Lullshift.",
      perms,
    ],
    verification: codeRiskVerification(risk),
    failed: risk.status === "blockFailed",
    worstCase: CODE_RISK_WORST_CASE,
    noGuarantee: CODE_RISK_NO_GUARANTEE,
    consentLabel: CODE_RISK_CONSENT_LABEL,
  };
}

/**
 * Whether the review's Install or Update button may be pressed: third-party code needs the
 * consent box ticked; everything else needs nothing more.
 */
export function canConfirmReview(p: InstallPreview, consented: boolean): boolean {
  return !needsCodeConsent(p) || consented;
}

/**
 * The label shown beside an activity in the library and in its header: "Third-party program
 * from <marketplace>" for code from a non-built-in marketplace, "From <marketplace>" for a pack
 * from one, null for built-in activities.
 */
export function activitySourceLabel(
  source: { marketplaceId: string; builtin: boolean } | undefined,
  engine: string | null,
): string | null {
  if (!source || source.builtin) return null;
  const from = truncate(source.marketplaceId, 40);
  return engine === null ? `${THIRD_PARTY_PROGRAM_LABEL} from ${from}` : `From ${from}`;
}
