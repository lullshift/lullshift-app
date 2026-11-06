// Marketplace and activity-management commands (Sprint 08, ADR-015). This file mirrors the
// contract with the Tauri commands in `src-tauri`; change both together.
//
// Every command returns a `CommandError` ({ kind, message }) on failure. `message` is a short,
// safe sentence. A command that does not exist in this build rejects with `unavailable`.

import { invoke } from "@tauri-apps/api/core";

export type ErrorKind =
  | "offline"
  | "authDenied"
  | "notFound"
  | "timeout"
  | "gitMissing"
  /**
   * Lullshift's built-in Git (used when Git isn't installed) can't read this marketplace: it
   * asks for a sign-in, uses ssh, or is only reachable through a proxy (decision 28).
   */
  | "needsGit"
  | "invalidUrl"
  | "invalidIndex"
  | "invalidPack"
  /** Third-party code was sent to install or update without `acceptCodeRisk: true`. */
  | "codeRiskNotAccepted"
  | "alreadyExists"
  | "builtinProtected"
  | "busy"
  | "tampered"
  /** An install's file exists but couldn't be read just now (locked by another program). */
  | "unreadable"
  /** The address is on this computer or a private network and the user hasn't confirmed it. */
  | "privateHost"
  /** The repository's server redirected; Lullshift didn't follow. See `moved`. */
  | "moved"
  | "internal"
  /** UI only: the backend does not have this command (older or partial build). */
  | "unavailable";

/** A redirect Lullshift reported instead of following. `url` is checked https; show as text. */
export interface MovedTo {
  url: string;
  /** Same host and port: the user can accept it in one step. */
  sameHost: boolean;
}

export interface CommandError {
  kind: ErrorKind;
  message: string;
  /** Only for `moved`. */
  moved?: MovedTo;
}

const KINDS: readonly string[] = [
  "offline",
  "authDenied",
  "notFound",
  "timeout",
  "gitMissing",
  "needsGit",
  "invalidUrl",
  "invalidIndex",
  "invalidPack",
  "codeRiskNotAccepted",
  "alreadyExists",
  "builtinProtected",
  "busy",
  "tampered",
  "unreadable",
  "privateHost",
  "moved",
  "internal",
];

function movedOf(v: unknown): MovedTo | undefined {
  if (!v || typeof v !== "object") return undefined;
  const m = v as { url?: unknown; sameHost?: unknown };
  if (typeof m.url !== "string" || !m.url.startsWith("https://") || m.url.length > 300) {
    return undefined;
  }
  return { url: m.url, sameHost: m.sameHost === true };
}

