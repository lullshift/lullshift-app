// @vitest-environment jsdom
// The Conversation options context picker (ADR-020 amendment): a second modal with a local search
// and several ticked sessions, chips, notes grouped per session, and a layout that never scrolls
// sideways. jsdom does no layout, so the overflow check is at class/structure level, and the CSS
// rules that stop the overflow are checked in the stylesheet text.

import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { act, useEffect, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

type Call = [command: string, args: unknown];
const calls: Call[] = [];

const detailFor = (id: string) => ({
  context: {
    intent: null,
    latestRequest: null,
    ask: null,
    notes: id === "s1" ? "my notes" : "",
    reviewedUtc: null,
    hasCaptured: false,
    userTitle: null,
    agentTitle: null,
  },
  items: [
    {
      id: id === "s1" ? 11 : 21,
      kind: "checklist",
      category: null,
      text: `Item of ${id}`,
      done: false,
      position: 0,
      createdUtc: "2026-11-07T10:00:00Z",
    },
  ],
  links: [],
  timeline: [],
  retentionDays: 30,
});

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args: Record<string, unknown> = {}) => {
    calls.push([command, args]);
    if (command === "session_detail") return Promise.resolve(detailFor(String(args.sessionId)));
    return Promise.resolve(null);
  },
  Channel: class {},
}));

import type { SessionView } from "../sessions";
import { Dialog } from "../ui/Dialog";
import { ContextPicker } from "./ContextPicker";
import { buildScope, EMPTY_SCOPE, MAX_SCOPE_SESSIONS, type ScopeDraft } from "./logic";
import { click, polyfillDialog, settle, typeInto } from "./testDom";

polyfillDialog();

function session(id: string, more: Partial<SessionView> = {}): SessionView {
  return {
    provider: "claude-code",
    providerSessionId: id,
    state: "working",
    reason: null,
    lastKnownState: "working",
    lastSeenUtc: "2026-11-07T10:00:00Z",
    lastReceivedUtc: "2026-11-07T10:00:00Z",
    needsReview: false,
    userTitle: `Task ${id}`,
    agentTitle: null,
    intentLine: null,
    disconnectedUtc: null,
    ...more,
  } as SessionView;
}

const SESSIONS: SessionView[] = [
  session("s1", { userTitle: "Fix the Login Flow" }),
  session("s2", { userTitle: "Dark mode", provider: "cursor" }),
  session("s3", { userTitle: "Refactor billing" }),
  session("s4", { userTitle: "Docs pass" }),
  session("s5", { userTitle: "Release notes" }),
  session("s6", { userTitle: "Spike: search" }),
];

let host: HTMLDivElement;
let root: Root;
let current: ScopeDraft = EMPTY_SCOPE;

function Harness({ sessions = SESSIONS }: { sessions?: SessionView[] }) {
  const [draft, setDraft] = useState<ScopeDraft>(EMPTY_SCOPE);
  useEffect(() => {
    current = draft;
  });
  return <ContextPicker draft={draft} onChange={setDraft} sessions={sessions} disabled={false} />;
}

beforeEach(async () => {
  calls.length = 0;
  current = EMPTY_SCOPE;
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  await act(async () => root.render(<Harness />));
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
});

const radio = (label: string) => {
  const l = [...host.querySelectorAll("label.scope-option")].find((x) =>
    x.textContent?.startsWith(label),
  );
  return l!.querySelector("input")!;
};
const pickerDialog = () =>
  [...host.querySelectorAll("dialog")].find(
    (d) =>
      d.querySelector(":scope > .dialog-body > .dialog-title")?.textContent === "Choose sessions",
  ) as HTMLDialogElement;
const btn = (root: ParentNode, label: string) =>
  [...root.querySelectorAll("button")].find(
    (b) => b.textContent?.trim() === label || b.getAttribute("aria-label") === label,
  )!;
const rows = () => [...pickerDialog().querySelectorAll(".session-pick-list label.pick")];
const chips = () => [...host.querySelectorAll(".session-chip-text")].map((e) => e.textContent);

