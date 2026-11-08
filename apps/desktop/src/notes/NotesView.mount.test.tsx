// @vitest-environment jsdom
// Notes (ADR-026 section 7): grouping, search, filters, empty states, editing through the same
// commands as the session panel, and opening a session.

import { act, useRef } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

type Call = [command: string, args: Record<string, unknown>];
const calls: Call[] = [];
let overviewAnswer: unknown;

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args: Record<string, unknown> = {}) => {
    calls.push([command, args]);
    if (command === "notes_overview") {
      return overviewAnswer instanceof Error
        ? Promise.reject("store_unavailable")
        : Promise.resolve(structuredClone(overviewAnswer));
    }
    return Promise.resolve(null);
  },
  Channel: class {},
}));

import { buttonIn, click, polyfillDialog, settle, typeInto } from "../companion/testDom";
import type { NotesOverview, SessionItem } from "../context/api";
import type { SessionView } from "../sessions";
import NotesView from "./NotesView";
import { useNotesOverview } from "./useNotesOverview";

polyfillDialog();

const NOW = Date.parse("2026-11-05T12:30:00Z");
const item = (id: number, text: string, done = false): SessionItem => ({
  id,
  kind: "checklist",
  category: null,
  text,
  done,
  position: id,
  createdUtc: "2026-11-05T12:00:00Z",
});
const view = (id: string, state: SessionView["state"]): SessionView => ({
  provider: "claude-code",
  providerSessionId: id,
  state,
  reason: "observed",
  lastKnownState: state,
  lastSeenUtc: "2026-11-05T12:00:00Z",
  lastReceivedUtc: "2026-11-05T12:00:00Z",
  needsReview: false,
  userTitle: null,
  agentTitle: null,
  intentLine: null,
  disconnectedUtc: null,
});
const OVERVIEW: NotesOverview = {
  truncated: false,
  sessions: [
    {
      provider: "claude-code",
      sessionId: "aaaa1111",
      lastReceivedUtc: "2026-11-05T12:25:00Z",
      userTitle: "Token refresh",
      agentTitle: null,
      intent: "Refresh tokens quietly",
      notes: "Check the two-tab case",
      checklist: [item(1, "Refresh path has a test", true), item(2, "Sign-out clears the profile")],
    },
    {
      provider: "claude-code",
      sessionId: "bbbb2222",
      lastReceivedUtc: "2026-11-05T09:00:00Z",
      userTitle: "SQLite move",
      agentTitle: null,
      intent: null,
      notes: "Old JSON files stay as backup",
      checklist: [],
    },
    {
      provider: "claude-code",
      sessionId: "gone0000",
      lastReceivedUtc: "2026-11-01T09:00:00Z",
      userTitle: null,
      agentTitle: null,
      intent: null,
      notes: "",
      checklist: [item(9, "From a session no longer listed")],
    },
  ],
};
const SESSIONS = [view("aaaa1111", "needs_attention"), view("bbbb2222", "ended")];

let host: HTMLDivElement;
let root: Root;
let opened: { provider: string; sessionId: string }[] = [];

function Harness() {
  const heading = useRef<HTMLHeadingElement>(null);
  const { load, reload } = useNotesOverview(0);
  return (
    <NotesView
      headingRef={heading}
      load={load}
      reload={reload}
      sessions={SESSIONS}
      now={NOW}
      onOpenSession={(k) => opened.push(k)}
    />
  );
}

async function mount(answer: unknown = OVERVIEW) {
  overviewAnswer = answer;
  calls.length = 0;
  opened = [];
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  await act(async () => {
    root.render(<Harness />);
  });
  await settle();
}

afterEach(() => {
  act(() => root.unmount());
  host.remove();
});

const titles = () => [...host.querySelectorAll(".notes-group-title")].map((e) => e.textContent);
const search = () => host.querySelector<HTMLInputElement>('input[type="search"]')!;
const chip = (label: string) => buttonIn(host, label);