/** Turns whatever `invoke` rejected with into a `CommandError`. Never throws. */
export function toCommandError(err: unknown): CommandError {
  if (err && typeof err === "object" && "kind" in err) {
    const e = err as { kind?: unknown; message?: unknown; moved?: unknown };
    if (typeof e.kind === "string" && KINDS.includes(e.kind)) {
      const out: CommandError = {
        kind: e.kind as ErrorKind,
        message: typeof e.message === "string" ? e.message : "",
      };
      const moved = e.kind === "moved" ? movedOf(e.moved) : undefined;
      if (moved) out.moved = moved;
      return out;
    }
  }
  const text = typeof err === "string" ? err : err instanceof Error ? err.message : "";
  // Tauri reports a missing or unpermitted command as a plain string.
  if (/not found|not allowed|unknown command/i.test(text)) {
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

export interface Marketplace {
  /** The index `name`; "lullshift" for the built-in one. */
  id: string;
  /** Display form, never with credentials. Show as text, never as a link. */
  url: string;
  builtin: boolean;
  description: string | null;
  /** Self-declared and unverified. */
  owner: string | null;
  lastFetchedCommit: string | null;
  /** RFC 3339. */
  lastFetchedAt: string | null;
  /** The user confirmed it is on their own computer or network. */
  privateHost: boolean;
}

export type ActivityKind = "app" | "content";

export interface MarketplaceEntry {
  activityId: string;
  name: string | null;
  description: string | null;
  kind: ActivityKind | null;
  /** The source is another Git repository. */
  external: boolean;
  featured: boolean;
  installed: { commit: string; version: string; enabled: boolean } | null;
  /**
   * Code from a third-party marketplace (owner decision 27.5): installing it shows the risk
   * warning and asks for consent. Known up front only for same-repository entries.
   */
  thirdPartyCode: boolean;
}

/**
 * What the WebRTC block is known to be on this computer, for the code-risk warning. None of
 * these is a guarantee.
 * - windowsNotReadBack: the sandbox proof passed on Windows; the block is set by browser
 *   arguments Lullshift can't read back.
 * - unverified: macOS or Linux; the block is in place and read back, but the sandbox proof
 *   hasn't run on this platform.
 * - blockFailed: the block couldn't be applied on this computer.
 * - notChecked: not checked yet, or an unexpected state.
 */
export type CodeRiskStatus = "windowsNotReadBack" | "unverified" | "blockFailed" | "notChecked";

export interface CodeRisk {
  platform: "windows" | "macos" | "linux";
  status: CodeRiskStatus;
}

export interface InstallPreview {
  /** Opaque, single use, expires after 10 minutes. */
  token: string;
  marketplaceId: string;
  activityId: string;
  name: string;
  version: string;
  description: string;
  sourceUrl: string;
  sourcePath: string | null;
  /** 40 hex characters. */
  commit: string;
  kind: ActivityKind;
  /** "id@major" */
  engine: string | null;
  permissions: string[];
  sizeBytes: number;
  fileCount: number;
  /** True for any marketplace other than the built-in one. */
  thirdParty: boolean;
  /**
   * Set for third-party code only. Installing it, or updating it to a new commit, needs the
   * user's consent (`acceptCodeRisk: true`). Missing on older hosts.
   */
  codeRisk?: CodeRisk | null;
  /** Saved progress from an earlier install of this id exists and will be used. */
  hasSavedState: boolean;
  /**
   * Kept progress for this id: none; own (same marketplace and source, used); unknown (older
   * than the source record, used); foreign (another marketplace's or source's: the review asks
   * whether to delete it, deleted by default). Missing on older hosts.
   */
  savedState?: SavedStateKind;
}

export type SavedStateKind = "none" | "own" | "unknown" | "foreign";

export interface UpdatePreview extends InstallPreview {
  from: { commit: string; version: string; permissions: string[] };
  addedPermissions: string[];
  removedPermissions: string[];
  kindChanged: boolean;
  engineChanged: boolean;
  /** Set when the update comes from a different repository than the install did. */
  sourceChanged: { old: string; new: string } | null;
  /** Same commit: nothing to update. */
  unchanged: boolean;
}

/** `unreadable`: a file couldn't be read during the last check; it is checked again next time. */
export type ManageProblem = "tampered" | "missingEngine" | "unreadable";

export interface ManageItem {
  activityId: string;
  name: string;
  kind: ActivityKind;
  builtin: boolean;
  marketplaceId: string;
  enabled: boolean;
  commit: string | null;
  version: string;
  problem: ManageProblem | null;
  /** Its marketplace moved and this pinned version wasn't found at the new address. */
  sourceNote: "notFoundAtNewAddress" | null;
}

/** The result of a confirmed change of address. */
export interface AddressChanged {
  marketplace: Marketplace;
  /** Ids of installs whose pinned commit wasn't at the new address; they stay as installed. */
  notFound: string[];
}

export const marketplaceList = () => call<Marketplace[]>("marketplace_list");

/**
 * Which Git marketplaces use: "system" (Git installed on this computer) or "builtin"
 * (Lullshift's built-in Git, public https only). Anything else reads as unknown (null).
 */
export type GitBackend = "system" | "builtin";
export const marketplaceGitBackend = async (): Promise<GitBackend | null> => {
  const r = await call<{ backend?: unknown }>("marketplace_git_backend");
  return r && (r.backend === "system" || r.backend === "builtin") ? r.backend : null;
};
/** `confirmPrivate` is the "This marketplace is on my own computer or network" checkbox. */
export const marketplaceAdd = (url: string, confirmPrivate = false) =>
  call<Marketplace>("marketplace_add", { url, confirmPrivate });
export const marketplaceRemove = (id: string) => call<null>("marketplace_remove", { id });
/** `confirmPrivate`: the user ticked the private-network box after a `privateHost` answer. */
export const marketplaceRefresh = (id: string, confirmPrivate = false) =>
  call<Marketplace>("marketplace_refresh", { id, confirmPrivate });
/** Moves a marketplace to `url` after the user saw both addresses and confirmed. */
export const marketplaceChangeAddress = (id: string, url: string, confirmPrivate = false) =>
  call<AddressChanged>("marketplace_change_address", { id, url, confirmPrivate });
export const marketplaceEntries = (id: string) =>
  call<MarketplaceEntry[]>("marketplace_entries", { id });
export const marketplacePreviewInstall = (marketplaceId: string, activityId: string) =>
  call<InstallPreview>("marketplace_preview_install", { marketplaceId, activityId });
/**
 * Installs a reviewed activity. `keepSavedState` only matters when the review said
 * `savedState: "foreign"`: true keeps that progress, false (the default) deletes it.
 * `acceptCodeRisk` is the consent box under the third-party code warning; the host refuses
 * third-party code without it.
 */
export const marketplaceInstall = (token: string, keepSavedState = false, acceptCodeRisk = false) =>
  call<null>("marketplace_install", { token, keepSavedState, acceptCodeRisk });
export const marketplacePreviewUpdate = (activityId: string) =>
  call<UpdatePreview>("marketplace_preview_update", { activityId });
/** `acceptCodeRisk`: needed to update third-party code to a new commit. */
export const marketplaceUpdate = (token: string, acceptCodeRisk = false) =>
  call<null>("marketplace_update", { token, acceptCodeRisk });
export const activityUninstall = (activityId: string, deleteSavedState: boolean) =>
  call<null>("activity_uninstall", { activityId, deleteSavedState });
export const activitySetEnabled = (activityId: string, enabled: boolean) =>
  call<null>("activity_set_enabled", { activityId, enabled });
export const activitiesManageList = () => call<ManageItem[]>("activities_manage_list");