describe("session picker modal", () => {
  it("offers no native select and opens the second modal for a session scope", async () => {
    expect(host.querySelector("select")).toBeNull();
    expect(host.querySelector(".scope-session")).toBeNull();
    await click(radio("Session summary"));
    expect(host.querySelector("select")).toBeNull();
    expect(pickerDialog().hasAttribute("open")).toBe(false);
    await click(btn(host, "Choose sessions…"));
    expect(pickerDialog().hasAttribute("open")).toBe(true);
    expect(rows()).toHaveLength(SESSIONS.length);
  });

  it("filters locally and case-insensitively by title, tool and ID, sending nothing", async () => {
    await click(radio("Session summary"));
    await click(btn(host, "Choose sessions…"));
    const search = pickerDialog().querySelector<HTMLInputElement>("input[type=search]")!;
    await typeInto(search, "LOGIN");
    expect(rows().map((r) => r.textContent)).toEqual([expect.stringContaining("Fix the Login")]);
    await typeInto(search, "cursor");
    expect(rows()).toHaveLength(1);
    expect(rows()[0]?.textContent).toContain("Dark mode");
    await typeInto(search, "s3");
    expect(rows()[0]?.textContent).toContain("Refactor billing");
    await typeInto(search, "nothing like this");
    expect(rows()).toHaveLength(0);
    expect(pickerDialog().textContent).toContain("No session matches");
    expect(calls.filter(([c]) => c !== "session_detail")).toEqual([]);
  });

  it("ticks several sessions, shows them as removable chips, and Cancel changes nothing", async () => {
    await click(radio("Session summary"));
    await click(btn(host, "Choose sessions…"));
    await click(rows()[0]!.querySelector("input")!);
    await click(rows()[1]!.querySelector("input")!);
    await click(btn(pickerDialog(), "Cancel"));
    expect(current.sessions).toHaveLength(0);
    expect(chips()).toEqual([]);

    await click(btn(host, "Choose sessions…"));
    await click(rows()[0]!.querySelector("input")!);
    await click(rows()[1]!.querySelector("input")!);
    await click(btn(pickerDialog(), "Use these sessions"));
    expect(pickerDialog().hasAttribute("open")).toBe(false);
    expect(chips()).toHaveLength(2);
    expect(chips()[0]).toContain("Fix the Login Flow");
    expect(buildScope(current)).toEqual({
      kind: "sessionSummary",
      sessions: [
        { provider: "claude-code", sessionId: "s1" },
        { provider: "cursor", sessionId: "s2" },
      ],
    });
    // Remove one chip.
    await click(btn(host, "Remove Fix the Login Flow (Claude Code)"));
    expect(chips()).toHaveLength(1);
    expect(current.sessions.map((s) => s.sessionId)).toEqual(["s2"]);
  });

  it("stops at the session cap", async () => {
    await click(radio("Session summary"));
    await click(btn(host, "Choose sessions…"));
    for (let i = 0; i < MAX_SCOPE_SESSIONS; i++) {
      await click(rows()[i]!.querySelector("input")!);
    }
    const last = rows()[MAX_SCOPE_SESSIONS]!.querySelector("input")!;
    expect(last.disabled).toBe(true);
    expect(pickerDialog().textContent).toContain(`${MAX_SCOPE_SESSIONS} of ${MAX_SCOPE_SESSIONS}`);
    // Unticking one frees a place.
    await click(rows()[0]!.querySelector("input")!);
    expect(rows()[MAX_SCOPE_SESSIONS]!.querySelector("input")!.disabled).toBe(false);
  });

  it("groups notes per chosen session for Selected notes", async () => {
    await click(radio("Selected notes"));
    await click(btn(host, "Choose sessions…"));
    await click(rows()[0]!.querySelector("input")!);
    await click(rows()[1]!.querySelector("input")!);
    await click(btn(pickerDialog(), "Use these sessions"));
    await settle();
    const groups = [...host.querySelectorAll(".notes-group")];
    expect(groups).toHaveLength(2);
    expect(groups[0]?.textContent).toContain("Fix the Login Flow");
    expect(groups[1]?.textContent).toContain("Dark mode");
    // Nothing is shareable until something is ticked from each session.
    expect(buildScope(current)).toBeNull();
    const tick = async (g: Element, text: string) => {
      const l = [...g.querySelectorAll("label.pick")].find((x) => x.textContent?.includes(text));
      await click(l!.querySelector("input")!);
    };
    await tick(groups[0]!, "Your notes");
    expect(buildScope(current)).toBeNull();
    await tick(groups[1]!, "Item of s2");
    expect(buildScope(current)).toEqual({
      kind: "selectedNotes",
      sessions: [
        { provider: "claude-code", sessionId: "s1", notes: true, itemIds: [] },
        { provider: "cursor", sessionId: "s2", notes: false, itemIds: [21] },
      ],
    });
    // Removing a chip drops that session's ticks.
    await click(btn(host, "Remove Dark mode (Cursor)"));
    expect(current.sessions.map((s) => s.sessionId)).toEqual(["s1"]);
  });
});

