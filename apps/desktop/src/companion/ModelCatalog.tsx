// Browse models from models.dev (decision 32). Nothing here contacts the network on its own:
// the list is requested only when the user presses Load or Refresh, through the Rust core, and
// it lives in the core's memory for this app run. Choosing a model only fills the model field.

import { type KeyboardEvent, useEffect, useId, useMemo, useState } from "react";
import { Banner } from "../ui/Banner";
import {
  companionCatalogModels,
  companionModelCatalog,
  toCommandError,
  type CatalogMode,
  type CatalogModel,
  type ModelCatalog,
  type ProviderKind,
} from "./api";
import {
  CATALOG_DISCLAIMER,
  CATALOG_LOAD_LABEL,
  CATALOG_MAX_ROWS,
  CATALOG_PROVIDER,
  CATALOG_SENDS_TEXT,
  CATALOG_SORTS,
  catalogCountText,
  catalogErrorText,
  countOf,
  DEFAULT_CATALOG_FILTERS,
  filterModels,
  filterProviders,
  loadedAtText,
  modelFacts,
  providerKindLabel,
  type CatalogFilters,
  type CatalogSort,
} from "./logic";

/** Enter in a search box must not submit the provider form around it. */
function keepEnterInField(e: KeyboardEvent<HTMLInputElement>) {
  if (e.key === "Enter") e.preventDefault();
}

/** The filter controls. One row when they fit, wrapping otherwise. */
export function CatalogFilterBar({
  filters,
  onChange,
}: {
  filters: CatalogFilters;
  onChange: (f: CatalogFilters) => void;
}) {
  const id = useId();
  const set = (patch: Partial<CatalogFilters>) => onChange({ ...filters, ...patch });
  return (
    <div className="catalog-filters" role="group" aria-label="Filter models">
      <label className="catalog-search">
        <span className="field-label">Search models</span>
        <input
          className="field"
          type="search"
          autoComplete="off"
          spellCheck={false}
          placeholder="Name, id or family"
          value={filters.text}
          onChange={(e) => set({ text: e.target.value })}
          onKeyDown={keepEnterInField}
        />
      </label>
      <label className="check-row">
        <input
          type="checkbox"
          checked={filters.tools}
          onChange={(e) => set({ tools: e.target.checked })}
        />
        Supports tools
      </label>
      <label className="check-row">
        <input
          type="checkbox"
          checked={filters.reasoning}
          onChange={(e) => set({ reasoning: e.target.checked })}
        />
        Reasoning
      </label>
      <label className="check-row">
        <input
          type="checkbox"
          checked={filters.hideOld}
          onChange={(e) => set({ hideOld: e.target.checked })}
        />
        Hide models older than a year
      </label>
      <label className="catalog-sort" htmlFor={`${id}-sort`}>
        <span className="field-label">Sort</span>
        <select
          id={`${id}-sort`}
          className="field"
          value={filters.sort}
          onChange={(e) => set({ sort: e.target.value as CatalogSort })}
        >
          {CATALOG_SORTS.map((s) => (
            <option key={s.value} value={s.value}>
              {s.label}
            </option>
          ))}
        </select>
      </label>
    </div>
  );
}

