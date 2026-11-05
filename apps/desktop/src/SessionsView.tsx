import {
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
  type KeyboardEvent,
  type RefObject,
} from "react";
import { setUiPrefs, uiPrefs, type SessionKey } from "./context/api";
import DetailPanel from "./context/DetailPanel";
import {
  decideLayout,
  groupedList,
  keyString,
  sameSession,
  showsReviewPill,
  viewKey,
} from "./context/panelLogic";
import {
  filterSessions,
  groupSessions,
  providerName,
  relativeTime,
  shortId,
  sortHistory,
  summarize,
  toolChoices,
  type HistorySort,
  type HistorySortKey,
} from "./format";
import { reasonText, stateView, type SessionView } from "./sessions";
import { Banner } from "./ui/Banner";
import { Icon } from "./ui/Icon";
import { Mascot } from "./ui/Mascot";
import { StatePill } from "./ui/StatePill";

interface Props {
  headingRef: RefObject<HTMLHeadingElement | null>;
  loading: boolean;
  coreError: boolean;
  hasLast: boolean;
  problems: string[];
  sessions: SessionView[];
  staleAfterSeconds: number;
  now: number;
  onConnect: () => void;
  /** Bumps on each sessions-changed event; the open detail panel re-reads on it. */
  changeTick: number;
  /** Re-reads the dashboard (after a delete or review). */
  onRefresh: () => void;
  onOpenSettings: () => void;
}

type Tab = "live" | "history";

