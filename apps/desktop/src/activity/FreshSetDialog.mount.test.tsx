// @vitest-environment jsdom
// "Fresh set from my model": the topic is prefilled only for built-in packs, the choices match
// the engine, "Show exact text" reads the real request, progress has one polite sentence per
// phase and a Cancel, and a failure ends in a calm card with the built-in set as the way out.

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

type Call = { command: string; args?: Record<string, unknown> };
const calls: Call[] = [];
let pending: {
  resolve: (v: unknown) => void;
  reject: (e: unknown) => void;
  emit: (e: unknown) => void;
} | null = null;

const PROFILES = [
  {
    id: "p1",
    kind: "openaiCompatible",
    label: "Local",
    baseUrl: "http://localhost:11434/v1",
    host: "localhost:11434",
    model: "small",
    models: [
      { model: "small", label: null, isDefault: true },
      { model: "large", label: "Large one", isDefault: false },
    ],
    monthlyTokenBudget: null,
    isDefault: true,
    hasKey: true,
    keychainOk: true,
  },
];
let profiles: unknown[] = PROFILES;

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args?: Record<string, unknown>) => {
    calls.push({ command, args });
    switch (command) {
      case "companion_profiles":
        return Promise.resolve(profiles);
      case "generation_preview":
        return Promise.resolve(JSON.stringify((args as { req: unknown }).req));
      case "generate_set":
        return new Promise((resolve, reject) => {
          const ch = (args as { onEvent: { onmessage: (e: unknown) => void } }).onEvent;
          pending = { resolve, reject, emit: (e) => ch.onmessage(e) };
        });
      case "generation_cancel":
        return Promise.resolve(null);
      default:
        return Promise.resolve(null);
    }
  },
  Channel: class {
    onmessage: (e: unknown) => void = () => undefined;
  },
}));

import { FreshSetDialog, initialTopic } from "./FreshSetDialog";
import type { ActivityInfo } from "./ActivityHost";
import { buttonIn, click, polyfillDialog, settle, typeInto } from "../companion/testDom";

polyfillDialog();

const pack = (over: Partial<ActivityInfo> = {}): ActivityInfo => ({
  id: "lullshift.quiz-basics",
  name: "Quick Quiz: Basics",
  description: "d",
  category: "interactive",
  version: "1.0.0",
  storage: false,
  savedUtc: null,
  help: null,
  preview: false,
  playedSeconds: 0,
  engine: "lullshift.quiz@1",
  source: { marketplaceId: "lullshift", builtin: true, commit: null },
  ...over,
});
const INSTALLED = pack({
  id: "com.example.facts",
  name: "CANARY third-party title",
  source: { marketplaceId: "acme", builtin: false, commit: "abc" },
});
const NEW_SET: ActivityInfo = {
  ...pack({ id: "lullshift.generated.quiz.abcdefghij12", name: "Fresh quiz" }),
  source: { marketplaceId: "generated", builtin: false, commit: null },
  generated: {
    model: "small",
    engine: "lullshift.quiz@1",
    createdUtc: "2026-11-08T10:00:00Z",
    topic: "x",
    difficulty: "medium",
    unusable: false,
  },
};

let host: HTMLDivElement;
let root: Root;
const started: string[] = [];
const played: string[] = [];
const written: number[] = [];
let closed = 0;

const mount = async (engineRef: string, p: ActivityInfo | null) => {
  await act(async () =>
    root.render(
      <FreshSetDialog
        open
        engineRef={engineRef}
        pack={p}
        onClose={() => {
          closed += 1;
        }}
        onWritten={() => written.push(1)}
        onStart={(s) => started.push(s.id)}
        onPlayPack={(a) => played.push(a.id)}
      />,
    ),
  );
  await settle();
};
const dlg = () => document.querySelector<HTMLDialogElement>("dialog[open]")!;
const field = (label: string) =>
  [...dlg().querySelectorAll("label")].find((l) => l.textContent?.startsWith(label))!;
