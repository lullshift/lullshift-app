// @vitest-environment jsdom
// The model picker modal (decisions 32 and 36), mounted with the Tauri bridge mocked: proves that
// opening it loads the models.dev list the first time only, that later opens use memory, and
// that the results work from the keyboard and a typed name is still possible.

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { renderToStaticMarkup } from "react-dom/server";
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
import { CATALOG_MAX_ROWS, CATALOG_NOTE, DEFAULT_CATALOG_FILTERS } from "./logic";
import { CatalogFilterBar, CatalogModelList, ModelPickerDialog } from "./ModelPicker";
import { buttonIn, click, key, polyfillDialog, settle, typeInto } from "./testDom";

polyfillDialog();

const NOW = new Date(Date.UTC(2026, 10, 7));
const LOADED = "2026-11-07T10:42:00Z";

const catalog = (more: Partial<ModelCatalog> = {}): ModelCatalog => ({
  loadedAt: LOADED,
  checkedAt: LOADED,
  unchanged: false,
  fromMemory: false,
  providers: [{ id: "anthropic", name: "Anthropic", modelCount: 2 }],
  ...more,
});

function model(id: string, more: Partial<CatalogModel> = {}): CatalogModel {
  return {
    id,
    name: `Name of ${id}`,
    family: null,
    toolCall: false,
    reasoning: false,
    releaseDate: "2026-01-01",
    contextTokens: 128_000,
    outputTokens: null,
    costInput: 1,
    costOutput: 2,
    ...more,
  };
}