export default function SessionsView({
  headingRef,
  loading,
  coreError,
  hasLast,
  problems,
  sessions,
  staleAfterSeconds,
  now,
  onConnect,
  changeTick,
  onRefresh,
  onOpenSettings,
}: Props) {
  const [tab, setTab] = useState<Tab>("live");
  const [query, setQuery] = useState("");
  const [tool, setTool] = useState("all");
  const [sort, setSort] = useState<HistorySort>({ key: "lastSeen", descending: true });

  const [selected, setSelected] = useState<SessionKey | null>(null);
  const [pinned, setPinned] = useState(true);
  const width = useWindowWidth();
  const returnTo = useRef<string | null>(null);

  // The pin preference is read once; if it can't be read, the panel stays pinned.
  useEffect(() => {
    uiPrefs()
      .then((p) => setPinned(p.detailPanelPinned))
      .catch(() => undefined);
  }, []);
  const togglePin = () => {
    const next = !pinned;
    setPinned(next);
    setUiPrefs({ detailPanelPinned: next }).catch(() => undefined);
  };

  const selectedView = selected
    ? sessions.find((s) => sameSession(viewKey(s), selected))
    : undefined;
  // If the selected session is gone from the list (deleted), no panel shows.
  const layout = decideLayout(selectedView !== undefined, pinned, width);
  const floating = layout.panel === "floating";

  const open = useCallback((s: SessionView) => setSelected(viewKey(s)), []);
  const close = useCallback(() => {
    setSelected((cur) => {
      returnTo.current = cur ? keyString(cur) : null;
      return null;
    });
  }, []);

  // Give focus back to the card or row that opened the panel.
  useEffect(() => {
    if (selected !== null || returnTo.current === null) return;
    const k = returnTo.current;
    returnTo.current = null;
    const found = [...document.querySelectorAll<HTMLElement>("[data-session-key]")].filter(
      (el) => el.dataset.sessionKey === k,
    );
    (found.find((el) => el.offsetParent !== null) ?? found[0] ?? headingRef.current)?.focus();
  }, [selected, headingRef]);

  // Esc closes the panel (unless a dialog or an inline edit is using it); a click outside closes
  // a floating one.
  useEffect(() => {
    if (!selected) return;
    const onKey = (e: globalThis.KeyboardEvent) => {
      if (e.key !== "Escape" || e.defaultPrevented) return;
      if (document.querySelector("dialog[open]")) return;
      close();
    };
    const onPointer = (e: PointerEvent) => {
      if (!floating || !(e.target instanceof Element)) return;
      if (e.target.closest("#session-detail, [data-session-key], dialog")) return;
      close();
    };
    document.addEventListener("keydown", onKey);
    document.addEventListener("pointerdown", onPointer);
    return () => {
      document.removeEventListener("keydown", onKey);
      document.removeEventListener("pointerdown", onPointer);
    };
  }, [selected, floating, close]);

  const summary = summarize(sessions);
  const filtering = query.trim() !== "" || tool !== "all";
  const groups = groupSessions(filterSessions(sessions, { query, tool }));
  const liveCount = groups.needsYou.length + groups.working.length;
  const showBoard = !loading && sessions.length > 0;
  const showEmpty = !loading && !coreError && sessions.length === 0;
  const clearFilters = () => {
    setQuery("");
    setTool("all");
  };

  return (
    <section aria-labelledby="sessions-heading" className="view view-fill">
      <h1 id="sessions-heading" ref={headingRef} tabIndex={-1}>
        Sessions
      </h1>

      {coreError && (
        <Banner tone="danger">
          Lullshift couldn't reach its core, so agent status is unknown.
          {hasLast ? " Showing the last list received." : ""}
        </Banner>
      )}
      {problems.map((problem) => (
        <Banner tone="danger" key={problem}>
          {problem}
        </Banner>
      ))}

      {loading && (
        <div className="skeletons" role="status">
          <span className="visually-hidden">Loading sessions…</span>
          <div className="skeleton" />
          <div className="skeleton skeleton-hero" />
        </div>
      )}

      {showEmpty && (
        <div className="empty">
          <Mascot face="watching" size={112} />
          <h2>No agent sessions yet</h2>
          <p>
            Connect Claude Code and your sessions appear here as they work. Lullshift tells you when
            one needs you, so you can step away in between.
          </p>
          <button type="button" className="btn btn-primary" onClick={onConnect}>
            Connect Claude Code
            <Icon name="arrow" size={18} />
          </button>
        </div>
      )}

      {showBoard && (
        <>
          <div className="board-bar">
            <Tabs
              tab={tab}
              onChange={setTab}
              counts={{ live: liveCount, history: groups.recent.length }}
            />
            <input
              type="search"
              className="board-search"
              placeholder="Search…"
              aria-label="Search sessions by ID or tool"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
            <div className="chips" role="group" aria-label="Filter by tool">
              {["all", ...toolChoices(sessions)].map((t) => (
                <button
                  key={t}
                  type="button"
                  className="chip"
                  aria-pressed={tool === t}
                  onClick={() => setTool(t)}
                >
                  {t === "all" ? "All tools" : providerName(t)}
                </button>
              ))}
            </div>
          </div>

          <p className={`summary-line summary-${summary.tone}`} role="status" aria-live="polite">
            <span className="summary-dot" aria-hidden="true" />
            <strong className="summary-title">{summary.title}</strong>
            <span className="summary-detail">
              <span aria-hidden="true">· </span>
              {summary.detail}
            </span>
          </p>

          <div className={`board-stage${layout.panel === "docked" ? " has-dock" : ""}`}>
            <div className="stage-main">
              <div
                role="tabpanel"
                id="panel-live"
                aria-labelledby="tab-live"
                hidden={tab !== "live"}
                className="board-wrap"
              >
                {layout.board === "grouped-list" ? (
                  <GroupedList
                    sorted={[...groups.needsYou, ...groups.working]}
                    filtering={filtering}
                    selected={selected}
                    onOpen={open}
                    now={now}
                  />
                ) : (
                  <div className="board">
                    <Column
                      title="Needs you"
                      tone="attention"
                      items={groups.needsYou}
                      emptyText={filtering ? "No matches." : "Nothing needs you."}
                      now={now}
                      staleAfterSeconds={staleAfterSeconds}
                      selected={selected}
                      onOpen={open}
                    />
                    <Column
                      title="Working"
                      tone="active"
                      items={groups.working}
                      emptyText={filtering ? "No matches." : "No agent is working."}
                      now={now}
                      staleAfterSeconds={staleAfterSeconds}
                      selected={selected}
                      onOpen={open}
                    />
                  </div>
                )}
              </div>

              <div
                role="tabpanel"
                id="panel-history"
                aria-labelledby="tab-history"
                hidden={tab !== "history"}
                className="history-wrap"
              >
                {layout.board === "grouped-list" ? (
                  // Next to a docked panel the table has no room; show the same compact rows.
                  <section aria-label="History" className="column grouped-scroll">
                    <div className="column-scroll">
                      <CompactRows
                        items={sortHistory(groups.recent, sort)}
                        empty={filtering ? "No matches." : "No finished sessions yet."}
                        selected={selected}
                        onOpen={open}
                        now={now}
                      />
                    </div>
                  </section>
                ) : (
                  <History
                    items={sortHistory(groups.recent, sort)}
                    sort={sort}
                    onSort={setSort}
                    filtering={filtering}
                    onClear={clearFilters}
                    now={now}
                    selected={selected}
                    onOpen={open}
                  />
                )}
              </div>
            </div>
            {selectedView && selected && (
              <DetailPanel
                key={keyString(selected)}
                session={selectedView}
                sessions={sessions}
                now={now}
                staleAfterSeconds={staleAfterSeconds}
                tick={changeTick}
                pinned={pinned}
                floating={floating}
                onTogglePin={togglePin}
                onClose={close}
                onOpenSession={setSelected}
                onOpenSettings={onOpenSettings}
                onChanged={onRefresh}
                onDeleted={() => {
                  close();
                  onRefresh();
                }}
              />
            )}
          </div>
        </>
      )}
    </section>
  );
}

