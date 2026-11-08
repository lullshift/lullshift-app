// @vitest-environment jsdom
// The panel header's buttons (Rename, Pin/Pinned) share one size class in every state.

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: () => Promise.reject("not_found"),
  Channel: class {},
}));

import { polyfillDialog, settle } from "../companion/testDom";
import type { SessionView } from "../sessions";
import DetailPanel from "./DetailPanel";

polyfillDialog();

const session: SessionView = {
  provider: "claude-code",
  providerSessionId: "abc12345",
  state: "needs_attention",
  reason: "observed",
  lastKnownState: "needs_attention",
  lastSeenUtc: "2026-11-05T12:00:00Z",
  lastReceivedUtc: "2026-11-05T12:00:00Z",
  needsReview: false,
  userTitle: null,
  agentTitle: null,
  intentLine: null,
  disconnectedUtc: null,
};

let host: HTMLDivElement;
let root: Root;

async function render(pinned: boolean) {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  await act(async () => {
    root.render(
      <DetailPanel
        session={session}
        sessions={[session]}
        now={Date.parse("2026-11-05T12:05:00Z")}
        staleAfterSeconds={600}
        tick={0}
        pinned={pinned}
        floating={!pinned}
        onTogglePin={() => undefined}
        onClose={() => undefined}
        onOpenSession={() => undefined}
        onOpenSettings={() => undefined}
        onChanged={() => undefined}
        onDeleted={() => undefined}
      />,
    );
  });
  await settle();
}

afterEach(() => {
  act(() => root.unmount());
  host.remove();
});

describe("panel header buttons", () => {
  for (const pinned of [false, true]) {
    it(`Rename and ${pinned ? "Pinned" : "Pin"} use the same size class`, async () => {
      await render(pinned);
      const buttons = [...host.querySelectorAll<HTMLButtonElement>(".detail-head button")];
      const rename = buttons.find((b) => b.textContent?.startsWith("Rename"))!;
      const pin = buttons.find((b) => b.getAttribute("aria-pressed") !== null)!;
      expect(pin.textContent).toBe(pinned ? "Pinned" : "Pin");
      expect(rename.classList.contains("head-btn")).toBe(true);
      expect(pin.classList.contains("head-btn")).toBe(true);
      // Neither carries a competing size class.
      expect(rename.classList.contains("btn-mini")).toBe(false);
    });
  }
});
