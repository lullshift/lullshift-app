import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";

// Status never resolves, as when the core is unavailable: the button must still be offered.
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(() => new Promise(() => undefined)) }));

import { Onboarding } from "./Onboarding";

describe("welcome dialog", () => {
  const html = renderToStaticMarkup(
    <Onboarding
      open
      onDone={() => undefined}
      onConnect={() => undefined}
      onTryActivity={() => undefined}
    />,
  );
  it("offers Connect an agent while agent status is still unknown", () => {
    expect(html).toContain("Connect an agent");
    expect(html).not.toContain("Set up Claude Code");
  });
  it("puts Skip first among the buttons", () => {
    expect(html.indexOf("Skip for now")).toBeGreaterThan(-1);
    expect(html.indexOf("Skip for now")).toBeLessThan(html.indexOf("Connect an agent"));
  });
});
