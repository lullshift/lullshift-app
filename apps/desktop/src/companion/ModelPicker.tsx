// The model picker (decisions 32 and 36): a modal that lists models from models.dev and lets the
// user type any other model name.
//
// Network rule: opening the picker counts as asking for the list. It is mounted only while open;
// on mount it reads the core's memory, and only if nothing is there yet does it ask the core to
// load the list (one request to models.dev). Later opens in the same app run use the list in
// memory. Refresh is the only other path that contacts models.dev. Choosing a model sends nothing.

import {
  type KeyboardEvent,
  type RefObject,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
} from "react";
import { Banner } from "../ui/Banner";
import { Dialog } from "../ui/Dialog";
import {
  companionCatalogModels,
  companionModelCatalog,
  toCommandError,
  type CatalogModel,
  type ModelCatalog,
  type ProviderKind,
} from "./api";
import {
  CATALOG_DISCLAIMER,
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
import { modelIdProblem } from "./modelChoice";

/** Enter in a search box must not submit a form around it. */
function keepEnterInField(e: KeyboardEvent<HTMLInputElement>) {
  if (e.key === "Enter") e.preventDefault();
}

/** The filter controls. `onArrowDown` moves focus from the search box into the results. */
export function CatalogFilterBar({
  filters,
  onChange,
  onSearchKey,
  searchRef,
  listId,
}: {
  filters: CatalogFilters;
  onChange: (f: CatalogFilters) => void;
  onSearchKey?: (e: KeyboardEvent<HTMLInputElement>) => void;
  searchRef?: RefObject<HTMLInputElement | null>;
  listId?: string;
}) {
  const id = useId();
  const set = (patch: Partial<CatalogFilters>) => onChange({ ...filters, ...patch });
  return (
    <div className="catalog-filters" role="group" aria-label="Filter models">
      <label className="catalog-search">
        <span className="field-label">Search models</span>
        <input
          ref={searchRef}
          className="field"
          type="search"
          autoComplete="off"
          spellCheck={false}
          placeholder="Name, id or family"
          aria-controls={listId}
          value={filters.text}
          onChange={(e) => set({ text: e.target.value })}
          onKeyDown={(e) => {
            onSearchKey?.(e);
            keepEnterInField(e);
          }}
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

/**
 * The filtered models as a single-select listbox. Arrow keys, Home and End move; Enter or Space
 * chooses. Pure: no network.
 */
export function CatalogModelList({
  models,
  filters,
  now,
  chosen,
  onChoose,
  listId,
  listRef,
  active,
  onActive,
}: {
  models: readonly CatalogModel[];
  filters: CatalogFilters;
  now: Date;
  chosen: string;
  onChoose: (m: CatalogModel) => void;
  listId?: string;
  listRef?: RefObject<HTMLUListElement | null>;
  /** Index of the highlighted row. */
  active?: number;
  onActive?: (i: number) => void;
}) {
  const ownId = useId();
  const id = listId ?? ownId;
  const shown = useMemo(() => filterModels(models, filters, now), [models, filters, now]);
  const rows = shown.slice(0, CATALOG_MAX_ROWS);
  const at = Math.min(Math.max(active ?? 0, 0), Math.max(rows.length - 1, 0));
  const move = (i: number) => {
    const next = Math.min(Math.max(i, 0), rows.length - 1);
    onActive?.(next);
    document.getElementById(`${id}-opt-${next}`)?.scrollIntoView?.({ block: "nearest" });
  };
  const onKey = (e: KeyboardEvent<HTMLUListElement>) => {
    if (rows.length === 0) return;
    const keys: Record<string, () => void> = {
      ArrowDown: () => move(at + 1),
      ArrowUp: () => move(at - 1),
      Home: () => move(0),
      End: () => move(rows.length - 1),
      PageDown: () => move(at + 10),
      PageUp: () => move(at - 10),
      Enter: () => {
        const m = rows[at];
        if (m) onChoose(m);
      },
      " ": () => {
        const m = rows[at];
        if (m) onChoose(m);
      },
    };
    const run = keys[e.key];
    if (run) {
      e.preventDefault();
      run();
    }
  };
  return (
    <>
      <p className="catalog-count" role="status" aria-live="polite">
        {catalogCountText(shown.length, models.length)}
        {shown.length > rows.length &&
          `. Showing the first ${rows.length}; search to narrow the list.`}
      </p>
      {rows.length > 0 && (
        <ul
          id={id}
          ref={listRef}
          className="catalog-list"
          role="listbox"
          tabIndex={0}
          aria-label="Models from models.dev"
          aria-activedescendant={`${id}-opt-${at}`}
          onKeyDown={onKey}
        >
          {rows.map((m, i) => {
            const isChosen = m.id === chosen;
            const cls = ["catalog-model"];
            if (isChosen) cls.push("catalog-model-chosen");
            if (i === at) cls.push("catalog-model-active");
            return (
              <li
                key={m.id}
                id={`${id}-opt-${i}`}
                role="option"
                aria-selected={isChosen}
                className={cls.join(" ")}
                title={[m.name, m.id, modelFacts(m)].filter(Boolean).join(" · ")}
                onMouseEnter={() => onActive?.(i)}
                onClick={() => onChoose(m)}
              >
                <span className="catalog-model-line">
                  {m.name !== null && (
                    <>
                      <span className="catalog-model-name">{m.name}</span>{" "}
                    </>
                  )}
                  <code className="catalog-model-id">{m.id}</code>
                </span>
                <span className="catalog-model-facts">{modelFacts(m)}</span>
              </li>
            );
          })}
        </ul>
      )}
    </>
  );
}

type Busy = "checking" | "load" | "refresh" | null;

/** What a pick hands back: the model id and the models.dev name, if it had one. */
export interface PickedModel {
  model: string;
  name: string | null;
}

/**
 * The model picker dialog. Render it only while it is open (it loads on mount). `kind` picks
 * which list: Anthropic and OpenAI map to models.dev's own providers; a compatible server picks
 * one by name or id.
 */
export function ModelPickerDialog({
  kind,
  title,
  chosen,
  onChoose,
  onClose,
}: {
  kind: ProviderKind;
  title: string;
  chosen: string;
  onChoose: (picked: PickedModel) => void;
  onClose: () => void;
}) {
  const id = useId();
  const listId = `${id}-list`;
  const [catalog, setCatalog] = useState<ModelCatalog | null>(null);
  const [busy, setBusy] = useState<Busy>("checking");
  const [error, setError] = useState<string | null>(null);
  const [pickedProvider, setPickedProvider] = useState("");
  const [providerQuery, setProviderQuery] = useState("");
  const [fetched, setFetched] = useState<{
    key: string;
    models: CatalogModel[] | null;
    error: string | null;
  } | null>(null);
  const [filters, setFilters] = useState<CatalogFilters>(DEFAULT_CATALOG_FILTERS);
  const [active, setActive] = useState(0);
  const [typed, setTyped] = useState("");
  const [typedProblem, setTypedProblem] = useState<string | null>(null);
  const [announce, setAnnounce] = useState("Loading the model list…");
  const [now] = useState(() => new Date());
  const searchRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLUListElement>(null);
  const opener = useRef<Element | null>(null);

  const fixed = CATALOG_PROVIDER[kind];
  const providerId = fixed ?? pickedProvider;

  // Focus goes back to whatever opened the picker.
  useEffect(() => {
    opener.current = document.activeElement;
    return () => {
      const el = opener.current;
      if (el instanceof HTMLElement && el.isConnected) el.focus();
    };
  }, []);

  // Opening the picker is the user's request for the list: memory first, then one load if the
  // list isn't in memory yet. State changes only in the promise callbacks.
  useEffect(() => {
    let live = true;
    companionModelCatalog("cached")
      .then((c) => c ?? companionModelCatalog("load"))
      .then((c) => {
        if (!live) return;
        setCatalog(c);
        setBusy(null);
        setAnnounce(c ? `Model list ready: ${countOf(c.providers.length, "provider")}.` : "");
      })
      .catch((err: unknown) => {
        if (!live) return;
        setBusy(null);
        setError(catalogErrorText(toCommandError(err)));
        setAnnounce("");
      });
    return () => {
      live = false;
    };
  }, []);

  const listed = catalog?.providers.some((p) => p.id === providerId) ?? false;
  const modelsKey = catalog && listed ? `${catalog.loadedAt}\n${providerId}` : null;
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
  const models: CatalogModel[] | null = modelsKey === null ? [] : (current?.models ?? null);
  const modelsError = current?.error ?? null;

  const refresh = async () => {
    setBusy("refresh");
    setError(null);
    setAnnounce("Refreshing the model list…");
    try {
      const c = await companionModelCatalog("refresh");
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

  const choose = (m: CatalogModel) => onChoose({ model: m.id, name: m.name });

  const applyTyped = () => {
    const problem = modelIdProblem(typed);
    setTypedProblem(problem);
    if (!problem) onChoose({ model: typed.trim(), name: null });
  };

  const onSearchKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "ArrowDown" && listRef.current) {
      e.preventDefault();
      listRef.current.focus();
    } else if (e.key === "Enter" && models && models.length > 0 && listRef.current) {
      // Enter in the search box chooses the highlighted model.
      const rows = filterModels(models, filters, now).slice(0, CATALOG_MAX_ROWS);
      const m = rows[Math.min(active, rows.length - 1)];
      if (m) choose(m);
    }
  };

  return (
    <Dialog
      open
      title={title}
      busy={false}
      onClose={onClose}
      actions={
        <button type="button" className="btn btn-quiet" onClick={onClose}>
          Cancel
        </button>
      }
      description={
        <>
          <p id={`${id}-sends`}>{CATALOG_SENDS_TEXT}</p>
          <p className="field-note">{CATALOG_DISCLAIMER}</p>
        </>
      }
    >
      <div className="picker">
        <div className="catalog-load">
          {busy === "checking" || busy === "load" ? (
            <span className="catalog-loaded">Loading the model list from models.dev…</span>
          ) : catalog ? (
            <span className="catalog-loaded">
              {loadedAtText(catalog)} · {countOf(catalog.providers.length, "provider")}
            </span>
          ) : (
            <span className="catalog-loaded">The model list isn't loaded.</span>
          )}
          <button
            type="button"
            className="btn btn-secondary"
            disabled={busy !== null}
            aria-describedby={`${id}-sends`}
            onClick={() => void refresh()}
          >
            {busy === "refresh" ? "Refreshing…" : catalog ? "Refresh" : "Try again"}
          </button>
        </div>

        <p className="visually-hidden" role="status" aria-live="polite">
          {announce}
        </p>
        {error && (
          <Banner tone="danger">
            {error}
            {catalog
              ? ` Still showing the list from earlier (${loadedAtText(catalog).toLowerCase()}).`
              : " You can still type the model name below."}
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
              onChange={(e) => {
                setPickedProvider(e.target.value);
                setActive(0);
              }}
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
              models.dev.
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
                name below.
              </p>
            ) : (
              <>
                <CatalogFilterBar
                  filters={filters}
                  onChange={(f) => {
                    setFilters(f);
                    setActive(0);
                  }}
                  onSearchKey={onSearchKey}
                  searchRef={searchRef}
                  listId={listId}
                />
                <CatalogModelList
                  models={models}
                  filters={filters}
                  now={now}
                  chosen={chosen}
                  onChoose={choose}
                  listId={listId}
                  listRef={listRef}
                  active={active}
                  onActive={setActive}
                />
              </>
            )}
          </div>
        )}

        <div className="picker-typed">
          <label className="field-label" htmlFor={`${id}-typed`}>
            Use a model name not in the list
          </label>
          <div className="picker-typed-row">
            <input
              id={`${id}-typed`}
              className="field"
              type="text"
              autoComplete="off"
              spellCheck={false}
              aria-invalid={typedProblem ? true : undefined}
              aria-describedby={`${id}-typed-hint`}
              value={typed}
              onChange={(e) => setTyped(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  applyTyped();
                }
              }}
            />
            <button type="button" className="btn btn-secondary" onClick={applyTyped}>
              Use this name
            </button>
          </div>
          <p
            id={`${id}-typed-hint`}
            className={typedProblem ? "field-note field-error" : "field-note"}
          >
            {typedProblem ??
              "Type it exactly as the provider names it. Lullshift can't check it until you send or test."}
          </p>
        </div>
      </div>
    </Dialog>
  );
}
