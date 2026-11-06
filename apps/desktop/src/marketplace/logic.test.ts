import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import * as api from "./api";
import type {
  CodeRisk,
  CodeRiskStatus,
  ErrorKind,
  ManageItem,
  MarketplaceEntry,
  UpdatePreview,
} from "./api";
import {
  activitySourceLabel,
  addressChangedText,
  asksAboutSavedState,
  canConfirmReview,
  CODE_RISK_CONSENT_LABEL,
  CODE_RISK_HEADING,
  codeRiskVerification,
  codeRiskView,
  needsCodeConsent,
  THIRD_PARTY_PROGRAM_LABEL,
  FOREIGN_STATE_DELETE,
  FOREIGN_STATE_KEEP_WARNING,
  ADDRESS_CHANGED_SINCE_CONSENT,
  addressChangedSinceConsent,
  FOREIGN_STATE_TEXT,
  canCheckUpdate,
  canInstall,
  changeAddressCheck,
  checkGitInput,
  entryStatus,
  entryTitle,
  errorText,
  fetchedLine,
  gitBackendText,
  formatBytes,
  hostOf,
  looksPrivate,
  manageSubtitle,
  permissionExplanation,
  problemLabel,
  problemText,
  shortCommit,
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

  it("tells the user to install Git when the built-in Git can't read a marketplace", () => {
    const text = errorText({ kind: "needsGit", message: "ignored detail" });
    expect(text).toMatch(/^Lullshift's built-in Git couldn't read this marketplace\./);
    // Honest about 403: it may be rate limiting, and Git only helps for private repositories.
    expect(text).toContain("limiting requests");
    expect(text).toContain("only for private repositories");
    expect(text).not.toContain("asks for a sign-in");
    // Different from the generic "Git is missing" message, and never a link.
    expect(text).not.toBe(errorText({ kind: "gitMissing" }));
    expect(text).not.toMatch(/https?:\/\//);
    expect(api.toCommandError({ kind: "needsGit", message: "x" }).kind).toBe("needsGit");
  });

  it("describes which Git is in use, and nothing when unknown", () => {
    expect(gitBackendText("system")).toBe("Using Git installed on this computer.");
    expect(gitBackendText("builtin")).toMatch(
      /^Using Lullshift's built-in Git \(public https only\)\./,
    );
    expect(gitBackendText("builtin")).not.toMatch(/https?:\/\//);
    expect(gitBackendText(null)).toBeNull();
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

describe("change address", () => {
  it("needs a valid address that differs from the current one", () => {
    const now = "https://github.com/acme/market";
    expect(changeAddressCheck(now, now)).toMatchObject({ ok: false });
    expect(changeAddressCheck(now, "acme/market")).toMatchObject({ ok: false });
    expect(changeAddressCheck(now, "https://u:t@github.com/a/b")).toMatchObject({ ok: false });
    expect(changeAddressCheck(now, "team/market")).toMatchObject({
      ok: true,
      value: "https://github.com/team/market",
    });
  });
  it("says what happened, naming installs that stayed as they were", () => {
    expect(addressChangedText("acme", [])).toMatch(/keep their versions and saved progress/);
    expect(addressChangedText("acme", ["com.acme.a", "com.acme.b"])).toMatch(
      /kept exactly as installed: com\.acme\.a, com\.acme\.b\.$/,
    );
  });
  it("sends the contract's argument names", async () => {
    invoke.mockReset();
    invoke.mockResolvedValue({ marketplace: {}, notFound: [] });
    await api.marketplaceChangeAddress("acme", "https://git.example.com/a/b");
    expect(invoke).toHaveBeenCalledWith("marketplace_change_address", {
      id: "acme",
      url: "https://git.example.com/a/b",
      confirmPrivate: false,
    });
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
    expect(savedStateLine(preview({ hasSavedState: true, savedState: "unknown" }))).toBe(
      "Saved progress from an earlier install of this id will be used.",
    );
  });

  it("asks only about progress from another source, and older hosts never ask", () => {
    expect(asksAboutSavedState(preview({ savedState: "foreign" }))).toBe(true);
    for (const s of ["none", "own", "unknown", undefined] as const) {
      expect(asksAboutSavedState(preview({ savedState: s })), String(s)).toBe(false);
    }
    // The question replaces the "will be used" line, which would be wrong for deleted state.
    expect(savedStateLine(preview({ hasSavedState: true, savedState: "foreign" }))).toBeNull();
  });

  it("warns before keeping progress from another source", () => {
    expect(FOREIGN_STATE_DELETE).toMatch(/recommended/);
    expect(FOREIGN_STATE_KEEP_WARNING).toMatch(/trust both sources/);
    expect(FOREIGN_STATE_TEXT).toMatch(/different marketplace or repository/);
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
    thirdPartyCode: false,
    ...over,
  });
  it("falls back to the id and truncates long names", () => {
    expect(entryTitle(entry())).toBe("a-b");
    expect(entryTitle(entry({ name: "n".repeat(200) })).length).toBe(80);
  });
  it("labels third-party code and still offers to review it (decision 27.5)", () => {
    const e = entry({ thirdPartyCode: true });
    expect(entryStatus(e).program).toBe(THIRD_PARTY_PROGRAM_LABEL);
    expect(canInstall(e)).toBe(true);
    expect(entryStatus(entry({ kind: "content" })).program).toBeNull();
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
    sourceNote: null,
    addressChanged: false,
    ...over,
  });
  it("flags a third-party program whose address changed since the user accepted it", () => {
    expect(addressChangedSinceConsent(item({ kind: "app", addressChanged: true }))).toBe(true);
    expect(addressChangedSinceConsent(item({ kind: "app", addressChanged: false }))).toBe(false);
    // Packs and built-ins never show it, and an older host sends no field at all.
    expect(addressChangedSinceConsent(item({ kind: "content", addressChanged: true }))).toBe(false);
    expect(
      addressChangedSinceConsent(item({ kind: "app", builtin: true, addressChanged: true })),
    ).toBe(false);
    expect(
      addressChangedSinceConsent({ ...item({ kind: "app" }), addressChanged: undefined } as never),
    ).toBe(false);
    expect(ADDRESS_CHANGED_SINCE_CONSENT).toBe("Address changed since you accepted");
  });
  it("offers update checks only for installed, untampered items", () => {
    expect(canCheckUpdate(item())).toBe(true);
    expect(canCheckUpdate(item({ builtin: true }))).toBe(false);
    expect(canCheckUpdate(item({ problem: "tampered" }))).toBe(false);
  });
  it("tells a file that couldn't be read apart from a damaged one", () => {
    expect(problemLabel("unreadable")).toMatch(/just now/);
    expect(problemText("unreadable")).toMatch(/try again/);
    expect(problemText("unreadable")).not.toMatch(/reinstall|Install it again/i);
    expect(problemLabel("tampered")).toMatch(/reinstall/);
    expect(problemText("missingEngine")).toMatch(/engine/);
  });
  it("describes the source", () => {
    expect(manageSubtitle(item())).toBe("Content pack · From friends · version 1.0.0");
    expect(manageSubtitle(item({ builtin: true, kind: "app" }))).toContain("Built in");
    expect(manageSubtitle(item({ builtin: true, kind: "app" }))).not.toContain("Third-party");
    expect(manageSubtitle(item({ kind: "app" }))).toBe(
      "Third-party program · From friends · version 1.0.0",
    );
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
  it("keeps a checked redirect target only on moved errors", () => {
    expect(
      api.toCommandError({
        kind: "moved",
        message: "m",
        moved: { url: "https://github.com/n/r", sameHost: true },
      }),
    ).toEqual({
      kind: "moved",
      message: "m",
      moved: { url: "https://github.com/n/r", sameHost: true },
    });
    // Anything that isn't https is dropped; so is `moved` on another kind.
    expect(
      api.toCommandError({ kind: "moved", message: "", moved: { url: "javascript:x" } }).moved,
    ).toBeUndefined();
    expect(
      api.toCommandError({ kind: "offline", message: "", moved: { url: "https://x.example/a" } })
        .moved,
    ).toBeUndefined();
    expect(errorText({ kind: "moved", message: "Moved to https://a.example/b." })).toBe(
      "Moved to https://a.example/b.",
    );
  });
  it("reads the Git backend and treats anything unexpected as unknown", async () => {
    invoke.mockResolvedValue({ backend: "builtin" });
    expect(await api.marketplaceGitBackend()).toBe("builtin");
    expect(invoke).toHaveBeenCalledWith("marketplace_git_backend", undefined);
    invoke.mockResolvedValue({ backend: "system" });
    expect(await api.marketplaceGitBackend()).toBe("system");
    invoke.mockResolvedValue({ backend: "<b>other</b>" });
    expect(await api.marketplaceGitBackend()).toBeNull();
    invoke.mockResolvedValue(null);
    expect(await api.marketplaceGitBackend()).toBeNull();
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
    await api.marketplaceRefresh("home", true);
    expect(invoke).toHaveBeenCalledWith("marketplace_refresh", {
      id: "home",
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
    await api.marketplaceInstall("t");
    expect(invoke).toHaveBeenCalledWith("marketplace_install", {
      token: "t",
      keepSavedState: false,
      acceptCodeRisk: false,
    });
    await api.marketplaceInstall("t", true);
    expect(invoke).toHaveBeenCalledWith("marketplace_install", {
      token: "t",
      keepSavedState: true,
      acceptCodeRisk: false,
    });
    await api.marketplaceInstall("t", false, true);
    expect(invoke).toHaveBeenCalledWith("marketplace_install", {
      token: "t",
      keepSavedState: false,
      acceptCodeRisk: true,
    });
    await api.marketplaceUpdate("u");
    expect(invoke).toHaveBeenCalledWith("marketplace_update", {
      token: "u",
      acceptCodeRisk: false,
    });
    await api.marketplaceUpdate("u", true);
    expect(invoke).toHaveBeenCalledWith("marketplace_update", {
      token: "u",
      acceptCodeRisk: true,
    });
  });
});

describe("third-party code warning and consent (owner decision 27.5)", () => {
  const code = (status: CodeRiskStatus, platform: CodeRisk["platform"] = "windows") =>
    preview({
      kind: "app",
      engine: null,
      thirdParty: true,
      codeRisk: { platform, status },
    });

  it("needs consent only for third-party code", () => {
    expect(needsCodeConsent(code("windowsNotReadBack"))).toBe(true);
    expect(needsCodeConsent(preview())).toBe(false);
    expect(needsCodeConsent(preview({ kind: "app", engine: null, thirdParty: false }))).toBe(false);
    // A host that forgot the note still gets the warning and the box.
    expect(needsCodeConsent(preview({ kind: "app", engine: null, codeRisk: null }))).toBe(true);
  });

  it("keeps Install disabled until the box is ticked", () => {
    expect(canConfirmReview(code("windowsNotReadBack"), false)).toBe(false);
    expect(canConfirmReview(code("windowsNotReadBack"), true)).toBe(true);
    expect(canConfirmReview(preview(), false)).toBe(true);
  });

  it("lists the sandbox, the missing access and the declared permissions", () => {
    const view = codeRiskView(code("unverified", "macos"));
    expect(view?.heading).toBe(CODE_RISK_HEADING);
    expect(view?.heading).toBe("This is someone else's program");
    expect(view?.consentLabel).toBe(CODE_RISK_CONSENT_LABEL);
    expect(view?.consentLabel).toBe("I understand and want to install this program");
    const points = view?.points.join(" ") ?? "";
    expect(points).toMatch(/inside Lullshift's sandbox/);
    expect(points).toMatch(/no access to your files, the network, your clipboard/);
    expect(points).toContain("It asks for: activity.storage.");
    expect(codeRiskView(code("unverified"))?.worstCase).toMatch(
      /send out what it shows you or what you type into it/,
    );
    const none = codeRiskView({ ...code("unverified"), permissions: [] });
    expect(none?.points).toContain("It asks for no permissions.");
    expect(codeRiskView(preview())).toBeNull();
  });

  it("says honestly what is known on each platform and never claims a guarantee", () => {
    const win = codeRiskVerification({ platform: "windows", status: "windowsNotReadBack" });
    expect(win).toMatch(/passed on a test computer/);
    expect(win).toMatch(/can't read those settings back/);
    const mac = codeRiskVerification({ platform: "macos", status: "unverified" });
    expect(mac).toMatch(/haven't been run on macOS/);
    expect(mac).toMatch(/not verified/);
    const linux = codeRiskVerification({ platform: "linux", status: "unverified" });
    expect(linux).toMatch(/haven't been run on Linux/);
    const failed = codeRiskVerification({ platform: "linux", status: "blockFailed" });
    expect(failed).toMatch(/couldn't turn off WebRTC on this computer/);
    expect(failed).toMatch(/may be able to send data out/);
    const unknown = codeRiskVerification({ platform: "windows", status: "notChecked" });
    expect(unknown).toMatch(/hasn't confirmed/);
    const all = [win, mac, linux, failed, unknown];
    expect(new Set(all).size).toBe(all.length);
    for (const text of all) {
      expect(text).not.toMatch(/\bguarantee[sd]?\b|\bsafe\b|\bsecure\b|\bproven\b/i);
    }
    // An unknown status from a newer host reads as "not confirmed", never as better news.
    expect(
      codeRiskVerification({ platform: "windows", status: "nope" as unknown as CodeRiskStatus }),
    ).toBe(unknown);
    const view = codeRiskView(code("blockFailed"));
    expect(view?.failed).toBe(true);
    expect(view?.noGuarantee).toMatch(/can't guarantee/);
    expect(codeRiskView(code("windowsNotReadBack"))?.failed).toBe(false);
  });
});

describe("where an activity came from", () => {
  it("labels third-party code, packs from a marketplace, and nothing for built-ins", () => {
    expect(activitySourceLabel({ marketplaceId: "friends", builtin: false }, null)).toBe(
      "Third-party program from friends",
    );
    expect(activitySourceLabel({ marketplaceId: "friends", builtin: false }, "quiz@1")).toBe(
      "From friends",
    );
    expect(activitySourceLabel({ marketplaceId: "lullshift", builtin: true }, null)).toBeNull();
    expect(activitySourceLabel(undefined, null)).toBeNull();
    expect(
      activitySourceLabel({ marketplaceId: "m".repeat(100), builtin: false }, null)?.length,
    ).toBeLessThan(80);
  });
});
