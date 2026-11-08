// @vitest-environment jsdom
// Activities with model-written sets (ADR-028 section 8): the Fresh set action appears on packs
// of the eight engines only while the switches allow it (body engines need the second switch),
// and each engine's saved sets sit in a "Made by your model" group with the AI-generated badge
// and the model name. Sets launch like any activity; they never join the normal list.

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ActivityInfo } from "./ActivityHost";

let prefs: unknown = { toolsEnabled: false, lastScope: null };
let sets: unknown = [];
const started: string[] = [];

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string) => {
    switch (command) {
      case "companion_prefs":
        return Promise.resolve(prefs);
      case "generated_sets_list":
        return sets instanceof Error ? Promise.reject(sets) : Promise.resolve(sets);
      case "companion_profiles":
        return Promise.resolve([]);
      default:
        return Promise.reject(new Error("unavailable"));
    }
  },
  Channel: class {},
}));

vi.mock("./ActivityHost", async () => ({
  ActivityHost: ({ activity }: { activity: ActivityInfo }) => {
    started.push(activity.id);
    return <div data-testid="host">{activity.name}</div>;
  },
}));

import ActivitiesView from "./ActivitiesView";
import type { ActivityLoad } from "./useActivityList";
import { buttonIn, click, polyfillDialog, settle } from "../companion/testDom";

polyfillDialog();

const info = (id: string, name: string, engine: string | null, category = "interactive") =>
  ({
    id,
    name,
    description: `About ${name}`,
    category,
    tags: [],
    version: "1.0.0",
    storage: false,
    savedUtc: null,
    help: null,
    preview: false,
    playedSeconds: 0,
    engine,
    source: { marketplaceId: "lullshift", builtin: true, commit: null },
  }) as ActivityInfo;

const QUIZ = info("lullshift.quiz-basics", "Quiz basics", "lullshift.quiz@1");
const MOVE = info("lullshift.desk-moves", "Desk moves", "lullshift.movement@1", "break");
const GAME = info("lullshift.game", "A game", null, "arcade");

const generated = (
  id: string,
  name: string,
  engine: string,
  model: string | null,
  category = "interactive",
): ActivityInfo => ({
  ...info(id, name, engine, category),
  description: "Written for you by your own model. It can be wrong.",
  source: { marketplaceId: "generated", builtin: false, commit: null },
  generated: {
    model,
    engine,
    createdUtc: "2026-11-08T10:00:00Z",
    topic: "Rivers",
    difficulty: "medium",
    unusable: false,
  },
});

let host: HTMLDivElement;
let root: Root;

const render = async (page: "play" | "rest" | "continue" = "play", list = [QUIZ, MOVE, GAME]) => {
  const load: ActivityLoad = { status: "ready", list };
  await act(async () => {
    root.render(
      <ActivitiesView
        headingRef={{ current: null }}
        active
        page={page}
        load={load}
        refresh={() => undefined}
        pause={null}
        now={Date.now()}
        onGoToSessions={() => undefined}
        onShowPage={() => undefined}
      />,
    );
  });
  await settle();
};
const buttons = (root: ParentNode = host) =>
  [...root.querySelectorAll("button")].map((b) => b.textContent?.trim() ?? "");
const freshOn = (name: string) =>
  [...host.querySelectorAll("li.activity-card")]
    .find((li) => li.querySelector(".activity-name")?.textContent === name)!
    .querySelector<HTMLButtonElement>(".btn-quiet:last-of-type");

beforeEach(() => {
  prefs = { toolsEnabled: false, lastScope: null };
  sets = [];
  started.length = 0;
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});
afterEach(() => {
  act(() => root.unmount());
  host.remove();
  document.querySelectorAll("dialog").forEach((d) => d.remove());
});

