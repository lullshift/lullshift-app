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
  | "invalidUrl"
  | "invalidIndex"
  | "invalidPack"
  | "thirdPartyCodeBlocked"
  | "alreadyExists"
  | "builtinProtected"
  | "busy"
  | "tampered"
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
  "invalidUrl",
  "invalidIndex",
  "invalidPack",
  "thirdPartyCodeBlocked",
  "alreadyExists",
  "builtinProtected",
  "busy",
  "tampered",
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
  blocked: "thirdPartyCode" | null;
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
  /** Saved progress from an earlier install of this id exists and will be used. */
  hasSavedState: boolean;
}

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

export type ManageProblem = "tampered" | "missingEngine";

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
export const marketplaceInstall = (token: string) => call<null>("marketplace_install", { token });
export const marketplacePreviewUpdate = (activityId: string) =>
  call<UpdatePreview>("marketplace_preview_update", { activityId });
export const marketplaceUpdate = (token: string) => call<null>("marketplace_update", { token });
export const activityUninstall = (activityId: string, deleteSavedState: boolean) =>
  call<null>("activity_uninstall", { activityId, deleteSavedState });
export const activitySetEnabled = (activityId: string, enabled: boolean) =>
  call<null>("activity_set_enabled", { activityId, enabled });
export const activitiesManageList = () => call<ManageItem[]>("activities_manage_list");
