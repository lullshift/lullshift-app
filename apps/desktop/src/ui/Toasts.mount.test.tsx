// @vitest-environment jsdom
// The notice region on its own: timers, visibility, focus after dismissal, markup.

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ToastRegion } from "./Toasts";
import { pushToast, resetToasts, TOAST_MS } from "./toastQueue";

let host: HTMLDivElement;
let root: Root;

beforeEach(async () => {
  resetToasts();
  vi.useFakeTimers();
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  await act(async () => {
    root.render(
      <>
        <main id="main-content" tabIndex={-1}>
          <button type="button">page</button>
        </main>
        <ToastRegion />
      </>,
    );
  });
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
  vi.useRealTimers();
  vi.restoreAllMocks();
});

const toasts = () => host.querySelectorAll(".toast");
const input = (n: number) => ({ key: `k${n}`, title: `Notice ${n}`, tone: "info" as const });
const advance = (ms: number) =>
  act(async () => {
    vi.advanceTimersByTime(ms);
  });

function setVisibility(state: "visible" | "hidden") {
  vi.spyOn(document, "visibilityState", "get").mockReturnValue(state);
  return act(async () => {
    document.dispatchEvent(new Event("visibilitychange"));
  });
}

describe("auto-dismiss and the window", () => {
  it("does not run out while the window is hidden, and starts again when it is shown", async () => {
    await act(async () => pushToast(input(1)));
    await setVisibility("hidden");
    await advance(TOAST_MS * 3);
    expect(toasts()).toHaveLength(1);
    await setVisibility("visible");
    await advance(TOAST_MS - 100);
    expect(toasts()).toHaveLength(1);
    await advance(200);
    expect(toasts()).toHaveLength(0);
  });

  it("does not start counting for a notice that arrives while hidden", async () => {
    await setVisibility("hidden");
    await act(async () => pushToast(input(1)));
    await advance(TOAST_MS * 3);
    expect(toasts()).toHaveLength(1);
    await setVisibility("visible");
    await advance(TOAST_MS + 100);
    expect(toasts()).toHaveLength(0);
  });
});
