import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import * as api from "./api";
import type { ErrorKind, ManageItem, MarketplaceEntry, UpdatePreview } from "./api";
import {
  canCheckUpdate,
  canInstall,
  checkGitInput,
  entryStatus,
  entryTitle,
  errorText,
  fetchedLine,
  formatBytes,
  hostOf,
  looksPrivate,
  manageSubtitle,
  permissionExplanation,
  shortCommit,
  THIRD_PARTY_CODE_TEXT,
  truncate,
  savedStateLine,
  updateView,
} from "./logic";

const KINDS: ErrorKind[] = [
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
  "unavailable",
];

const COMMIT = "a".repeat(40);
const OTHER = "b".repeat(40);

const preview = (over: Partial<UpdatePreview> = {}): UpdatePreview => ({
  token: "t",
  marketplaceId: "friends",
  activityId: "quiet-quiz",
  name: "Quiet Quiz",
  version: "1.1.0",
  description: "d",
  sourceUrl: "https://example.com/x",
  sourcePath: null,
  commit: OTHER,
  kind: "content",
  engine: "quiz@1",
  permissions: ["activity.storage"],
  sizeBytes: 2048,
  fileCount: 3,
  thirdParty: true,
  hasSavedState: false,
  from: { commit: COMMIT, version: "1.0.0", permissions: [] },
  addedPermissions: [],
  removedPermissions: [],
  kindChanged: false,
  engineChanged: false,
  sourceChanged: null,
  unchanged: false,
  ...over,
});

describe("errorText", () => {
  it("has a distinct honest message for every kind", () => {
    const texts = KINDS.map((kind) => errorText({ kind }));
    expect(new Set(texts).size).toBe(KINDS.length);
    for (const t of texts) expect(t.length).toBeGreaterThan(10);
  });

  it("uses the exact auth-denied wording", () => {
    expect(errorText({ kind: "authDenied" })).toBe(
      "Git couldn't access this repository with your current Git sign-in. Check you can clone it with git, then try again.",
    );
  });

  it("adds the backend sentence only for validation kinds, truncated", () => {
    expect(errorText({ kind: "invalidIndex", message: "missing field x" })).toContain(
      "Missing field x.",
    );
    expect(errorText({ kind: "invalidUrl", message: "addresses here aren't allowed" })).toMatch(
      /use\. Addresses here aren't allowed\.$/,
    );
    expect(errorText({ kind: "offline", message: "stderr: secret" })).not.toContain("secret");
    expect(errorText({ kind: "invalidPack", message: "y".repeat(500) }).length).toBeLessThan(300);
  });

  it("shows the backend sentence for already-exists, which is fixed text", () => {
    expect(errorText({ kind: "alreadyExists" })).toBe("That marketplace is already added.");
    const message = "Removed earlier; its activities come from a different address.";
    expect(errorText({ kind: "alreadyExists", message })).toBe(message);
  });

  it("falls back for an unexpected kind", () => {
    expect(errorText({ kind: "weird" as ErrorKind })).toBe(errorText({ kind: "internal" }));
  });
});

describe("permissionExplanation", () => {
  it("explains known permissions", () => {
    expect(permissionExplanation("activity.storage")).toMatch(/progress/);
  });
  it("flags unknown permissions instead of hiding them", () => {
    expect(permissionExplanation("net.fetch")).toMatch(/unrecognised/);
  });
});