/** The filtered models as a list of buttons, with a polite live count. Pure: no network. */
export function CatalogModelList({
  models,
  filters,
  now,
  chosen,
  onChoose,
}: {
  models: readonly CatalogModel[];
  filters: CatalogFilters;
  now: Date;
  chosen: string;
  onChoose: (id: string) => void;
}) {
  const shown = useMemo(() => filterModels(models, filters, now), [models, filters, now]);
  const rows = shown.slice(0, CATALOG_MAX_ROWS);
  return (
    <>
      <p className="catalog-count" role="status" aria-live="polite">
        {catalogCountText(shown.length, models.length)}
        {shown.length > rows.length &&
          `. Showing the first ${rows.length}; search to narrow the list.`}
      </p>
      {rows.length > 0 && (
        <ul className="catalog-list" aria-label="Models from models.dev">
          {rows.map((m) => {
            const isChosen = m.id === chosen;
            return (
              <li key={m.id}>
                <button
                  type="button"
                  className={isChosen ? "catalog-model catalog-model-chosen" : "catalog-model"}
                  aria-pressed={isChosen}
                  onClick={() => onChoose(m.id)}
                >
                  <span className="catalog-model-name">{m.name}</span>{" "}
                  <code className="catalog-model-id">{m.id}</code>
                  <span className="catalog-model-facts">{modelFacts(m)}</span>
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </>
  );
}

type Busy = "load" | "refresh" | null;

/**
 * The models.dev browser shown under the model field. `kind` picks which list: Anthropic and
 * OpenAI map to models.dev's own providers, a compatible server picks one by name or id.
 */
export function ModelCatalogBrowser({
  kind,
  chosen,
  onChoose,
}: {
  kind: ProviderKind;
  chosen: string;
  onChoose: (id: string) => void;
}) {
  const id = useId();
  const [catalog, setCatalog] = useState<ModelCatalog | null>(null);
  const [busy, setBusy] = useState<Busy>(null);
  const [error, setError] = useState<string | null>(null);
  const [pickedProvider, setPickedProvider] = useState("");
  const [providerQuery, setProviderQuery] = useState("");
  const [fetched, setFetched] = useState<{
    key: string;
    models: CatalogModel[] | null;
    error: string | null;
  } | null>(null);
  const [filters, setFilters] = useState<CatalogFilters>(DEFAULT_CATALOG_FILTERS);
  const [announce, setAnnounce] = useState("");
  const [now] = useState(() => new Date());

  const fixed = CATALOG_PROVIDER[kind];
  const providerId = fixed ?? pickedProvider;

  // Memory only: shows a list already loaded earlier in this app run. Never fetches.
  useEffect(() => {
    let live = true;
    companionModelCatalog("cached")
      .then((c) => {
        if (live && c) setCatalog(c);
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, []);

  // Models of the chosen provider, read from the core's memory. The key changes with the
  // provider and with each load or refresh, so a refreshed list is read again.
  const listed = catalog?.providers.some((p) => p.id === providerId) ?? false;
  const modelsKey = catalog && listed ? `${catalog.fetchedAt}\n${providerId}` : null;
  useEffect(() => {
    if (!modelsKey) return;
    let live = true;
    companionCatalogModels(providerId)
      .then((m) => {
        if (live) setFetched({ key: modelsKey, models: m, error: null });
      })
      .catch((err: unknown) => {
        if (live)
          setFetched({
            key: modelsKey,
            models: null,
            error: catalogErrorText(toCommandError(err)),
          });
      });
    return () => {
      live = false;
    };
  }, [modelsKey, providerId]);
  const current = fetched?.key === modelsKey ? fetched : null;
  // null while reading; [] when the loaded list has no such provider.
  const models: CatalogModel[] | null = modelsKey === null ? [] : (current?.models ?? null);
  const modelsError = current?.error ?? null;

  const load = async (mode: Exclude<CatalogMode, "cached">) => {
    setBusy(mode);
    setError(null);
    setAnnounce(mode === "refresh" ? "Refreshing the model list…" : "Loading the model list…");
    try {
      const c = await companionModelCatalog(mode);
      if (c) {
        setCatalog(c);
        setAnnounce(
          c.unchanged && !c.fromMemory
            ? "The model list hasn't changed."
            : `Model list loaded: ${countOf(c.providers.length, "provider")}.`,
        );
      }
    } catch (err) {
      setError(catalogErrorText(toCommandError(err)));
      setAnnounce("");
    } finally {
      setBusy(null);
    }
  };

  const providers = useMemo(
    () => (catalog ? filterProviders(catalog.providers, providerQuery) : []),
    [catalog, providerQuery],
  );
  const providerName =
    catalog?.providers.find((p) => p.id === providerId)?.name ??
    (fixed ? providerKindLabel(kind) : "");

  return (
    <section className="catalog" aria-labelledby={`${id}-h`}>
      <h4 className="catalog-title" id={`${id}-h`}>
        Browse models from models.dev
      </h4>
      <p className="field-note">{CATALOG_DISCLAIMER}</p>

      <div className="catalog-load">
        {catalog === null ? (
          <button
            type="button"
            className="btn btn-secondary"
            disabled={busy !== null}
            aria-describedby={`${id}-sends`}
            onClick={() => void load("load")}
          >
            {busy === "load" ? "Loading model list…" : CATALOG_LOAD_LABEL}
          </button>
        ) : (
          <>
            <span className="catalog-loaded">
              {loadedAtText(catalog)} · {countOf(catalog.providers.length, "provider")}
            </span>
            <button
              type="button"
              className="btn btn-secondary"
              disabled={busy !== null}
              aria-describedby={`${id}-sends`}
              onClick={() => void load("refresh")}
            >
              {busy === "refresh" ? "Refreshing…" : "Refresh"}
            </button>
          </>
        )}
        <span id={`${id}-sends`} className="field-note">
          {CATALOG_SENDS_TEXT}
        </span>
      </div>

      <p className="visually-hidden" role="status" aria-live="polite">
        {announce}
      </p>
      {error && (
        <Banner tone="danger">
          {error}
          {catalog
            ? ` Still showing the list from earlier (${loadedAtText(catalog).toLowerCase()}).`
            : " You can still type the model name yourself."}
        </Banner>
      )}

      {catalog && fixed === null && (
        <div className="catalog-provider">
          <label className="field-label" htmlFor={`${id}-pq`}>
            Find a models.dev provider
          </label>
          <input
            id={`${id}-pq`}
            className="field"
            type="search"
            autoComplete="off"
            spellCheck={false}
            placeholder="Name or id"
            value={providerQuery}
            onChange={(e) => setProviderQuery(e.target.value)}
            onKeyDown={keepEnterInField}
          />
          <label className="field-label" htmlFor={`${id}-pp`}>
            models.dev provider
          </label>
          <select
            id={`${id}-pp`}
            className="field"
            aria-describedby={`${id}-pp-hint`}
            value={providerId}
            onChange={(e) => setPickedProvider(e.target.value)}
          >
            <option value="">
              {providers.length === 0
                ? "No provider matches"
                : `Choose one of ${countOf(providers.length, "provider")}…`}
            </option>
            {providers.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name} ({p.id}, {countOf(p.modelCount, "model")})
              </option>
            ))}
          </select>
          <p id={`${id}-pp-hint`} className="field-note">
            This only picks which list to browse. Lullshift never fills in the base URL from
            models.dev; enter your server's address above.
          </p>
        </div>
      )}

      {catalog && providerId && (
        <div className="catalog-browse">
          {modelsError ? (
            <Banner tone="danger">{modelsError}</Banner>
          ) : models === null ? (
            <p className="field-note">Reading the list…</p>
          ) : models.length === 0 ? (
            <p className="field-note">
              The loaded list has no chat models for {providerName}. You can still type the model
              name yourself.
            </p>
          ) : (
            <>
              <CatalogFilterBar filters={filters} onChange={setFilters} />
              <CatalogModelList
                models={models}
                filters={filters}
                now={now}
                chosen={chosen}
                onChoose={(m) => {
                  onChoose(m);
                  setAnnounce(`Model set to ${m}.`);
                }}
              />
            </>
          )}
        </div>
      )}
    </section>
  );
}
