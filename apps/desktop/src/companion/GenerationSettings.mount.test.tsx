// @vitest-environment jsdom
// Settings > Companion: the two content switches (off by default, consent with the exact example
// text before turning on, the body switch only while the first is on and turned off with it) and
// the list of saved model-written sets with Delete and Delete all.

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const calls: { command: string; args?: Record<string, unknown> }[] = [];
let prefs = { enabled: false, bodyEnabled: false };
let sets: unknown[] = [];

const SET = (id: string, name: string, model: string | null, engine = "lullshift.quiz@1") => ({
  id,
  name,
  description: "Written for you by your own model. It can be wrong.",
  category: "interactive",
  version: "0.1.0",
  storage: false,
  savedUtc: null,
  help: null,
  preview: false,
  playedSeconds: 0,
  engine,
  generated: {
    model,
    engine,
    createdUtc: "2026-11-08T10:00:00Z",
    topic: "Space",
    difficulty: "medium",
    unusable: false,
  },
});

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args?: Record<string, unknown>) => {
    calls.push({ command, args });
    switch (command) {
      case "companion_prefs":
        return Promise.resolve({ toolsEnabled: false, lastScope: null, contentGeneration: prefs });
      case "generation_set_settings": {
        const a = args as { enabled: boolean; bodyEnabled: boolean };
        prefs = { enabled: a.enabled, bodyEnabled: a.enabled && a.bodyEnabled };
        return Promise.resolve(prefs);
      }
      case "generation_preview":
        return Promise.resolve(
          `{"example":"${(args as { req: { engine: string } }).req.engine} request"}`,
        );
      case "generated_sets_list":
        return Promise.resolve(sets);
      case "generated_set_delete":
        sets = sets.filter((s) => (s as { id: string }).id !== (args as { id: string }).id);
        return Promise.resolve(true);
      case "generated_sets_delete_all": {
        const n = sets.length;
        sets = [];
        return Promise.resolve(n);
      }
      default:
        return Promise.resolve(null);
    }
  },
  Channel: class {},
}));

import { GenerationSettings } from "./GenerationSettings";
import { buttonIn, click, polyfillDialog, settle } from "./testDom";

polyfillDialog();

let host: HTMLDivElement;
let root: Root;
const notices: string[] = [];

const mount = async (hasModel = true) => {
  await act(async () =>
    root.render(<GenerationSettings hasModel={hasModel} say={(n) => notices.push(n.text)} />),
  );
  await settle();
};
const sw = (label: string) =>
  [...host.querySelectorAll<HTMLButtonElement>("[role=switch]")].find((b) =>
    document.getElementById(b.getAttribute("aria-labelledby")!)?.textContent?.includes(label),
  )!;
const content = () => sw("Let my model write activity content");
const body = () => sw("movement, breathing and pause");
const dialog = () => document.querySelector<HTMLDialogElement>("dialog[open]");

beforeEach(() => {
  calls.length = 0;
  notices.length = 0;
  prefs = { enabled: false, bodyEnabled: false };
  sets = [];
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});
afterEach(() => {
  act(() => root.unmount());
  host.remove();
});

