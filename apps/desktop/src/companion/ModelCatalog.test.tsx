import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";

const invoke = vi.fn<(...args: unknown[]) => Promise<unknown>>(() => new Promise(() => undefined));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invoke(...args),
  Channel: class {},
}));

import type { CatalogModel } from "./api";
import {
  CATALOG_DISCLAIMER,
  CATALOG_LOAD_LABEL,
  CATALOG_MAX_ROWS,
  CATALOG_SENDS_TEXT,
  DEFAULT_CATALOG_FILTERS,
} from "./logic";
import { CatalogFilterBar, CatalogModelList, ModelCatalogBrowser } from "./ModelCatalog";

const NOW = new Date(Date.UTC(2026, 10, 7));

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

describe("ModelCatalogBrowser before anything is loaded", () => {
  it("offers the load button with the privacy note and the disclaimer, and sends nothing", () => {
    invoke.mockClear();
    const html = renderToStaticMarkup(
      <ModelCatalogBrowser kind="anthropic" chosen="" onChoose={() => undefined} />,
    );
    expect(html).toContain(CATALOG_LOAD_LABEL);
    expect(html).toContain(CATALOG_SENDS_TEXT);
    expect(html).toContain("may be out of date");
    expect(html).toContain(CATALOG_DISCLAIMER.replace(/'/g, "&#x27;"));
    // The button is a plain button so it never submits the profile form.
    expect(html).toMatch(/<button type="button"[^>]*aria-describedby="[^"]+-sends"/);
    // Rendering alone calls nothing (the memory-only check runs after mount, never a fetch).
    expect(invoke).not.toHaveBeenCalled();
  });

  it("shows no provider picker or list until a list is loaded", () => {
    const html = renderToStaticMarkup(
      <ModelCatalogBrowser kind="openaiCompatible" chosen="" onChoose={() => undefined} />,
    );
    expect(html).not.toContain("models.dev provider");
    expect(html).not.toContain("catalog-list");
  });
});

describe("CatalogModelList", () => {
  const models = [
    model("a-old", { releaseDate: "2023-05-01", toolCall: true }),
    model("b-new", { releaseDate: "2026-09-01", costInput: null, costOutput: null }),
  ];

  it("renders buttons with name, id and facts, and a polite live count", () => {
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
    expect(html).toContain("2 models");
    expect(html).toContain('<code class="catalog-model-id">a-old</code>');
    expect(html).toContain("price unknown");
    expect(html).toContain("supports tools");
    // Newest first, and the chosen one is marked for assistive technology.
    expect(html.indexOf("b-new")).toBeLessThan(html.indexOf("a-old"));
    expect(html).toMatch(/aria-pressed="true"[^>]*>.*b-new/);
    expect(html.match(/<button type="button"/g)).toHaveLength(2);
  });

  it("applies the filters and says when nothing matches", () => {
    const hidden = renderToStaticMarkup(
      <CatalogModelList
        models={models}
        filters={{ ...DEFAULT_CATALOG_FILTERS, hideOld: true }}
        now={NOW}
        chosen=""
        onChoose={() => undefined}
      />,
    );
    expect(hidden).toContain("1 of 2 models");
    expect(hidden).not.toContain("a-old");
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
    expect(none).not.toContain("<ul");
  });

  it("draws at most the row cap and says so", () => {
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
    expect(html.match(/<li>/g)).toHaveLength(CATALOG_MAX_ROWS);
    expect(html).toContain(`Showing the first ${CATALOG_MAX_ROWS}`);
  });
});

describe("CatalogFilterBar", () => {
  it("labels every control", () => {
    const html = renderToStaticMarkup(
      <CatalogFilterBar filters={DEFAULT_CATALOG_FILTERS} onChange={() => undefined} />,
    );
    for (const label of [
      "Search models",
      "Supports tools",
      "Reasoning",
      "Hide models older than a year",
      "Sort",
      "Newest first",
      "Cheapest input first",
    ]) {
      expect(html).toContain(label);
    }
    expect(html).toContain('role="group" aria-label="Filter models"');
  });
});