const input = (label: string) =>
  dlg().querySelector<HTMLInputElement | HTMLSelectElement>(`#${field(label).htmlFor}`)!;
const sent = () =>
  calls.find((c) => c.command === "generate_set")?.args as {
    requestId: string;
    req: Record<string, unknown>;
  };
const phase = () => dlg().querySelector("[data-testid=phase]")?.textContent;

beforeEach(() => {
  calls.length = 0;
  pending = null;
  profiles = PROFILES;
  started.length = 0;
  played.length = 0;
  written.length = 0;
  closed = 0;
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});
afterEach(() => {
  act(() => root.unmount());
  host.remove();
  document.querySelectorAll("dialog").forEach((d) => d.remove());
});

describe("the topic", () => {
  it("starts with a built-in pack's title and with nothing for an installed pack", async () => {
    expect(initialTopic(pack())).toBe("Quick Quiz: Basics");
    expect(initialTopic(INSTALLED)).toBe("");
    expect(initialTopic(null)).toBe("");
    await mount("lullshift.quiz@1", INSTALLED);
    expect(input("Topic").value).toBe("");
    expect(dlg().textContent).not.toContain("CANARY");
    // Nothing can be written without a topic.
    expect(buttonIn(dlg(), "Write the set").disabled).toBe(true);
  });

  it("is prefilled for a built-in pack and can be changed; 1 to 80 characters", async () => {
    await mount("lullshift.quiz@1", pack());
    expect(input("Topic").value).toBe("Quick Quiz: Basics");
    expect(buttonIn(dlg(), "Write the set").disabled).toBe(false);
    await typeInto(input("Topic") as HTMLInputElement, "   ");
    expect(buttonIn(dlg(), "Write the set").disabled).toBe(true);
    await typeInto(input("Topic") as HTMLInputElement, "x".repeat(81));
    expect(buttonIn(dlg(), "Write the set").disabled).toBe(true);
    await typeInto(input("Topic") as HTMLInputElement, "x".repeat(80));
    expect(buttonIn(dlg(), "Write the set").disabled).toBe(false);
  });
});