describe("the switches", () => {
  it("are off by default, and the first needs a model to be set up", async () => {
    await mount(false);
    expect(content().getAttribute("aria-checked")).toBe("false");
    expect(content().disabled).toBe(true);
    expect(body().disabled).toBe(true);
    expect(host.textContent).toContain("Set up a provider and a model");
    expect(calls.map((c) => c.command)).not.toContain("generation_set_settings");
  });

  it("show the exact example text before turning on, and Not now changes nothing", async () => {
    await mount();
    expect(content().disabled).toBe(false);
    await click(content());
    expect(dialog()?.textContent).toContain("Fresh set from my model");
    expect(dialog()?.textContent).toContain("never holds your sessions");
    expect(dialog()?.querySelector("[data-testid=consent-text]")?.textContent).toBe(
      '{"example":"quiz request"}',
    );
    // Only the preview was asked for: nothing is turned on, nothing sent.
    expect(calls.map((c) => c.command)).toContain("generation_preview");
    expect(calls.map((c) => c.command)).not.toContain("generation_set_settings");
    await click(buttonIn(dialog()!, "Not now"));
    expect(dialog()).toBeNull();
    expect(content().getAttribute("aria-checked")).toBe("false");
    expect(prefs.enabled).toBe(false);
  });

  it("turn on after consent, then the body switch needs its own safety consent", async () => {
    await mount();
    expect(body().disabled).toBe(true);
    await click(content());
    await click(buttonIn(dialog()!, "Turn on"));
    expect(content().getAttribute("aria-checked")).toBe("true");
    expect(calls.find((c) => c.command === "generation_set_settings")?.args).toEqual({
      enabled: true,
      bodyEnabled: false,
    });
    expect(body().disabled).toBe(false);
    expect(body().getAttribute("aria-checked")).toBe("false");

    await click(body());
    const d = dialog()!;
    expect(d.textContent).toContain("AI-generated and can be wrong");
    expect(d.textContent).toContain("Stop if anything feels uncomfortable");
    expect(d.textContent).toContain("injury or a health condition");
    expect(d.querySelector("[data-testid=consent-text]")?.textContent).toBe(
      '{"example":"movement request"}',
    );
    // The consent dialog describes itself for screen readers.
    expect(d.getAttribute("aria-describedby")).toBeTruthy();
    await click(buttonIn(d, "Turn on"));
    expect(body().getAttribute("aria-checked")).toBe("true");
    expect(prefs).toEqual({ enabled: true, bodyEnabled: true });
  });

  it("turning the first off turns the body switch off with it, with no question", async () => {
    prefs = { enabled: true, bodyEnabled: true };
    await mount();
    expect(content().getAttribute("aria-checked")).toBe("true");
    expect(body().getAttribute("aria-checked")).toBe("true");
    await click(content());
    expect(dialog()).toBeNull();
    expect(content().getAttribute("aria-checked")).toBe("false");
    expect(body().getAttribute("aria-checked")).toBe("false");
    expect(body().disabled).toBe(true);
    expect(prefs).toEqual({ enabled: false, bodyEnabled: false });
    // Saved sets are not touched by turning it off.
    expect(calls.map((c) => c.command)).not.toContain("generated_set_delete");
  });

  it("an older core without the setting reads as off", async () => {
    await mount();
    expect(content().getAttribute("aria-checked")).toBe("false");
  });
});

describe("the saved sets", () => {
  beforeEach(() => {
    sets = [
      SET("lullshift.generated.quiz.aaaaaaaaaaaa", "Space quiz", "gpt-test"),
      SET(
        "lullshift.generated.movement.bbbbbbbbbbbb",
        "Desk stretch",
        null,
        "lullshift.movement@1",
      ),
    ];
  });

  it("lists title, topic, engine, date and model, each with the AI-generated label and Delete", async () => {
    await mount();
    const rows = [...host.querySelectorAll(".generated-row")];
    expect(rows).toHaveLength(2);
    expect(rows[0]!.textContent).toContain("Space quiz");
    expect(rows[0]!.textContent).toContain("Topic: Space");
    expect(rows[0]!.textContent).toContain("Quiz sets");
    expect(rows[0]!.querySelector("[data-testid=ai-tag]")?.textContent).toBe(
      "AI-generated · gpt-test",
    );
    expect(rows[1]!.querySelector("[data-testid=ai-tag]")?.textContent).toBe("AI-generated");
    expect(buttonIn(rows[0] as HTMLElement, "Delete Space quiz")).toBeTruthy();
  });

  it("deletes one set", async () => {
    await mount();
    await click(buttonIn(host, "Delete Space quiz"));
    expect(calls.find((c) => c.command === "generated_set_delete")?.args).toEqual({
      id: "lullshift.generated.quiz.aaaaaaaaaaaa",
    });
    expect(host.textContent).not.toContain("Space quiz");
    expect(host.textContent).toContain("Desk stretch");
    expect(notices).toContain("Space quiz was deleted.");
  });

  it("deletes everything only after a confirmation", async () => {
    await mount();
    await click(buttonIn(host, "Delete all model-written sets"));
    expect(dialog()?.textContent).toContain("can't be undone");
    expect(calls.map((c) => c.command)).not.toContain("generated_sets_delete_all");
    await click(buttonIn(dialog()!, "Keep them"));
    expect(dialog()).toBeNull();
    expect(calls.map((c) => c.command)).not.toContain("generated_sets_delete_all");
    await click(buttonIn(host, "Delete all model-written sets"));
    await click(buttonIn(dialog()!, "Delete all"));
    expect(calls.map((c) => c.command)).toContain("generated_sets_delete_all");
    expect(host.textContent).toContain("No model-written sets are saved.");
  });

  it("says so when there are none", async () => {
    sets = [];
    await mount();
    expect(host.textContent).toContain("No model-written sets are saved.");
    const labels = [...host.querySelectorAll("button")].map((b) => b.textContent?.trim());
    expect(labels).not.toContain("Delete all model-written sets");
  });
});
