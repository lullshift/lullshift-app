// @vitest-environment jsdom
// Mounted in a DOM with the Tauri bridge mocked: proves which commands the model browser sends,
// and when (decision 32). Only a click on Load or Refresh may ask for the network.

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
      return Promise.reject(err instanceof Error ? err : new Error(String(err)));
    }
  },
  Channel: class {},
}));

import type { CatalogModel, ModelCatalog } from "./api";
import { CATALOG_LOAD_LABEL } from "./logic";
import { ModelCatalogBrowser } from "./ModelCatalog";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const LOADED = "2026-11-07T10:42:00Z";
const CHECKED = "2026-11-07T11:05:00Z";

const catalog = (more: Partial<ModelCatalog> = {}): ModelCatalog => ({
  loadedAt: LOADED,
  checkedAt: LOADED,
  unchanged: false,
  fromMemory: false,
  providers: [{ id: "anthropic", name: "Anthropic", modelCount: 2 }],
  ...more,
});

const MODELS: CatalogModel[] = [
  {
    id: "claude-named",
    name: "Named Model",
    family: null,
    toolCall: true,
    reasoning: false,
    releaseDate: "2026-09-01",
    contextTokens: 200_000,
    outputTokens: null,
    costInput: 3,
    costOutput: 15,
  },
  {
    id: "claude-unnamed",
    name: null,
    family: null,
    toolCall: false,
    reasoning: false,
    releaseDate: "2026-08-01",
    contextTokens: null,
    outputTokens: null,
    costInput: null,
    costOutput: null,
  },
];

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  calls.length = 0;
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
});

/** Lets pending promises and the effects they trigger settle. */
async function settle() {
  for (let i = 0; i < 5; i++) {
    await act(async () => {
      await Promise.resolve();
    });
  }
}

function button(label: string): HTMLButtonElement {
  const b = [...host.querySelectorAll("button")].find((x) => x.textContent?.trim() === label);
  if (!b) throw new Error(`no button "${label}" in: ${host.textContent}`);
  return b;
}

async function click(label: string) {
  await act(async () => {
    button(label).click();
  });
  await settle();
}

const commands = () => calls.map(([c]) => c);

describe("ModelCatalogBrowser mounted", () => {
  it("only reads memory on mount, and loads only after Load is clicked", async () => {
    let state: ModelCatalog | null = null;
    answer = (command, args) => {
      if (command === "companion_model_catalog") {
        if (args.mode === "cached") return state;
        state = catalog();
        return state;
      }
      if (command === "companion_catalog_models") return MODELS;
      throw new Error(`unexpected ${command}`);
    };
    const chosen = vi.fn();
    await act(async () => {
      root.render(<ModelCatalogBrowser kind="anthropic" chosen="" onChoose={chosen} />);
    });
    await settle();

    // Mounting sends exactly one memory-only read and nothing else.
    expect(calls).toEqual([["companion_model_catalog", { mode: "cached" }]]);
    expect(host.textContent).toContain(CATALOG_LOAD_LABEL);
    expect(host.querySelector(".catalog-list")).toBeNull();

    await click(CATALOG_LOAD_LABEL);
    expect(calls[1]).toEqual(["companion_model_catalog", { mode: "load" }]);
    // Then the chosen provider's models are read from the core's memory.
    expect(calls[2]).toEqual(["companion_catalog_models", { providerId: "anthropic" }]);
    expect(calls).toHaveLength(3);
    expect(host.textContent).toContain("Loaded at");

    // A model without a name shows its id once, with no empty name span.
    const rows = [...host.querySelectorAll(".catalog-model")];
    expect(rows).toHaveLength(2);
    const unnamed = rows.find((r) => r.textContent?.includes("claude-unnamed"));
    expect(unnamed?.querySelector(".catalog-model-name")).toBeNull();

    // Choosing a model only fills the field: no command is sent.
    await act(async () => {
      (rows[0] as HTMLButtonElement).click();
    });
    expect(chosen).toHaveBeenCalledWith("claude-named");
    expect(calls).toHaveLength(3);
  });

  it("refreshes only when Refresh is clicked, and shows both times after no change", async () => {
    let state = catalog({ fromMemory: true });
    answer = (command, args) => {
      if (command === "companion_model_catalog") {
        if (args.mode === "refresh") {
          state = catalog({ checkedAt: CHECKED, unchanged: true });
          return state;
        }
        return state;
      }
      if (command === "companion_catalog_models") return MODELS;
      throw new Error(`unexpected ${command}`);
    };
    await act(async () => {
      root.render(<ModelCatalogBrowser kind="anthropic" chosen="" onChoose={() => undefined} />);
    });
    await settle();
    // A list already in memory: one memory read, then its models from memory. No load/refresh.
    expect(calls).toEqual([
      ["companion_model_catalog", { mode: "cached" }],
      ["companion_catalog_models", { providerId: "anthropic" }],
    ]);

    await click("Refresh");
    expect(commands().filter((c) => c === "companion_model_catalog")).toHaveLength(2);
    expect(calls[2]).toEqual(["companion_model_catalog", { mode: "refresh" }]);
    // Same load time, so the models are not read again.
    expect(calls).toHaveLength(3);
    expect(host.querySelector(".catalog-loaded")?.textContent).toMatch(
      /^Loaded at \d\d:\d\d, checked at \d\d:\d\d: no changes/,
    );
    expect(host.textContent).toContain("The model list hasn't changed.");
  });

  it("sends nothing more when the memory read fails", async () => {
    answer = () => {
      throw new Error("bridge down");
    };
    await act(async () => {
      root.render(
        <ModelCatalogBrowser kind="openaiCompatible" chosen="" onChoose={() => undefined} />,
      );
    });
    await settle();
    expect(calls).toEqual([["companion_model_catalog", { mode: "cached" }]]);
    expect(button(CATALOG_LOAD_LABEL).disabled).toBe(false);
  });
});