describe("checkGitInput", () => {
  it("rejects empty input and control characters", () => {
    expect(checkGitInput("  ").ok).toBe(false);
    expect(checkGitInput("https://a.com/x\nrm").ok).toBe(false);
    expect(checkGitInput("https://a.com/x y").ok).toBe(false);
  });
  it("expands owner/repo to a GitHub https address with a hint", () => {
    expect(checkGitInput("acme/games")).toEqual({
      ok: true,
      value: "https://github.com/acme/games",
      hint: "Lullshift will use https://github.com/acme/games",
    });
    const dotGit = checkGitInput("acme/games.git");
    expect(dotGit.ok && dotGit.value).toBe("https://github.com/acme/games");
  });
  it("refuses credentials in the address", () => {
    // secretlint-disable-next-line -- fake credential: the test checks it is refused
    const r = checkGitInput("https://user:token@example.com/a/b.git");
    expect(r.ok).toBe(false);
    expect(!r.ok && r.reason).toMatch(/never stores/);
    expect(checkGitInput("https://token@example.com/a/b").ok).toBe(false);
  });
  it("accepts https and ssh forms and rejects others", () => {
    expect(checkGitInput(" https://example.com/a/b.git ")).toMatchObject({
      ok: true,
      value: "https://example.com/a/b.git",
    });
    expect(checkGitInput("git@example.com:a/b.git").ok).toBe(true);
    expect(checkGitInput("http://example.com/a/b").ok).toBe(false);
    expect(checkGitInput("file:///etc/passwd").ok).toBe(false);
    expect(checkGitInput("just-a-word").ok).toBe(false);
  });
});

describe("looksPrivate", () => {
  it("flags addresses on this computer or a private network, like the backend", () => {
    for (const url of [
      "https://localhost/a/b",
      "https://127.0.0.1:8443/a/b.git",
      "https://192.168.1.10/a/b",
      "https://8.8.8.8/a/b",
      "https://gitserver/a/b",
      "https://printer.local/a/b",
      "https://git.corp/a/b",
      "https://Server.LAN/a/b",
      "ssh://git@nas.home:2222/a/b",
      "git@gitserver:a/b.git",
    ]) {
      expect(looksPrivate(url), url).toBe(true);
    }
    for (const url of [
      "https://github.com/a/b",
      "https://gitlab.example.org:8443/a/b",
      "git@github.com:a/b.git",
      "acme/games",
      "",
    ]) {
      expect(looksPrivate(url), url).toBe(false);
    }
  });
  it("reads the host from each address form", () => {
    expect(hostOf("https://Git.Example.com:443/a")).toBe("git.example.com");
    expect(hostOf("ssh://git@host.example/a")).toBe("host.example");
    expect(hostOf("git@host.example:a/b")).toBe("host.example");
    expect(hostOf("nonsense")).toBeNull();
  });
});

describe("updateView", () => {
  it("reports unchanged as the latest version", () => {
    const v = updateView(preview({ unchanged: true, commit: COMMIT }));
    expect(v.unchanged).toBe(true);
    expect(v.headline).toBe("You have the latest version");
  });
  it("shows from/to version and short commits", () => {
    const v = updateView(preview());
    expect(v.versionLine).toBe("1.0.0 to 1.1.0");
    expect(v.commitLine).toBe("aaaaaaa to bbbbbbb");
    expect(v.warnings).toEqual([]);
  });
  it("highlights added permissions and lists removed ones", () => {
    const v = updateView(
      preview({ addedPermissions: ["activity.storage"], removedPermissions: ["x"] }),
    );
    expect(v.added).toEqual(["activity.storage"]);
    expect(v.removed).toEqual(["x"]);
    expect(v.warnings.join(" ")).toMatch(/permissions/);
  });
  it("flags kind and engine changes", () => {
    const v = updateView(preview({ kindChanged: true, engineChanged: true }));
    expect(v.warnings).toHaveLength(2);
  });
  it("highlights a changed source with both addresses", () => {
    expect(updateView(preview()).sourceChange).toBeNull();
    const v = updateView(
      preview({ sourceChanged: { old: "https://old.example/a", new: "https://new.example/b" } }),
    );
    expect(v.sourceChange).toEqual({ old: "https://old.example/a", new: "https://new.example/b" });
    expect(v.warnings.join(" ")).toMatch(/different source/);
  });
});

describe("savedStateLine", () => {
  it("says saved progress will be used, only when there is some", () => {
    expect(savedStateLine(preview())).toBeNull();
    expect(savedStateLine(preview({ hasSavedState: true }))).toBe(
      "Saved progress from an earlier install of this id will be used.",
    );
  });
});