describe("the choices", () => {
  it("offer difficulty and the engine's sizes, and the avoid switch is on with a sentence", async () => {
    await mount("lullshift.quiz@1", pack());
    expect(input("Difficulty").value).toBe("medium");
    expect([...(input("How many") as HTMLSelectElement).options].map((o) => o.text)).toEqual([
      "5 questions",
      "8 questions",
      "12 questions",
    ]);
    const avoid = dlg().querySelector<HTMLInputElement>("input[type=checkbox]")!;
    expect(avoid.checked).toBe(true);
    expect(dlg().textContent).toContain("Sends up to 20 earlier questions");
    expect(dlg().textContent).toContain("never yours");
    expect(dlg().textContent).toContain("Never your sessions, tasks, notes, code or file names");
    await click(avoid);
    await click(buttonIn(dlg(), "Write the set"));
    expect(sent().req).toMatchObject({
      engine: "quiz",
      topic: "Quick Quiz: Basics",
      difficulty: "medium",
      size: 8,
      avoid: false,
      originPack: "lullshift.quiz-basics",
      breathing: null,
      profileId: "p1",
      model: "small",
    });
  });

  it("have no difficulty for body engines, which are always gentle", async () => {
    await mount("lullshift.movement@1", pack({ engine: "lullshift.movement@1" }));
    expect(
      [...dlg().querySelectorAll("label")].some((l) => l.textContent?.startsWith("Difficulty")),
    ).toBe(false);
    expect(dlg().textContent).toContain("Gentle, desk-friendly");
    expect([...(input("How many") as HTMLSelectElement).options].map((o) => o.value)).toEqual([
      "3",
      "5",
      "8",
    ]);
    await click(buttonIn(dlg(), "Write the set"));
    expect(sent().req).toMatchObject({ engine: "movement", difficulty: "gentle", size: 5 });
  });

  it("show the pattern and rounds for breathing, and send them", async () => {
    await mount("lullshift.breathing@1", pack({ engine: "lullshift.breathing@1" }));
    expect(
      [...(input("Breathing pattern") as HTMLSelectElement).options].map((o) => o.value),
    ).toEqual(["even", "longerOut", "boxGentle"]);
    expect([...(input("Rounds") as HTMLSelectElement).options].map((o) => o.value)).toEqual([
      "4",
      "6",
      "8",
    ]);
    // No size and no avoid list for one breathing routine.
    expect(dlg().querySelector("input[type=checkbox]")).toBeNull();
    await act(async () => {
      const sel = input("Breathing pattern") as HTMLSelectElement;
      sel.value = "boxGentle";
      sel.dispatchEvent(new Event("change", { bubbles: true }));
    });
    await click(buttonIn(dlg(), "Write the set"));
    expect(sent().req).toMatchObject({
      engine: "breathing",
      size: 1,
      avoid: false,
      breathing: { pattern: "boxGentle", rounds: 6 },
    });
  });

  it("use the model picked for this set only", async () => {
    await mount("lullshift.quiz@1", pack());
    expect(dlg().textContent).toContain("small · Local");
    await click(buttonIn(dlg(), "Change model"));
    const picker = [...document.querySelectorAll("dialog[open]")].find((d) =>
      d.textContent?.includes("Model for this set"),
    )!;
    expect(picker.textContent).toContain("Your pick is used for this set only.");
    const large = [...picker.querySelectorAll<HTMLInputElement>("input[type=radio]")][1]!;
    await click(large);
    await click(buttonIn(picker as HTMLElement, "Done"));
    expect(dlg().textContent).toContain("Large one · Local");
    expect(dlg().textContent).toContain("(this set only)");
    await click(buttonIn(dlg(), "Write the set"));
    expect(sent().req).toMatchObject({ profileId: "p1", model: "large" });
  });

  it("say so when no provider is set up, and cannot write", async () => {
    profiles = [];
    await mount("lullshift.quiz@1", pack());
    expect(dlg().textContent).toContain("No provider is set up");
    expect(buttonIn(dlg(), "Write the set").disabled).toBe(true);
  });
});

describe("Show exact text", () => {
  it("reads the real request from the core, follows the choices, and hides again", async () => {
    await mount("lullshift.quiz@1", pack());
    expect(calls.map((c) => c.command)).not.toContain("generation_preview");
    await click(buttonIn(dlg(), "Show exact text"));
    const text = () => dlg().querySelector("[data-testid=exact-text]")?.textContent ?? "";
    expect(JSON.parse(text())).toMatchObject({ engine: "quiz", topic: "Quick Quiz: Basics" });
    await typeInto(input("Topic") as HTMLInputElement, "Rivers");
    await settle();
    expect(JSON.parse(text())).toMatchObject({ topic: "Rivers" });
    await click(buttonIn(dlg(), "Hide exact text"));
    expect(dlg().querySelector("[data-testid=exact-text]")).toBeNull();
    // Looking never sends the set.
    expect(calls.map((c) => c.command)).not.toContain("generate_set");
  });
});

