import { useId, useRef, useState, type KeyboardEvent, type RefObject } from "react";
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
}: Props) {
  const [tab, setTab] = useState<Tab>("live");
  const [query, setQuery] = useState("");
  const [tool, setTool] = useState("all");
  const [sort, setSort] = useState<HistorySort>({ key: "lastSeen", descending: true });

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

          <div
            role="tabpanel"
            id="panel-live"
            aria-labelledby="tab-live"
            hidden={tab !== "live"}
            className="board-wrap"
          >
            <div className="board">
              <Column
                title="Needs you"
                tone="attention"
                items={groups.needsYou}
                emptyText={filtering ? "No matches." : "Nothing needs you."}
                now={now}
                staleAfterSeconds={staleAfterSeconds}
              />
              <Column
                title="Working"
                tone="active"
                items={groups.working}
                emptyText={filtering ? "No matches." : "No agent is working."}
                now={now}
                staleAfterSeconds={staleAfterSeconds}
              />
            </div>
          </div>

          <div
            role="tabpanel"
            id="panel-history"
            aria-labelledby="tab-history"
            hidden={tab !== "history"}
            className="history-wrap"
          >
            <History
              items={sortHistory(groups.recent, sort)}
              sort={sort}
              onSort={setSort}
              filtering={filtering}
              onClear={clearFilters}
              now={now}
            />
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
}: {
  title: string;
  tone: "attention" | "active";
  items: SessionView[];
  emptyText: string;
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
}: {
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
            <tr key={`${s.provider}:${s.providerSessionId}`}>
              <td className="history-tool">{providerName(s.provider)}</td>
              <td className="history-id" title={s.providerSessionId}>
                {shortId(s.providerSessionId)}
              </td>
              <td>
                <StatePill view={stateView(s.state)} />
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

function SessionCard({
  s,
  now,
  staleAfterSeconds,
}: {
  s: SessionView;
  now: number;
  staleAfterSeconds: number;
}) {
  const view = stateView(s.state);
  const why = reasonText(s, staleAfterSeconds);
  return (
    <li className={`session tone-${view.tone}`}>
      <div className="session-row">
        <p className="session-name">{providerName(s.provider)}</p>
        <StatePill view={view} />
      </div>
      <div className="session-row">
        <p className="session-id" title={s.providerSessionId}>
          <span className="visually-hidden">Session </span>
          {shortId(s.providerSessionId)}
        </p>
        <time
          className="session-time"
          dateTime={s.lastReceivedUtc}
          title={new Date(s.lastReceivedUtc).toLocaleString()}
        >
          {relativeTime(s.lastReceivedUtc, now)}
        </time>
      </div>
      {why && <p className="session-reason">{why}</p>}
    </li>
  );
}