const TABS: { id: Tab; label: string }[] = [
  { id: "live", label: "Live" },
  { id: "history", label: "History" },
];

/** ARIA tabs: arrow keys, Home and End move between tabs and select them. */
function Tabs({
  tab,
  onChange,
  counts,
}: {
  tab: Tab;
  onChange: (t: Tab) => void;
  counts: Record<Tab, number>;
}) {
  const refs = useRef<Partial<Record<Tab, HTMLButtonElement | null>>>({});
  const onKey = (e: KeyboardEvent) => {
    const i = TABS.findIndex((t) => t.id === tab);
    const next =
      e.key === "ArrowRight"
        ? TABS[(i + 1) % TABS.length]
        : e.key === "ArrowLeft"
          ? TABS[(i - 1 + TABS.length) % TABS.length]
          : e.key === "Home"
            ? TABS[0]
            : e.key === "End"
              ? TABS[TABS.length - 1]
              : undefined;
    if (!next) return;
    e.preventDefault();
    onChange(next.id);
    refs.current[next.id]?.focus();
  };
  return (
    <div className="tabs" role="tablist" aria-label="Sessions" onKeyDown={onKey}>
      {TABS.map(({ id, label }) => (
        <button
          key={id}
          ref={(el) => {
            refs.current[id] = el;
          }}
          type="button"
          role="tab"
          id={`tab-${id}`}
          aria-selected={tab === id}
          aria-controls={`panel-${id}`}
          tabIndex={tab === id ? 0 : -1}
          className="tab"
          onClick={() => onChange(id)}
        >
          {label}
          <span className="tab-count">
            <span aria-hidden="true"> · </span>
            <span className="visually-hidden">, </span>
            {counts[id]}
          </span>
        </button>
      ))}
    </div>
  );
}

function Column({
  title,
  tone,
  items,
  emptyText,
  now,
  staleAfterSeconds,
  selected,
  onOpen,
}: {
  title: string;
  tone: "attention" | "active";
  items: SessionView[];
  emptyText: string;
  selected: SessionKey | null;
  onOpen: (s: SessionView) => void;
} & Pick<Props, "now" | "staleAfterSeconds">) {
  const id = useId();
  return (
    <section
      aria-labelledby={id}
      className={`column column-${tone}${items.length === 0 ? " column-empty" : ""}`}
    >
      <h2 id={id} className="column-title">
        <span className="column-dot" aria-hidden="true" />
        {title}
        <span aria-hidden="true"> · </span>
        <span className="visually-hidden">, </span>
        {items.length}
      </h2>
      {/* Scrolls on its own so the other column stays in view. */}
      <div className="column-scroll">
        {items.length === 0 ? (
          <p className="column-none">{emptyText}</p>
        ) : (
          <ul className="sessions">
            {items.map((s) => (
              <SessionCard
                key={`${s.provider}:${s.providerSessionId}`}
                s={s}
                now={now}
                staleAfterSeconds={staleAfterSeconds}
                selected={sameSession(viewKey(s), selected)}
                onOpen={onOpen}
              />
            ))}
          </ul>
        )}
      </div>
    </section>
  );
}