describe("no horizontal overflow", () => {
  it("has no native select, fields fill their container and nothing is sized to its content", async () => {
    await click(radio("Selected notes"));
    await click(btn(host, "Choose sessions…"));
    await click(rows()[0]!.querySelector("input")!);
    await click(btn(pickerDialog(), "Use these sessions"));
    expect(host.querySelector("select")).toBeNull();
    const picker = host.querySelector<HTMLElement>(".scope-picker")!;
    expect(picker.tagName).toBe("FIELDSET");
    expect(host.querySelector(".scope-session")).not.toBeNull();
    expect(host.querySelector(".session-chips")).not.toBeNull();
    expect(host.querySelector(".pick-list")).not.toBeNull();
  });

  it("the stylesheet lets fieldsets, grid children, fields and chips shrink", () => {
    const css = readFileSync(resolve(process.cwd(), "src/companion/companion.css"), "utf8");
    const block = (selector: string) => {
      const i = css.lastIndexOf(selector);
      expect(i, selector).toBeGreaterThan(-1);
      return css.slice(i, css.indexOf("}", i));
    };
    expect(block(".scope-picker,\n.pick-list {")).toMatch(/min-inline-size:\s*0/);
    expect(block(".scope-picker,\n.pick-list {")).toMatch(/min-width:\s*0/);
    expect(block(".scope-picker .field,")).toMatch(/width:\s*100%/);
    expect(block(".scope-options > *,")).toMatch(/min-width:\s*0/);
    expect(block(".session-chip-text {")).toMatch(/text-overflow:\s*ellipsis/);
    expect(block(".dialog:has(.scope-picker),")).toMatch(/overflow-x:\s*hidden/);
  });
});

describe("nested in the options dialog", () => {
  function Nested() {
    const [open, setOpen] = useState(true);
    const [draft, setDraft] = useState<ScopeDraft>({ ...EMPTY_SCOPE, kind: "sessionSummary" });
    return (
      <Dialog
        open={open}
        title="Conversation options"
        onClose={() => setOpen(false)}
        actions={<button type="button">Done</button>}
      >
        <ContextPicker draft={draft} onChange={setDraft} sessions={SESSIONS} disabled={false} />
      </Dialog>
    );
  }

  it("Esc on the session picker closes only it, and focus returns to Choose sessions", async () => {
    await act(async () => root.render(<Nested />));
    const outer = [...host.querySelectorAll("dialog")].find((d) =>
      d.textContent?.includes("Conversation options"),
    ) as HTMLDialogElement;
    expect(outer.hasAttribute("open")).toBe(true);
    await click(btn(outer, "Choose sessions…"));
    const inner = pickerDialog();
    expect(inner.hasAttribute("open")).toBe(true);
    // Worst case: the cancel event reaches the outer dialog as well.
    await act(async () => {
      inner.dispatchEvent(new Event("cancel", { bubbles: true, cancelable: true }));
    });
    await settle();
    expect(inner.hasAttribute("open")).toBe(false);
    expect(outer.hasAttribute("open")).toBe(true);
    expect(document.activeElement?.textContent).toBe("Choose sessions…");
  });
});
