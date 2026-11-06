import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import type { CodeRiskStatus, InstallPreview } from "./api";
import { CodeRiskSection, ConfirmButton } from "./ReviewDialog";

const preview = (over: Partial<InstallPreview> = {}): InstallPreview => ({
  token: "t",
  marketplaceId: "friends",
  activityId: "com.friends.game",
  name: "Game",
  version: "1.0.0",
  description: "",
  sourceUrl: "https://example.com/friends/market",
  sourcePath: "packs/game",
  commit: "a".repeat(40),
  kind: "app",
  engine: null,
  permissions: ["activity.storage"],
  sizeBytes: 100,
  fileCount: 2,
  thirdParty: true,
  codeRisk: { platform: "windows", status: "windowsNotReadBack" },
  hasSavedState: false,
  ...over,
});

const section = (p: InstallPreview, consented: boolean) =>
  renderToStaticMarkup(
    <CodeRiskSection preview={p} consented={consented} disabled={false} onChange={() => {}} />,
  );

const checkbox = (html: string) => html.match(/<input[^>]*type="checkbox"[^>]*>/g) ?? [];

describe("third-party code warning (owner decision 27.5)", () => {
  it("is a labelled section with the heading, the facts and an unticked box", () => {
    const html = section(preview(), false);
    expect(html).toContain('aria-labelledby="review-code-risk-title"');
    expect(html).toMatch(
      /<h3 id="review-code-risk-title"[^>]*>This is someone else&#x27;s program/,
    );
    expect(html).toContain("inside Lullshift&#x27;s sandbox");
    expect(html).toContain("no access to your files, the network, your clipboard");
    expect(html).toContain("It asks for: activity.storage.");
    expect(html).toContain("send out what it shows you or what you type into it");
    const boxes = checkbox(html);
    expect(boxes).toHaveLength(1);
    expect(boxes[0]).not.toContain("checked");
    expect(boxes[0]).toContain('aria-describedby="review-code-risk-text"');
    expect(html).toContain('id="review-code-risk-text"');
    expect(html).toContain("I understand and want to install this program");
    // The box is inside its label, so clicking the text or pressing Space on it toggles it.
    expect(html).toMatch(/<label[^>]*><input[^>]*type="checkbox"[^>]*\/?><span>I understand/);
    expect(html).toContain('id="review-code-risk-hint"');
  });

  it("shows the ticked state and drops the hint once ticked", () => {
    const html = section(preview(), true);
    expect(checkbox(html)[0]).toContain("checked");
    expect(html).not.toContain('id="review-code-risk-hint"');
  });

  it.each<[CodeRiskStatus, "windows" | "macos" | "linux", RegExp]>([
    ["windowsNotReadBack", "windows", /can&#x27;t read those settings back/],
    ["unverified", "macos", /haven&#x27;t been run on macOS/],
    ["unverified", "linux", /haven&#x27;t been run on Linux/],
    ["blockFailed", "linux", /couldn&#x27;t turn off WebRTC on this computer/],
    ["notChecked", "windows", /hasn&#x27;t confirmed yet/],
  ])("states %s on %s", (status, platform, text) => {
    expect(section(preview({ codeRisk: { platform, status } }), false)).toMatch(text);
  });

  it("uses the danger tone (an alert) when the block failed", () => {
    expect(
      section(preview({ codeRisk: { platform: "linux", status: "blockFailed" } }), false),
    ).toContain('role="alert"');
    expect(section(preview(), false)).not.toContain('role="alert"');
  });

  it("renders nothing for content packs or built-in code", () => {
    expect(section(preview({ kind: "content", engine: "quiz@1", codeRisk: null }), false)).toBe("");
    expect(section(preview({ thirdParty: false, codeRisk: null }), false)).toBe("");
  });
});

describe("Install button", () => {
  const button = (blocked: boolean) =>
    renderToStaticMarkup(<ConfirmButton label="Install" blocked={blocked} onConfirm={() => {}} />);

  it("is disabled and says why until the box is ticked", () => {
    const html = button(true);
    expect(html).toMatch(/<button[^>]*disabled=""/);
    expect(html).toContain('aria-describedby="review-code-risk-hint"');
  });

  it("is enabled once ticked", () => {
    const html = button(false);
    expect(html).not.toContain("disabled");
    expect(html).not.toContain("aria-describedby");
    expect(html).toContain(">Install</button>");
  });
});