describe("progress", () => {
  it("shows one sentence per phase with a Cancel, and a cancel returns to the form", async () => {
    await mount("lullshift.quiz@1", pack());
    await click(buttonIn(dlg(), "Write the set"));
    const id = sent().requestId;
    expect(phase()).toBe("Your model is writing the set. This can take up to a minute.");
    // Events for another request are ignored.
    await act(async () => pending!.emit({ type: "phase", requestId: "other", phase: "checking" }));
    expect(phase()).toContain("writing the set");
    await act(async () => pending!.emit({ type: "phase", requestId: id, phase: "checking" }));
    expect(phase()).toBe("Checking that the set fits the activity.");
    await act(async () => pending!.emit({ type: "phase", requestId: id, phase: "retrying" }));
    expect(phase()).toContain("asking once more");
    // The status sentence is polite, and Esc can't close the dialog while it runs.
    expect(dlg().querySelector("[data-testid=phase]")?.getAttribute("aria-live")).toBe("polite");
    await click(buttonIn(dlg(), "Cancel"));
    expect(calls.find((c) => c.command === "generation_cancel")?.args).toEqual({ requestId: id });
    await act(async () =>
      pending!.reject({ kind: "cancelled", message: "Cancelled.", providerReason: null }),
    );
    await settle();
    expect(buttonIn(dlg(), "Write the set")).toBeTruthy();
    expect(dlg().textContent).toContain("Cancelled.");
  });
});

describe("the result", () => {
  it("offers to start a set that fit", async () => {
    await mount("lullshift.quiz@1", pack());
    await click(buttonIn(dlg(), "Write the set"));
    await act(async () => pending!.resolve(NEW_SET));
    await settle();
    expect(written).toEqual([1]);
    expect(dlg().textContent).toContain("Fresh quiz");
    expect(dlg().querySelector("[data-testid=ai-notice]")?.textContent).toContain(
      "AI-generated by small. It can be wrong.",
    );
    await click(buttonIn(dlg(), "Start it"));
    expect(started).toEqual(["lullshift.generated.quiz.abcdefghij12"]);
    expect(closed).toBe(1);
  });

  it("shows the calm card when it did not fit, with the built-in set as the way out", async () => {
    await mount("lullshift.quiz@1", pack());
    await click(buttonIn(dlg(), "Write the set"));
    await act(async () =>
      pending!.reject({ kind: "didNotFit", message: "x", providerReason: null }),
    );
    await settle();
    const card = dlg().querySelector("[data-testid=fresh-failed]")!;
    expect(card.getAttribute("role")).toBe("alert");
    expect(card.textContent).toContain(
      "Your model's set didn't fit this activity, so Lullshift didn't start it. Nothing was lost.",
    );
    await click(buttonIn(dlg(), "Play the built-in set"));
    expect(played).toEqual(["lullshift.quiz-basics"]);
    expect(started).toEqual([]);
  });

  it("has no built-in set to play when started from an engine group", async () => {
    await mount("lullshift.quiz@1", null);
    await typeInto(input("Topic") as HTMLInputElement, "Birds");
    await click(buttonIn(dlg(), "Write the set"));
    await act(async () =>
      pending!.reject({ kind: "didNotFit", message: "x", providerReason: null }),
    );
    await settle();
    expect([...dlg().querySelectorAll("button")].map((b) => b.textContent)).not.toContain(
      "Play the built-in set",
    );
    expect(buttonIn(dlg(), "Change and try again")).toBeTruthy();
  });

  it.each([
    ["offline", "Can't reach localhost:11434. Check your connection."],
    ["noKey", "No key is set for this provider yet."],
    ["timeout", "took too long to answer"],
    ["budgetRefused", "token budget for this provider is used up"],
    ["provider", "had a problem and couldn't answer"],
  ])("tells the truth about a %s failure", async (kind, text) => {
    await mount("lullshift.quiz@1", pack());
    await click(buttonIn(dlg(), "Write the set"));
    await act(async () =>
      pending!.reject({
        kind,
        message: "x",
        providerReason:
          kind === "provider" ? { speaker: "The provider", text: "Overloaded" } : null,
      }),
    );
    await settle();
    const card = dlg().querySelector("[data-testid=fresh-failed]")!;
    expect(card.textContent).toContain(text);
    if (kind === "provider") {
      // The provider's words are apart from Lullshift's sentence, as plain text.
      expect(card.querySelector("[data-testid=provider-said]")?.textContent).toContain(
        "Overloaded",
      );
    }
  });
});