const MODELS = [
  model("claude-new", { releaseDate: "2026-09-01" }),
  model("claude-old", { releaseDate: "2026-08-01", name: null }),
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

/** A core whose memory starts empty and holds the list after the first load. */
function fakeCore() {
  let memory: ModelCatalog | null = null;
  answer = (command, args) => {
    if (command === "companion_model_catalog") {
      if (args.mode === "cached") return memory;
      memory = catalog({ fromMemory: memory !== null });
      return memory;
    }
    if (command === "companion_catalog_models") return MODELS;
    throw new Error(`unexpected ${command}`);
  };
}

async function openPicker(onChoose = vi.fn(), onClose = vi.fn()) {
  await act(async () => {
    root.render(
      <ModelPickerDialog
        kind="anthropic"
        title="Add a model"
        chosen=""
        onChoose={onChoose}
        onClose={onClose}
      />,
    );
  });
  await settle();
  return { onChoose, onClose };
}

describe("ModelPickerDialog", () => {
  it("loads the list on the first open only; later opens read memory", async () => {
    fakeCore();
    await openPicker();
    expect(calls).toEqual([
      ["companion_model_catalog", { mode: "cached" }],
      ["companion_model_catalog", { mode: "load" }],
      ["companion_catalog_models", { providerId: "anthropic" }],
    ]);
    // The privacy line is visible in the modal.
    expect(host.textContent).toContain(CATALOG_NOTE);
    expect(host.querySelectorAll("[role=option]")).toHaveLength(2);

    // Close (unmount) and open again: memory only, no load.
    act(() => root.render(<></>));
    calls.length = 0;
    await openPicker();
    expect(calls).toEqual([
      ["companion_model_catalog", { mode: "cached" }],
      ["companion_catalog_models", { providerId: "anthropic" }],
    ]);

    // Refresh is the other path to the network.
    calls.length = 0;
    await click(buttonIn(host, "Refresh"));
    expect(calls[0]).toEqual(["companion_model_catalog", { mode: "refresh" }]);
  });

  it("moves through results with the keyboard and chooses with Enter", async () => {
    fakeCore();
    const { onChoose } = await openPicker();
    const list = host.querySelector<HTMLUListElement>("[role=listbox]");
    expect(list).not.toBeNull();
    const active = () => list?.getAttribute("aria-activedescendant") ?? "";
    expect(active()).toMatch(/-opt-0$/);
    await key(list!, "ArrowDown");
    expect(active()).toMatch(/-opt-1$/);
    await key(list!, "ArrowDown");
    expect(active()).toMatch(/-opt-1$/);
    await key(list!, "Home");
    expect(active()).toMatch(/-opt-0$/);
    await key(list!, "End");
    expect(active()).toMatch(/-opt-1$/);
    const before = calls.length;
    await key(list!, "Enter");
    expect(onChoose).toHaveBeenCalledWith({ model: "claude-old", name: null });
    expect(calls).toHaveLength(before);

    // The search box filters and ArrowDown moves focus into the results.
    const search = host.querySelector<HTMLInputElement>('input[type="search"]')!;
    await typeInto(search, "new");
    expect(host.querySelectorAll("[role=option]")).toHaveLength(1);
    await key(search, "ArrowDown");
    expect(document.activeElement).toBe(host.querySelector("[role=listbox]"));
  });

  it("still takes a typed model name, checked with the same rule, when the list fails", async () => {
    answer = (command) => {
      if (command === "companion_model_catalog") throw new Error("offline");
      throw new Error(`unexpected ${command}`);
    };
    const { onChoose } = await openPicker();
    expect(host.textContent).toContain("You can still type the model name below.");
    const input = host.querySelector<HTMLInputElement>('input[type="text"]')!;
    await typeInto(input, "has space");
    await click(buttonIn(host, "Use this name"));
    expect(onChoose).not.toHaveBeenCalled();
    expect(host.textContent).toContain("letters, digits");
    await typeInto(input, "my-local/model:7b");
    await click(buttonIn(host, "Use this name"));
    expect(onChoose).toHaveBeenCalledWith({ model: "my-local/model:7b", name: null });
  });
});

describe("CatalogModelList", () => {
  const models = [
    model("a-old", { releaseDate: "2023-05-01", toolCall: true }),
    model("b-new", { releaseDate: "2026-09-01", costInput: null, costOutput: null }),
  ];

  it("renders a listbox with name, id and facts, and a polite live count", () => {
    const html = renderToStaticMarkup(
      <CatalogModelList
        models={models}
        filters={DEFAULT_CATALOG_FILTERS}
        now={NOW}
        chosen="b-new"
        onChoose={() => undefined}
      />,
    );
    expect(html).toContain('role="status" aria-live="polite"');
    expect(html).toContain('role="listbox"');
    expect(html).toContain("2 models");
    expect(html).toContain('<code class="catalog-model-id">a-old</code>');
    expect(html).toContain("price unknown");
    expect(html.indexOf("b-new")).toBeLessThan(html.indexOf("a-old"));
    expect(html).toMatch(/aria-selected="true"[^>]*>.*b-new/);
  });

  it("applies the filters and draws at most the row cap", () => {
    const none = renderToStaticMarkup(
      <CatalogModelList
        models={models}
        filters={{ ...DEFAULT_CATALOG_FILTERS, text: "zzz" }}
        now={NOW}
        chosen=""
        onChoose={() => undefined}
      />,
    );
    expect(none).toContain("No models match these filters (out of 2).");
    const many = Array.from({ length: CATALOG_MAX_ROWS + 5 }, (_, i) =>
      model(`m-${String(i).padStart(4, "0")}`),
    );
    const html = renderToStaticMarkup(
      <CatalogModelList
        models={many}
        filters={DEFAULT_CATALOG_FILTERS}
        now={NOW}
        chosen=""
        onChoose={() => undefined}
      />,
    );
    expect(html.match(/role="option"/g)).toHaveLength(CATALOG_MAX_ROWS);
    expect(html).toContain(`Showing the first ${CATALOG_MAX_ROWS}`);
  });
});

describe("CatalogFilterBar", () => {
  it("has a search box and one Filter and sort button, closed at first", () => {
    const html = renderToStaticMarkup(
      <CatalogFilterBar filters={DEFAULT_CATALOG_FILTERS} onChange={() => undefined} />,
    );
    expect(html).toContain("Search models");
    expect(html).toContain("Filter and sort");
    expect(html).toContain('aria-expanded="false"');
    expect(html).not.toContain('type="checkbox"');
  });
});

describe("picker layout and the merged Filter and sort menu", () => {
  it("shows one short note, with Refresh and the loaded time in the footer beside Cancel", async () => {
    fakeCore();
    await openPicker();
    const notes = host.querySelectorAll(".dialog-description p");
    expect(notes).toHaveLength(1);
    expect(notes[0]?.textContent).toBe(CATALOG_NOTE);
    const footer = host.querySelector(".dialog-actions")!;
    expect(footer.textContent).toContain("Loaded");
    expect(footer.textContent).toContain("1 provider");
    expect(buttonIn(footer, "Refresh")).toBeTruthy();
    expect(buttonIn(footer, "Cancel")).toBeTruthy();
    // Not in the body any more.
    expect(host.querySelector(".dialog-body .picker")?.textContent).not.toContain("Refresh");
  });

  it("filters and sorts from one popover with checkboxes and a radio group", async () => {
    fakeCore();
    await openPicker();
    expect(host.querySelectorAll('input[type="checkbox"]')).toHaveLength(0);
    const menu = buttonIn(host, "Filter and sort");
    expect(menu.getAttribute("aria-expanded")).toBe("false");
    await click(menu);
    expect(menu.getAttribute("aria-expanded")).toBe("true");
    const panel = host.querySelector(".filter-menu-panel")!;
    expect(panel.querySelectorAll('input[type="checkbox"]')).toHaveLength(3);
    const radios = [...panel.querySelectorAll<HTMLInputElement>('input[type="radio"]')];
    expect(radios.length).toBeGreaterThan(1);
    expect(radios.filter((r) => r.checked)).toHaveLength(1);

    // Sort: the other order puts the other model first.
    const first = () => host.querySelector("[role=option] .catalog-model-id")?.textContent;
    expect(first()).toBe("claude-new");
    const other = radios.find((r) => !r.checked && r.value !== "newest")!;
    await click(other);
    expect(radios.filter((r) => r.checked)).toHaveLength(1);

    // A filter narrows the list and puts a marker on the button.
    const reasoning = [...panel.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')].find(
      (c) => c.parentElement?.textContent?.includes("Reasoning"),
    )!;
    await click(reasoning);
    expect(host.querySelectorAll("[role=option]")).toHaveLength(0);
    expect(menu.querySelector("[aria-label='filters on']")).not.toBeNull();

    // Escape closes only the popover and returns focus to the button.
    await key(reasoning, "Escape");
    expect(host.querySelector(".filter-menu-panel")).toBeNull();
    expect(document.activeElement).toBe(menu);
    expect(menu.getAttribute("aria-expanded")).toBe("false");
  });

  it("keeps the typed-name row to one input and one button, with the help as placeholder", async () => {
    fakeCore();
    await openPicker();
    const row = host.querySelector(".picker-typed-row")!;
    expect(row.querySelectorAll("input")).toHaveLength(1);
    expect(row.querySelectorAll("button")).toHaveLength(1);
    expect(row.querySelector("input")?.getAttribute("placeholder")).toContain("model name");
  });
});
