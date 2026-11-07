import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(() => new Promise(() => undefined)) }));

import { ConnectionProblemsView } from "./ConnectionProblems";
import {
  failureText,
  fixErrorText,
  fixExplanation,
  fixResultText,
  packagedWarning,
  scanNote,
  staleDetail,
  staleFor,
  staleHeadline,
  type ConnectionHealth,
  type HookFailure,
} from "./connection";

const CLAUDE = { family: "Claude_pzs8sxrjxfjjc", name: "Claude" };
const at = () => "13:05";

describe("packaged-app wording (decision 33)", () => {
  it("names the app and says how to start Lullshift instead", () => {
    const w = packagedWarning(CLAUDE);
    expect(w).toContain("started from inside another app (Claude)");
    expect(w).toMatch(/agents may not reach it/);
    expect(w).toMatch(/Start menu or File Explorer/);
  });

  it("explains stale copies without their contents", () => {
    expect(staleHeadline({ package: CLAUDE, files: ["endpoint", "token"] })).toBe(
      "Agents running inside Claude can't reach Lullshift: that app has out-of-date copies of Lullshift's connection files.",
    );
    expect(staleDetail(["endpoint"])).toMatch(/points to an old address/);
    expect(staleDetail(["token"])).toMatch(/access key is out of date/);
    expect(staleDetail(["endpoint", "token"])).toMatch(/old address and its access key/);
    expect(staleDetail([])).toBe("");
  });

  it("says what Fix leaves alone and that open sessions may need a restart", () => {
    const t = fixExplanation(CLAUDE);
    expect(t).toMatch(/deletes only those out-of-date copies/);
    expect(t).toMatch(/restart it/);
    expect(t).toMatch(/copy of its database, stays where it is/);
  });

  it("describes a failed hook with time, reason and likely cause", () => {
    const f: HookFailure = {
      adapter: "claude-code",
      reason: "rejected",
      utc: "2026-11-07T13:05:00Z",
      package: CLAUDE,
    };
    const t = failureText(f, at);
    expect(t).toMatch(/^Claude Code tried to reach Lullshift at 13:05 but couldn't \(refused\)\./);
    expect(t).toMatch(/out-of-date access key/);
    expect(t).toMatch(/inside Claude/);
    const u = failureText({ ...f, reason: "unreachable", package: null, adapter: "codex" }, at);
    expect(u).toMatch(/^Codex tried .* \(unreachable\)\./);
    expect(u).toMatch(/Start menu or File Explorer/);
    expect(failureText({ ...f, adapter: "nope" }, at)).toMatch(/^An agent tried/);
  });

  it("reports Fix results honestly", () => {
    expect(fixResultText(CLAUDE, { deleted: ["token"], current: [], failed: [] }).tone).toBe(
      "success",
    );
    const partial = fixResultText(CLAUDE, {
      deleted: ["endpoint"],
      current: [],
      failed: ["token"],
    });
    expect(partial.tone).toBe("danger");
    expect(partial.text).toMatch(/Some were removed/);
    expect(fixResultText(CLAUDE, { deleted: [], current: ["token"], failed: [] }).text).toMatch(
      /already up to date/,
    );
    expect(fixErrorText("packaged")).toMatch(/running inside another app/);
    expect(fixErrorText("weird")).toMatch(/Nothing was removed/);
  });

  it("notes a partial or failed scan, and nothing when complete", () => {
    expect(scanNote("partial")).toMatch(/only some apps/);
    expect(scanNote("unavailable")).toMatch(/couldn't check/);
    expect(scanNote("complete")).toBeNull();
    expect(scanNote("skipped")).toBeNull();
  });
});

describe("connection problems view", () => {
  const health: ConnectionHealth = {
    package: null,
    scan: "complete",
    stale: [{ package: CLAUDE, files: ["endpoint", "token"] }],
    failures: [
      { adapter: "claude-code", reason: "rejected", utc: "2026-11-07T13:05:00Z", package: CLAUDE },
      { adapter: "codex", reason: "unreachable", utc: "2026-11-07T13:04:00Z", package: null },
    ],
  };

  it("offers one Fix per package and puts its failure note in the same card", () => {
    const html = renderToStaticMarkup(
      <ConnectionProblemsView health={health} busy={null} onFix={() => undefined} />,
    );
    expect(html.match(/>Fix</g)).toHaveLength(1);
    expect(html).toContain("Agents running inside Claude can&#x27;t reach Lullshift");
    expect(html).toContain("Claude Code tried to reach Lullshift");
    expect(html).toContain("Codex tried to reach Lullshift");
    expect(staleFor(health, CLAUDE)).not.toBeNull();
    expect(staleFor(health, null)).toBeNull();
  });

  it("shows nothing when there is nothing to fix", () => {
    const html = renderToStaticMarkup(
      <ConnectionProblemsView
        health={{ package: null, scan: "complete", stale: [], failures: [] }}
        busy={null}
        onFix={() => undefined}
      />,
    );
    expect(html).toBe("");
  });

  it("disables Fix while one is running", () => {
    const html = renderToStaticMarkup(
      <ConnectionProblemsView health={health} busy={CLAUDE.family} onFix={() => undefined} />,
    );
    expect(html).toContain("Fixing…");
    expect(html).toContain("disabled");
  });
});