describe("Fresh set from my model", () => {
  it("is not offered while the switch is off", async () => {
    await render();
    expect(buttons().some((b) => b.startsWith("Fresh set from my model"))).toBe(false);
  });

  it("is offered on quiz packs with the first switch, never on apps, and not for body engines", async () => {
    prefs = {
      toolsEnabled: false,
      lastScope: null,
      contentGeneration: { enabled: true, bodyEnabled: false },
    };
    await render();
    const cards = [...host.querySelectorAll("li.activity-card")];
    const fresh = (name: string) =>
      cards
        .find((li) => li.querySelector(".activity-name")?.textContent === name)
        ?.textContent?.includes("Fresh set from my model");
    expect(fresh("Quiz basics")).toBe(true);
    expect(fresh("A game")).toBe(false);
    await render("rest");
    expect(host.textContent).toContain("Desk moves");
    expect(host.textContent).not.toContain("Fresh set from my model");
  });

  it("is offered on body packs only with the second switch", async () => {
    prefs = {
      toolsEnabled: false,
      lastScope: null,
      contentGeneration: { enabled: true, bodyEnabled: true },
    };
    await render("rest");
    expect(host.textContent).toContain("Fresh set from my model");
  });

  it("a body switch alone (a stored mismatch) offers nothing", async () => {
    prefs = {
      toolsEnabled: false,
      lastScope: null,
      contentGeneration: { enabled: false, bodyEnabled: true },
    };
    await render("rest");
    expect(host.textContent).not.toContain("Fresh set from my model");
  });

  it("opens the dialog with the built-in pack's title, and the focus returns on close", async () => {
    prefs = {
      toolsEnabled: false,
      lastScope: null,
      contentGeneration: { enabled: true, bodyEnabled: false },
    };
    await render();
    const open = freshOn("Quiz basics")!;
    open.focus();
    await click(open);
    const dlg = document.querySelector<HTMLDialogElement>("dialog[open]")!;
    expect(dlg.textContent).toContain("Fresh set from my model");
    expect(dlg.querySelector<HTMLInputElement>("input[type=text]")?.value).toBe("Quiz basics");
    await click(buttonIn(dlg, "Close"));
    expect(document.querySelector("dialog[open]")).toBeNull();
    expect(document.activeElement).toBe(open);
  });
});

describe("Made by your model", () => {
  it("lists each engine's saved sets with the badge and the model name", async () => {
    sets = [
      generated(
        "lullshift.generated.quiz.aaaaaaaaaaaa",
        "Rivers quiz",
        "lullshift.quiz@1",
        "gpt-test",
      ),
      generated("lullshift.generated.quiz.bbbbbbbbbbbb", "More rivers", "lullshift.quiz@1", null),
    ];
    await render();
    const group = host.querySelector(".generated-group")!;
    expect(host.textContent).toContain("Made by your model");
    expect(group.querySelector("h3")?.textContent).toBe("Quiz sets");
    const cards = [...group.querySelectorAll("li.activity-card")];
    expect(cards).toHaveLength(2);
    expect(cards[0]!.querySelector("[data-testid=ai-tag]")?.textContent).toBe(
      "AI-generated · gpt-test",
    );
    expect(cards[1]!.querySelector("[data-testid=ai-tag]")?.textContent).toBe("AI-generated");
    // No "From generated" source line on a set.
    expect(cards[0]!.querySelector(".activity-source")).toBeNull();
    // The normal list is unchanged: sets are not part of it.
    expect(host.querySelectorAll(".activity-grid")[0]!.textContent).not.toContain("Rivers quiz");
  });

  it("starts a set like any activity", async () => {
    sets = [
      generated("lullshift.generated.quiz.aaaaaaaaaaaa", "Rivers quiz", "lullshift.quiz@1", "m"),
    ];
    await render();
    await click(buttonIn(host.querySelector(".generated-group")!, "Start Rivers quiz"));
    expect(started).toContain("lullshift.generated.quiz.aaaaaaaaaaaa");
  });

  it("keeps saved sets visible and playable with the switches off, but offers no new ones", async () => {
    sets = [
      generated("lullshift.generated.quiz.aaaaaaaaaaaa", "Rivers quiz", "lullshift.quiz@1", "m"),
    ];
    await render();
    const group = host.querySelector(".generated-group")!;
    expect(group.textContent).toContain("Rivers quiz");
    expect(buttons(group).some((b) => b.startsWith("Fresh set from my model"))).toBe(false);
  });

  it("puts rest sets on Rest and shows nothing when the list can't be read", async () => {
    sets = [
      generated(
        "lullshift.generated.movement.cccccccccccc",
        "Desk routine",
        "lullshift.movement@1",
        "m",
        "break",
      ),
    ];
    await render("play");
    expect(host.textContent).not.toContain("Made by your model");
    await render("rest");
    expect(host.textContent).toContain("Made by your model");
    expect(host.textContent).toContain("Movement routines");
    expect(host.querySelector("[data-testid=ai-tag]")).not.toBeNull();
    sets = new Error("down");
    await render("rest", [MOVE]);
    // A new mount reads again; a failed read is "no sets", never an error screen.
    await act(async () => root.unmount());
    root = createRoot(host);
    await render("rest");
    expect(host.textContent).not.toContain("Made by your model");
  });

  it("shows a saved set on Continue", async () => {
    sets = [
      {
        ...generated(
          "lullshift.generated.quiz.aaaaaaaaaaaa",
          "Rivers quiz",
          "lullshift.quiz@1",
          "m",
        ),
        savedUtc: new Date().toISOString(),
      },
    ];
    await render("continue");
    expect(host.querySelector(".resume-card")?.textContent).toContain("Rivers quiz");
    expect(host.querySelector(".resume-card [data-testid=ai-tag]")).not.toBeNull();
  });
});