describe("entries", () => {
  const entry = (over: Partial<MarketplaceEntry> = {}): MarketplaceEntry => ({
    activityId: "a-b",
    name: null,
    description: null,
    kind: "app",
    external: false,
    featured: false,
    installed: null,
    blocked: null,
    ...over,
  });
  it("falls back to the id and truncates long names", () => {
    expect(entryTitle(entry())).toBe("a-b");
    expect(entryTitle(entry({ name: "n".repeat(200) })).length).toBe(80);
  });
  it("blocks third-party code with the stated reason and no install", () => {
    const e = entry({ blocked: "thirdPartyCode" });
    expect(entryStatus(e).blocked).toBe(THIRD_PARTY_CODE_TEXT);
    expect(canInstall(e)).toBe(false);
  });
  it("does not offer install when already installed", () => {
    const e = entry({ installed: { commit: COMMIT, version: "1.0.0", enabled: false } });
    expect(canInstall(e)).toBe(false);
    expect(entryStatus(e).installed).toBe("Installed, version 1.0.0 (off)");
  });
});

describe("manage helpers", () => {
  const item = (over: Partial<ManageItem> = {}): ManageItem => ({
    activityId: "x",
    name: "X",
    kind: "content",
    builtin: false,
    marketplaceId: "friends",
    enabled: true,
    commit: COMMIT,
    version: "1.0.0",
    problem: null,
    ...over,
  });
  it("offers update checks only for installed, untampered items", () => {
    expect(canCheckUpdate(item())).toBe(true);
    expect(canCheckUpdate(item({ builtin: true }))).toBe(false);
    expect(canCheckUpdate(item({ problem: "tampered" }))).toBe(false);
  });
  it("describes the source", () => {
    expect(manageSubtitle(item())).toBe("Content pack · From friends · version 1.0.0");
    expect(manageSubtitle(item({ builtin: true, kind: "app" }))).toContain("Built in");
  });
});

describe("small formatters", () => {
  it("truncates by characters", () => {
    expect(truncate("abc", 5)).toBe("abc");
    expect(truncate("abcdef", 4)).toBe("abc…");
  });
  it("shortens commits and sizes honestly", () => {
    expect(shortCommit(COMMIT)).toBe("aaaaaaa");
    expect(shortCommit(null)).toBe("unknown");
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(2048)).toBe("2.0 KiB");
    expect(formatBytes(-1)).toBe("unknown size");
  });
  it("says when a marketplace was never fetched", () => {
    expect(fetchedLine(null, null, () => "x")).toBe("Not fetched yet");
    expect(fetchedLine(COMMIT, "2026-01-01T00:00:00Z", () => "2 days ago")).toBe(
      "Fetched 2 days ago · commit aaaaaaa",
    );
  });
});

describe("api error normalisation", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("passes structured errors through", async () => {
    invoke.mockRejectedValue({ kind: "offline", message: "No network." });
    expect(await api.marketplaceList().catch((e) => e)).toEqual({
      kind: "offline",
      message: "No network.",
    });
  });
  it("maps a missing command to unavailable", async () => {
    invoke.mockRejectedValue("Command marketplace_list not found");
    expect(await api.marketplaceList().catch((e) => e)).toMatchObject({ kind: "unavailable" });
  });
  it("maps anything else to internal without leaking text", async () => {
    invoke.mockRejectedValue("db exploded at C:\\secret");
    expect(await api.marketplaceList().catch((e) => e)).toEqual({ kind: "internal", message: "" });
  });
  it("sends the contract's argument names", async () => {
    invoke.mockResolvedValue(null);
    await api.marketplacePreviewInstall("m", "a");
    expect(invoke).toHaveBeenCalledWith("marketplace_preview_install", {
      marketplaceId: "m",
      activityId: "a",
    });
    await api.marketplaceAdd("https://10.0.0.5/a/b", true);
    expect(invoke).toHaveBeenCalledWith("marketplace_add", {
      url: "https://10.0.0.5/a/b",
      confirmPrivate: true,
    });
    await api.marketplaceAdd("acme/games");
    expect(invoke).toHaveBeenCalledWith("marketplace_add", {
      url: "acme/games",
      confirmPrivate: false,
    });
    await api.activityUninstall("a", true);
    expect(invoke).toHaveBeenCalledWith("activity_uninstall", {
      activityId: "a",
      deleteSavedState: true,
    });
  });
});