const HISTORY_COLUMNS: { key: HistorySortKey; label: string }[] = [
  { key: "tool", label: "Tool" },
  { key: "session", label: "Session" },
  { key: "state", label: "State" },
  { key: "lastSeen", label: "Last seen" },
];

function History({
  items,
  sort,
  onSort,
  filtering,
  onClear,
  now,
  selected,
  onOpen,
}: {
  selected: SessionKey | null;
  onOpen: (s: SessionView) => void;
  items: SessionView[];
  sort: HistorySort;
  onSort: (s: HistorySort) => void;
  filtering: boolean;
  onClear: () => void;
  now: number;
}) {
  if (items.length === 0) {
    return (
      <div className="history-none">
        <p>{filtering ? "No past sessions match." : "Finished and idle sessions show here."}</p>
        {filtering && (
          <button type="button" className="btn btn-quiet" onClick={onClear}>
            Clear search and filters
          </button>
        )}
      </div>
    );
  }
  return (
    <div className="history-scroll">
      <table className="history">
        <caption className="visually-hidden">Past sessions</caption>
        <thead>
          <tr>
            {HISTORY_COLUMNS.map(({ key, label }) => {
              const active = sort.key === key;
              return (
                <th
                  key={key}
                  scope="col"
                  aria-sort={active ? (sort.descending ? "descending" : "ascending") : "none"}
                >
                  <button
                    type="button"
                    className="sort"
                    onClick={() =>
                      onSort({ key, descending: active ? !sort.descending : key === "lastSeen" })
                    }
                  >
                    {label}
                    <span className="sort-arrow" aria-hidden="true">
                      {active ? (sort.descending ? "↓" : "↑") : ""}
                    </span>
                  </button>
                </th>
              );
            })}
          </tr>
        </thead>
        <tbody>
          {items.map((s) => (
            <tr
              key={`${s.provider}:${s.providerSessionId}`}
              className={`history-row${sameSession(viewKey(s), selected) ? " is-selected" : ""}`}
              onClick={() => onOpen(s)}
            >
              <td className="history-tool">
                <button
                  type="button"
                  className="row-open"
                  data-session-key={keyString(viewKey(s))}
                  aria-expanded={sameSession(viewKey(s), selected)}
                  aria-controls="session-detail"
                  onClick={(e) => {
                    e.stopPropagation();
                    onOpen(s);
                  }}
                >
                  {providerName(s.provider)}
                </button>
              </td>
              <td className="history-id" title={s.providerSessionId}>
                {shortId(s.providerSessionId)}
              </td>
              <td>
                <StatePill view={stateView(s.state)} />
                {showsReviewPill(s) && <ReviewPill />}
              </td>
              <td>
                <time
                  className="session-time"
                  dateTime={s.lastReceivedUtc}
                  title={new Date(s.lastReceivedUtc).toLocaleString()}
                >
                  {relativeTime(s.lastReceivedUtc, now)}
                </time>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function ReviewPill() {
  return (
    <span className="pill pill-review" title="This session ended and hasn't been reviewed">
      Review
    </span>
  );
}

function useWindowWidth() {
  const [width, setWidth] = useState(() => window.innerWidth);
  useEffect(() => {
    const onResize = () => setWidth(window.innerWidth);
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);
  return width;
}

/** One line per session: tool and time, then the short ID. Used next to a docked panel. */
function CompactRows({
  items,
  empty,
  selected,
  onOpen,
  now,
}: {
  items: SessionView[];
  empty: string;
  selected: SessionKey | null;
  onOpen: (s: SessionView) => void;
  now: number;
}) {
  if (items.length === 0) return <p className="column-none">{empty}</p>;
  return (
    <ul className="compact-rows">
      {items.map((s) => (
        <li key={keyString(viewKey(s))}>
          <button
            type="button"
            className={`compact-row tone-${stateView(s.state).tone}${
              sameSession(viewKey(s), selected) ? " is-selected" : ""
            }`}
            data-session-key={keyString(viewKey(s))}
            aria-expanded={sameSession(viewKey(s), selected)}
            aria-controls="session-detail"
            onClick={() => onOpen(s)}
          >
            <span className="compact-dot" aria-hidden="true" />
            <span className="compact-name">{providerName(s.provider)}</span>
            <span className="compact-time session-time">
              {relativeTime(s.lastReceivedUtc, now)}
            </span>
            <span className="compact-meta">
              <span className="session-id">
                <span className="visually-hidden">Session </span>
                {shortId(s.providerSessionId)}
              </span>
              <span className="visually-hidden">, {stateView(s.state).label}</span>
              {showsReviewPill(s) && <ReviewPill />}
            </span>
          </button>
        </li>
      ))}
    </ul>
  );
}

/** One list for narrow windows with a docked panel: Needs you pinned on top, Working below. */
function GroupedList({
  sorted,
  filtering,
  selected,
  onOpen,
  now,
}: {
  sorted: SessionView[];
  filtering: boolean;
  selected: SessionKey | null;
  onOpen: (s: SessionView) => void;
  now: number;
}) {
  const needsId = useId();
  const workId = useId();
  const { pinned, scrolling } = groupedList(sorted);
  const rows = (items: SessionView[], empty: string) => (
    <CompactRows
      items={items}
      empty={filtering ? "No matches." : empty}
      selected={selected}
      onOpen={onOpen}
      now={now}
    />
  );
  return (
    <div className="grouped">
      <section aria-labelledby={needsId} className="column column-attention grouped-pinned">
        <h2 id={needsId} className="column-title">
          <span className="column-dot" aria-hidden="true" />
          Needs you
          <span aria-hidden="true"> · </span>
          <span className="visually-hidden">, </span>
          {pinned.length}
        </h2>
        {rows(pinned, "Nothing needs you.")}
      </section>
      <section aria-labelledby={workId} className="column column-active grouped-scroll">
        <h2 id={workId} className="column-title">
          <span className="column-dot" aria-hidden="true" />
          Working
          <span aria-hidden="true"> · </span>
          <span className="visually-hidden">, </span>
          {scrolling.length}
        </h2>
        <div className="column-scroll">{rows(scrolling, "No agent is working.")}</div>
      </section>
    </div>
  );
}

function SessionCard({
  s,
  now,
  staleAfterSeconds,
  selected,
  onOpen,
}: {
  s: SessionView;
  now: number;
  staleAfterSeconds: number;
  selected: boolean;
  onOpen: (s: SessionView) => void;
}) {
  const view = stateView(s.state);
  const why = reasonText(s, staleAfterSeconds);
  return (
    <li>
      <button
        type="button"
        className={`session tone-${view.tone}${selected ? " is-selected" : ""}`}
        data-session-key={keyString(viewKey(s))}
        aria-expanded={selected}
        aria-controls="session-detail"
        onClick={() => onOpen(s)}
      >
        <span className="session-row">
          <span className="session-name">{providerName(s.provider)}</span>
          <StatePill view={view} />
        </span>
        <span className="session-row">
          <span className="session-id" title={s.providerSessionId}>
            <span className="visually-hidden">Session </span>
            {shortId(s.providerSessionId)}
          </span>
          <span className="session-row">
            {showsReviewPill(s) && <ReviewPill />}
            <time
              className="session-time"
              dateTime={s.lastReceivedUtc}
              title={new Date(s.lastReceivedUtc).toLocaleString()}
            >
              {relativeTime(s.lastReceivedUtc, now)}
            </time>
          </span>
        </span>
        {why && <span className="session-reason">{why}</span>}
      </button>
    </li>
  );
}
