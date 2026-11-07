// @vitest-environment jsdom
// The notice region on its own: timers, visibility, focus after dismissal, markup.

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../companion/testDom"; // turns on React's act environment
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

describe("focus after a notice closes", () => {
  const dismissButtons = () => [...host.querySelectorAll<HTMLButtonElement>(".toast-dismiss")];
  const escape = (el: Element) =>
    act(async () => {
      el.dispatchEvent(
        new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }),
      );
    });

  it("moves focus to the next notice's button when Escape closes the one that has it", async () => {
    await act(async () => {
      pushToast(input(1));
      pushToast(input(2));
    });
    const first = host.querySelector<HTMLButtonElement>(".toast button")!;
    first.focus();
    await escape(first);
    expect(toasts()).toHaveLength(1);
    expect(document.activeElement).toBe(dismissButtons()[0]);
    expect(host.querySelector(".toast-title")?.textContent).toBe("Notice 2");
  });

  it("moves focus to the main area when the last notice closes", async () => {
    await act(async () => pushToast(input(1)));
    const button = dismissButtons()[0]!;
    button.focus();
    await act(async () => button.click());
    expect(toasts()).toHaveLength(0);
    expect(document.activeElement).toBe(document.getElementById("main-content"));
  });

  it("does not move focus when the notice did not hold it, and Escape stops there", async () => {
    await act(async () => pushToast(input(1)));
    const page = host.querySelector<HTMLButtonElement>("main button")!;
    page.focus();
    const outer = vi.fn();
    document.addEventListener("keydown", outer);
    await escape(dismissButtons()[0]!);
    document.removeEventListener("keydown", outer);
    expect(toasts()).toHaveLength(0);
    expect(document.activeElement).toBe(page);
    expect(outer).not.toHaveBeenCalled();
  });
});

describe("a user-chosen name in a notice", () => {
  it("is isolated in <bdi> so it can't reorder the words around it", async () => {
    await act(async () =>
      pushToast({ ...input(1), title: "Companion replied in “gnp.exe”", subject: "gnp.exe" }),
    );
    const title = host.querySelector(".toast-title")!;
    expect(title.querySelector("bdi")?.textContent).toBe("gnp.exe");
    expect(title.textContent).toBe("Companion replied in “gnp.exe”");
  });
});

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
