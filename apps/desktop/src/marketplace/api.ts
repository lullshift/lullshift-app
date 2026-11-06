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
  | "internal"
  /** UI only: the backend does not have this command (older or partial build). */
  | "unavailable";

export interface CommandError {
  kind: ErrorKind;
  message: string;
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
  "internal",
];

/** Turns whatever `invoke` rejected with into a `CommandError`. Never throws. */
export function toCommandError(err: unknown): CommandError {
  if (err && typeof err === "object" && "kind" in err) {
    const e = err as { kind?: unknown; message?: unknown };
    if (typeof e.kind === "string" && KINDS.includes(e.kind)) {
      return {
        kind: e.kind as ErrorKind,
        message: typeof e.message === "string" ? e.message : "",
      };
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
}

export const marketplaceList = () => call<Marketplace[]>("marketplace_list");
/** `confirmPrivate` is the "This marketplace is on my own computer or network" checkbox. */
export const marketplaceAdd = (url: string, confirmPrivate = false) =>
  call<Marketplace>("marketplace_add", { url, confirmPrivate });
export const marketplaceRemove = (id: string) => call<null>("marketplace_remove", { id });
/** `confirmPrivate`: the user ticked the private-network box after a `privateHost` answer. */
export const marketplaceRefresh = (id: string, confirmPrivate = false) =>
  call<Marketplace>("marketplace_refresh", { id, confirmPrivate });
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