describe("Notes view", () => {
  beforeEach(() => mount());

  it("groups by session, newest first, with state, tool and time", () => {
    expect(titles()).toEqual(["Token refresh", "SQLite move", "Claude Code"]);
    const first = host.querySelector(".notes-group")!;
    expect(first.querySelector(".pill")?.textContent).toBe("Needs you");
    expect(first.textContent).toContain("Claude Code · aaaa1111");
    expect(first.textContent).toContain("5 min ago");
    expect(first.textContent).toContain("Before you answer");
    expect(first.querySelectorAll("li.item")).toHaveLength(2);
    expect(first.querySelector<HTMLTextAreaElement>("textarea")?.value).toBeDefined();
  });

  it("is honest about a session the dashboard no longer lists", () => {
    const last = host.querySelectorAll(".notes-group")[2]!;
    expect(last.querySelector(".pill")?.textContent).toBe("Unknown");
    expect(last.textContent).toContain("state is unknown");
    expect(buttonIn(last, "Open session: Claude Code").disabled).toBe(true);
  });

  it("Open session hands the key to the session flow", async () => {
    await click(buttonIn(host, "Open session: Token refresh"));
    expect(opened).toEqual([{ provider: "claude-code", sessionId: "aaaa1111" }]);
  });

  it("searches case-insensitively across notes, items and names", async () => {
    await typeInto(search(), "BACKUP");
    expect(titles()).toEqual(["SQLite move"]);
    // The live region waits for typing to pause.
    expect(host.querySelector(".notes-count")?.textContent).not.toBe("1 of 3 sessions");
    await act(async () => {
      await new Promise((r) => setTimeout(r, 700));
    });
    expect(host.querySelector(".notes-count")?.textContent).toBe("1 of 3 sessions");
    await typeInto(search(), "profile");
    expect(titles()).toEqual(["Token refresh"]);
    expect(host.querySelectorAll("li.item")).toHaveLength(1);
    await typeInto(search(), "sqlite");
    expect(titles()).toEqual(["SQLite move"]);
  });

  it("filter chips narrow what is shown and say which is pressed", async () => {
    await click(chip("Done items"));
    expect(chip("Done items").getAttribute("aria-pressed")).toBe("true");
    expect(chip("All").getAttribute("aria-pressed")).toBe("false");
    expect(titles()).toEqual(["Token refresh"]);
    expect(host.querySelectorAll("li.item")).toHaveLength(1);
    await click(chip("Open items"));
    expect(titles()).toEqual(["Token refresh", "Claude Code"]);
    await click(chip("Notes"));
    expect(titles()).toEqual(["Token refresh", "SQLite move"]);
    expect(host.querySelector("li.item")).toBeNull();
    await click(chip("Intents"));
    expect(titles()).toEqual(["Token refresh"]);
  });

  it("the state filter keeps only that state", async () => {
    await click(chip("Ended"));
    expect(titles()).toEqual(["SQLite move"]);
    await click(chip("Working"));
    expect(titles()).toEqual([]);
  });

  it("no matches offers a way back", async () => {
    await typeInto(search(), "zebra");
    expect(host.textContent).toContain("No matches");
    await click(buttonIn(host, "Clear search and filters"));
    expect(titles()).toHaveLength(3);
    expect(search().value).toBe("");
  });

  it("the checkbox toggles through the same command as the panel", async () => {
    const box = host.querySelector<HTMLInputElement>(
      'input[aria-label="Done: Sign-out clears the profile"]',
    )!;
    await click(box);
    expect(calls).toContainEqual(["update_session_item", { id: 2, text: null, done: true }]);
    // And the view re-reads Rust afterwards.
    expect(calls.filter(([c]) => c === "notes_overview").length).toBeGreaterThan(1);
  });

  it("an item edits in place and saves with Enter", async () => {
    const text = host.querySelector<HTMLButtonElement>(".notes-group .item-text")!;
    await click(text);
    const input = host.querySelector<HTMLInputElement>("input.item-edit")!;
    await typeInto(input, "Refresh path is tested");
    await act(async () => {
      input.dispatchEvent(
        new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }),
      );
    });
    await settle();
    expect(calls).toContainEqual([
      "update_session_item",
      { id: 1, text: "Refresh path is tested", done: null },
    ]);
  });

  it("deleting an item asks first, then uses the delete command", async () => {
    await click(
      host.querySelector<HTMLButtonElement>('[aria-label="Delete Sign-out clears the profile"]')!,
    );
    expect(calls.some(([c]) => c === "delete_session_item")).toBe(false);
    await click(buttonIn(host.querySelector(".notes-group dialog")!, "Delete"));
    expect(calls).toContainEqual(["delete_session_item", { id: 2 }]);
  });

  it("notes edit inline and save when the box loses focus", async () => {
    const notes = [...host.querySelectorAll<HTMLTextAreaElement>("textarea")].find(
      (t) => t.value === "Check the two-tab case",
    )!;
    await act(async () => notes.focus());
    await typeInto(notes, "Check the two-tab case, then ship");
    await act(async () => notes.blur());
    await settle();
    expect(calls).toContainEqual([
      "set_session_notes",
      {
        provider: "claude-code",
        sessionId: "aaaa1111",
        notes: "Check the two-tab case, then ship",
      },
    ]);
  });

  it("the intent edits through set_session_intent", async () => {
    const intent = [...host.querySelectorAll<HTMLTextAreaElement>("textarea")].find(
      (t) => t.value === "Refresh tokens quietly",
    )!;
    await act(async () => intent.focus());
    await typeInto(intent, "Refresh tokens quietly and sign out cleanly");
    await act(async () => intent.blur());
    await settle();
    expect(calls).toContainEqual([
      "set_session_intent",
      {
        provider: "claude-code",
        sessionId: "aaaa1111",
        intent: "Refresh tokens quietly and sign out cleanly",
      },
    ]);
  });
});

describe("empty and error states", () => {
  it("with no notes at all, says where notes come from", async () => {
    await mount({ sessions: [], truncated: false });
    expect(host.textContent).toContain("Nothing noted yet");
    expect(host.textContent).toContain("session panel");
    expect(host.querySelector(".mascot")).not.toBeNull();
    expect(host.querySelector('input[type="search"]')).toBeNull();
  });

  it("a failed read says so and offers Try again; nothing is invented", async () => {
    await mount(new Error("x"));
    expect(host.textContent).toContain("Couldn't read your notes");
    expect(host.textContent).not.toContain("Nothing noted yet");
    expect(buttonIn(host, "Try again")).toBeTruthy();
  });

  it("says when older sessions were left out by the caps", async () => {
    await mount({ ...OVERVIEW, truncated: true });
    expect(host.textContent).toContain("most recent sessions");
  });
});
