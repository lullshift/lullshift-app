// @vitest-environment jsdom
// Mounted with the Tauri bridge mocked: proves the dialog reads logs only when opened, imports
// only what was ticked, and that Cancel imports nothing (decision 34).

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

type Call = [command: string, args: unknown];
const calls: Call[] = [];
let answer: (command: string, args: Record<string, unknown>) => unknown = () => null;

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args: Record<string, unknown> = {}) => {
    calls.push([command, args]);
    try {
      return Promise.resolve(answer(command, args));
    } catch (err) {
      return Promise.reject(err instanceof Error ? err : (err as string));
    }
  },
  Channel: class {},
}));

import { ImportSessionsDialog } from "./ImportSessionsDialog";
import type { FoundSession, ScanResult } from "./importSessions";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// jsdom has no <dialog> modal API.
beforeEach(() => {
  HTMLDialogElement.prototype.showModal ??= function (this: HTMLDialogElement) {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close ??= function (this: HTMLDialogElement) {
    this.removeAttribute("open");
  };
});

const NOW = Date.parse("2026-11-07T12:00:00Z");
const row = (id: string, more: Partial<FoundSession> = {}): FoundSession => ({
  provider: "claude-code",
  sessionId: id,
  project: "shop/api",
  projectPath: "/home/u/shop/api",
  title: `Task ${id}`,
  lastActivityUtc: "2026-11-07T11:00:00Z",
  guess: "waiting",
  tracked: false,
  ...more,
});
const SCAN: ScanResult = {
  sessions: [
    row("a1", { lastActivityUtc: "2026-11-07T11:50:00Z" }),
    row("b2", { provider: "cursor", title: "Dark mode" }),
    row("c3", { tracked: true, lastActivityUtc: "2026-11-07T09:00:00Z" }),
  ],
  agents: [
    { provider: "claude-code", state: "read", sessions: 2, incomplete: false },
    { provider: "cursor", state: "read", sessions: 1, incomplete: false },
    { provider: "codex", state: "not_found", sessions: 0, incomplete: false },
    { provider: "opencode", state: "not_found", sessions: 0, incomplete: false },
  ],
  incomplete: true,
};

let host: HTMLDivElement;
let root: Root;
let closed = 0;
let imported: string[] = [];

beforeEach(() => {
  calls.length = 0;
  closed = 0;
  imported = [];
  answer = (cmd) =>
    cmd === "import_scan" ? SCAN : { imported: 1, alreadyTracked: 0, notFound: 0 };
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
});

async function render(open: boolean) {
  await act(async () => {
    root.render(
      <ImportSessionsDialog
        open={open}
        now={NOW}
        onClose={() => (closed += 1)}
        onImported={(m) => imported.push(m)}
      />,
    );
    await Promise.resolve();
  });
}

const boxes = () => [...host.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')];
const button = (text: string) =>
  [...host.querySelectorAll("button")].find((b) => b.textContent?.startsWith(text))!;
const click = async (el: HTMLElement) => {
  await act(async () => {
    el.click();
    await Promise.resolve();
  });
};

describe("Import sessions dialog", () => {
  it("reads nothing until it opens", async () => {
    await render(false);
    expect(calls).toEqual([]);
    await render(true);
    expect(calls.map(([c]) => c)).toEqual(["import_scan"]);
  });

  it("lists newest first, marks tracked rows and says the list may be incomplete", async () => {
    await render(true);
    const labels = boxes().map((b) => b.getAttribute("aria-label") ?? "");
    expect(labels[0]).toContain("Task a1");
    expect(labels[2]).toContain("already in Lullshift");
    expect(boxes()[2]!.disabled).toBe(true);
    expect(host.textContent).toContain("may be incomplete");
    expect(host.textContent).toContain("No Codex logs found");
    expect(host.querySelector('[aria-live="polite"]')?.textContent).toBe(
      "3 sessions found, 0 selected.",
    );
    expect(button("Import (0)").disabled).toBe(true);
  });

  it("imports only the ticked sessions", async () => {
    await render(true);
    await click(boxes()[1]!);
    expect(button("Import (1)").disabled).toBe(false);
    await click(button("Import (1)"));
    expect(calls.at(-1)).toEqual([
      "import_sessions",
      { picks: [{ provider: "cursor", sessionId: "b2" }] },
    ]);
    expect(imported).toEqual(["Imported 1 session."]);
    expect(closed).toBe(1);
  });

  it("select all shown follows the filter and skips tracked rows", async () => {
    await render(true);
    const filter = host.querySelector<HTMLInputElement>('input[type="search"]')!;
    await act(async () => {
      const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
      set.call(filter, "Task");
      filter.dispatchEvent(new Event("input", { bubbles: true }));
      await Promise.resolve();
    });
    expect(boxes()).toHaveLength(2);
    await click(button("Select all shown"));
    expect(button("Import (1)")).toBeDefined();
    expect(host.querySelector('[aria-live="polite"]')?.textContent).toBe(
      "2 of 3 shown, 1 selected.",
    );
  });

  it("Cancel imports nothing", async () => {
    await render(true);
    await click(boxes()[0]!);
    await click(button("Cancel"));
    expect(closed).toBe(1);
    expect(calls.some(([c]) => c === "import_sessions")).toBe(false);
  });

  it("shows an honest error with a retry", async () => {
    answer = () => {
      throw "internal";
    };
    await render(true);
    expect(host.textContent).toContain("couldn't be read");
    answer = () => SCAN;
    await click(button("Try again"));
    expect(boxes()).toHaveLength(3);
    expect(document.activeElement?.getAttribute("role")).toBe("status");
  });

  it("closing while the logs are being read stops the scan", async () => {
    let finish: (v: ScanResult) => void = () => undefined;
    answer = (cmd) => (cmd === "import_scan" ? new Promise<ScanResult>((r) => (finish = r)) : null);
    await render(true);
    await click(button("Cancel"));
    expect(closed).toBe(1);
    expect(calls.map(([c]) => c)).toEqual(["import_scan", "import_cancel"]);
    await act(async () => {
      finish(SCAN);
      await Promise.resolve();
    });
  });

  it("an empty scan says so", async () => {
    answer = () => ({ sessions: [], agents: [], incomplete: false });
    await render(true);
    expect(host.textContent).toContain("No sessions found");
  });
});
