import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState, type RefObject } from "react";
import { relativeTime } from "../format";
import { activitySourceLabel } from "../marketplace/logic";
import { ManagePanel } from "../marketplace/ManagePanel";
import { MarketplacesPanel } from "../marketplace/MarketplacesPanel";
import { Banner } from "../ui/Banner";
import { focusIfLost } from "../ui/focus";
import { Icon } from "../ui/Icon";
import { Mascot } from "../ui/Mascot";
import {
  ActivityHost,
  type ActivityInfo,
  type HostControl,
  type PauseRequest,
} from "./ActivityHost";
import { HelpDialog } from "./HelpDialog";
import {
  availableChips,
  filterLibrary,
  helpLabel,
  kindLabel,
  onPage,
  savedNewestFirst,
  type Page,
} from "./library";
import { ResumeCard } from "./ResumeCard";
import type { ActivityLoad } from "./useActivityList";
import { usePreview } from "./usePreview";

/** What the Activities slot shows: the two libraries, Continue, or managing what is installed. */
export type LibraryPage = Page | "continue";

type ManageTab = "manage" | "marketplaces";

const MANAGE_TABS: { id: ManageTab; label: string }[] = [
  { id: "manage", label: "Manage" },
  { id: "marketplaces", label: "Marketplaces" },
];

const COPY: Record<
  Page,
  { title: string; lede: string; search: string; all: string; none: string }
> = {
  play: {
    title: "Play",
    lede: "Short games that pause and save when an agent needs you.",
    search: "Search games",
    all: "All games",
    none: "No games are turned on.",
  },
  rest: {
    title: "Rest",
    lede: "Breathe, stretch, look away. Lullshift pauses and saves when an agent needs you.",
    search: "Search breaks",
    all: "All breaks",
    none: "No breaks are turned on.",
  },
};

interface Props {
  headingRef: RefObject<HTMLHeadingElement | null>;
  /** This view is showing. When it isn't, a running activity pauses. */
  active: boolean;
  page: LibraryPage;
  load: ActivityLoad;
  refresh: () => void;
  pause: PauseRequest | null;
  now: number;
  /** A start the user asked for elsewhere (Home, or a Companion suggestion they clicked). */
  launch?: { id: string; nonce: number; activity?: ActivityInfo } | null;
  /** Changes on every click of Play, Rest or Continue in the sidebar. */
  navNonce?: number;
  onGoToSessions: () => void;
  /** Open Play or Rest (also called once a launched activity's category is known). */
  onShowPage: (page: Page) => void;
}

/**
 * Play, Rest and Continue (ADR-026). One component serves all three so a running activity, its
 * pause and its paused card behave the same from any page, and stays mounted while hidden so a
 * running activity can save before it pauses.
 */
export default function ActivitiesView({
  headingRef,
  active,
  page,
  load,
  refresh,
  pause,
  now,
  launch,
  navNonce = 0,
  onGoToSessions,
  onShowPage,
}: Props) {
  // One launch of one activity. The host is keyed by it, so a new launch never inherits the
  // previous one's stats, pause bookkeeping or unsaved time.
  const [running, setRunning] = useState<{ activity: ActivityInfo; nonce: number } | null>(null);
  const runningRef = useRef(running);
  const launches = useRef(0);
  const beginSeq = useRef(0);
  const controlRef = useRef<HostControl | null>(null);
  // False while the list is showing over a paused (or pausing) launch, after a sidebar click.
  const [hostVisible, setHostVisible] = useState(true);
  const [seenNav, setSeenNav] = useState(navNonce);
  if (seenNav !== navNonce) {
    setSeenNav(navNonce);
    setHostVisible(false);
  }
  const [message, setMessage] = useState<string | null>(null);
  const [helpFor, setHelpFor] = useState<ActivityInfo | null>(null);
  const [manage, setManage] = useState<ManageTab | null>(null);
  const [filters, setFilters] = useState<Record<Page, { query: string; chip: string | null }>>({
    play: { query: "", chip: null },
    rest: { query: "", chip: null },
  });
  // Leaving the page you were managing from closes Manage (adjusting state while rendering).
  const [shownPage, setShownPage] = useState(page);
  if (shownPage !== page) {
    setShownPage(page);
    setManage(null);
    setFilters({ play: { query: "", chip: null }, rest: { query: "", chip: null } });
  }

  /**
   * Starts an activity. The same one that is already paused resumes in place (one click). A
   * different one waits until the current launch has paused and saved, then gets a host of its own.
   */
  const begin = useCallback(async (a: ActivityInfo) => {
    setMessage(null);
    setManage(null);
    // Only the latest click counts: a second click while the first waits for a pause wins.
    const seq = (beginSeq.current += 1);
    const current = runningRef.current;
    if (current && current.activity.id === a.id) {
      setHostVisible(true);
      controlRef.current?.resume();
      return;
    }
    if (current) {
      try {
        await controlRef.current?.pauseNow();
      } catch {
        // A pause that fails must not leave the new activity unable to start.
      }
      if (seq !== beginSeq.current) return;
    }
    launches.current += 1;
    const next = { activity: a, nonce: launches.current };
    runningRef.current = next;
    setRunning(next);
    setHostVisible(true);
  }, []);

  // Start the activity the user chose elsewhere, once per click. An id that is no longer
  // installed just leaves the page showing. The page then follows the activity's category.
  const lastLaunch = useRef(0);
  useEffect(() => {
    if (!launch || launch.nonce === lastLaunch.current) return;
    lastLaunch.current = launch.nonce;
    const go = (a: ActivityInfo) => {
      void begin(a);
      onShowPage(a.category === "break" ? "rest" : "play");
    };
    if (launch.activity) go(launch.activity);
    else
      invoke<ActivityInfo>("activity_info", { id: launch.id })
        .then(go)
        .catch(() => setMessage("That activity isn't available any more."));
  }, [launch, onShowPage, begin]);

  // Closing an activity removes the player, so focus would drop to the page: put it back on the
  // heading. Only when it was lost, and only after a real close.
  const wasRunning = useRef(false);
  useEffect(() => {
    if (wasRunning.current && !running) focusIfLost(() => headingRef.current);
    wasRunning.current = running !== null;
  }, [running, headingRef]);

  const startOver = async (a: ActivityInfo) => {
    setMessage(null);
    try {
      await invoke("activity_clear_snapshot", { id: a.id });
      refresh();
      setMessage(`${a.name} will start fresh next time.`);
    } catch {
      setMessage(`${a.name}'s saved progress couldn't be cleared.`);
    }
  };
  const start = (a: ActivityInfo) => void begin(a);

  const title = manage ? "Manage activities" : page === "continue" ? "Continue" : COPY[page].title;

  const showHost = running !== null && hostVisible;
  const hostSection = running && (
    <section
      aria-labelledby={showHost ? "activities-heading" : undefined}
      className="view view-fill"
      hidden={!showHost}
    >
      {showHost && (
        <h1 id="activities-heading" ref={headingRef} tabIndex={-1} className="visually-hidden">
          {title}
        </h1>
      )}
      <ActivityHost
        key={`${running.activity.id}:${running.nonce}`}
        activity={running.activity}
        active={active && hostVisible}
        pause={pause}
        controlRef={controlRef}
        onClose={() => {
          runningRef.current = null;
          setRunning(null);
          refresh();
        }}
        onGoToSessions={onGoToSessions}
      />
    </section>
  );
  if (showHost) return hostSection;

  const list = load.status === "ready" ? load.list : [];

  return (
    <>
      {hostSection}
      <section aria-labelledby="activities-heading" className="view">
        {running && (
          <div className="paused-bar" role="status">
            <span>{running.activity.name} is paused</span>
            <button type="button" className="link-btn" onClick={() => void begin(running.activity)}>
              Resume
              <span className="visually-hidden"> {running.activity.name}</span>
            </button>
          </div>
        )}
        {manage ? (
          <>
            <header className="view-header">
              <h1 id="activities-heading" ref={headingRef} tabIndex={-1}>
                {title}
              </h1>
              <p className="view-lede">
                Turn activities on or off, add more from a marketplace, or remove what you
                installed.
              </p>
            </header>
            <div className="manage-bar">
              <nav aria-label="Manage sections">
                <ul className="subtabs">
                  {MANAGE_TABS.map((t) => (
                    <li key={t.id}>
                      <button
                        type="button"
                        className="subtab"
                        aria-current={manage === t.id ? "page" : undefined}
                        onClick={() => setManage(t.id)}
                      >
                        {t.label}
                      </button>
                    </li>
                  ))}
                </ul>
              </nav>
              <button
                type="button"
                className="btn btn-quiet"
                onClick={() => {
                  setManage(null);
                  refresh();
                }}
              >
                Back to {page === "continue" ? "Continue" : COPY[page].title}
              </button>
            </div>
            {manage === "manage" && <ManagePanel onChanged={refresh} />}
            {manage === "marketplaces" && <MarketplacesPanel now={now} onChanged={refresh} />}
          </>
        ) : (
          <>
            <header className="view-header">
              <div className="view-header-row">
                <h1 id="activities-heading" ref={headingRef} tabIndex={-1}>
                  {title}
                </h1>
                {page !== "continue" && (
                  <button type="button" className="link-quiet" onClick={() => setManage("manage")}>
                    Manage activities
                  </button>
                )}
              </div>
              <p className="view-lede">
                {page === "continue"
                  ? "Everything you started, newest first. Each keeps its own save."
                  : COPY[page].lede}
              </p>
            </header>

            {message && <Banner tone="success">{message}</Banner>}
            {load.status === "loading" && (
              <div className="skeletons" role="status">
                <span className="visually-hidden">Loading activities…</span>
                <div className="skeleton" />
              </div>
            )}
            {load.status === "error" && (
              <Banner tone="danger">
                Activities couldn't be listed. Restart Lullshift to try again.
              </Banner>
            )}
            {load.status === "ready" && page === "continue" && (
              <ContinuePage
                list={list}
                now={now}
                onResume={start}
                onStartOver={(a) => void startOver(a)}
                onHelp={setHelpFor}
                onShowPage={onShowPage}
              />
            )}
            {load.status === "ready" && page !== "continue" && (
              <Library
                page={page}
                list={list}
                now={now}
                filter={filters[page]}
                onFilter={(f) => setFilters((all) => ({ ...all, [page]: f }))}
                onStart={start}
                onStartOver={(a) => void startOver(a)}
                onHelp={setHelpFor}
                onManage={() => setManage("manage")}
              />
            )}
          </>
        )}
        <HelpDialog
          open={helpFor !== null}
          name={helpFor?.name ?? ""}
          description={helpFor?.description ?? ""}
          help={helpFor?.help ?? null}
          category={helpFor?.category ?? ""}
          onClose={() => setHelpFor(null)}
        />
      </section>
    </>
  );
}

function ContinuePage({
  list,
  now,
  onResume,
  onStartOver,
  onHelp,
  onShowPage,
}: {
  list: ActivityInfo[];
  now: number;
  onResume: (a: ActivityInfo) => void;
  onStartOver: (a: ActivityInfo) => void;
  onHelp: (a: ActivityInfo) => void;
  onShowPage: (page: Page) => void;
}) {
  const saved = savedNewestFirst(list);
  if (saved.length === 0) {
    return (
      <div className="empty">
        <Mascot face="asleep" size={72} />
        <h2>Nothing saved yet</h2>
        <p>When you pause a game or a rest, it waits here until you come back to it.</p>
        <div className="empty-actions">
          <button type="button" className="btn btn-primary" onClick={() => onShowPage("play")}>
            Open Play
          </button>
          <button type="button" className="btn btn-secondary" onClick={() => onShowPage("rest")}>
            Open Rest
          </button>
        </div>
      </div>
    );
  }
  return (
    <ul className="resume-list">
      {saved.map((a) => (
        <ResumeCard
          key={a.id}
          activity={a}
          now={now}
          onResume={() => onResume(a)}
          onStartOver={() => onStartOver(a)}
          onHelp={() => onHelp(a)}
        />
      ))}
    </ul>
  );
}

function Library({
  page,
  list,
  now,
  filter,
  onFilter,
  onStart,
  onStartOver,
  onHelp,
  onManage,
}: {
  page: Page;
  list: ActivityInfo[];
  now: number;
  filter: { query: string; chip: string | null };
  onFilter: (f: { query: string; chip: string | null }) => void;
  onStart: (a: ActivityInfo) => void;
  onStartOver: (a: ActivityInfo) => void;
  onHelp: (a: ActivityInfo) => void;
  onManage: () => void;
}) {
  const copy = COPY[page];
  const all = onPage(list, page);
  const chips = availableChips(list, page);
  const shown = filterLibrary(list, page, filter.query, filter.chip);
  const filtering = filter.query.trim() !== "" || filter.chip !== null;
  const saved = savedNewestFirst(all).slice(0, 4);

  if (all.length === 0) {
    return (
      <div className="empty">
        <Mascot face="asleep" size={72} />
        <h2>{copy.none}</h2>
        <p>Turn some on under Manage activities, or add more from a marketplace.</p>
        <button type="button" className="btn btn-primary" onClick={onManage}>
          Manage activities
        </button>
      </div>
    );
  }

  return (
    <>
      <div className="library-bar">
        <label className="library-search">
          <span className="visually-hidden">{copy.search}</span>
          <Icon name="search" size={18} />
          <input
            type="search"
            value={filter.query}
            placeholder={copy.search}
            autoComplete="off"
            onChange={(e) => onFilter({ ...filter, query: e.target.value })}
          />
        </label>
        {chips.length > 0 && (
          <div className="chips" role="group" aria-label="Filter">
            <button
              type="button"
              className="chip"
              aria-pressed={filter.chip === null}
              onClick={() => onFilter({ ...filter, chip: null })}
            >
              All
            </button>
            {chips.map((c) => (
              <button
                key={c.id}
                type="button"
                className="chip"
                aria-pressed={filter.chip === c.id}
                onClick={() => onFilter({ ...filter, chip: filter.chip === c.id ? null : c.id })}
              >
                {c.label}
              </button>
            ))}
          </div>
        )}
      </div>

      {!filtering && saved.length > 0 && (
        <section aria-labelledby={`${page}-continue`} className="library-section">
          <h2 className="section-title" id={`${page}-continue`}>
            Continue
          </h2>
          <ul className="resume-list resume-list-row">
            {saved.map((a) => (
              <ResumeCard key={a.id} activity={a} now={now} onResume={() => onStart(a)} />
            ))}
          </ul>
        </section>
      )}

      <section aria-labelledby={`${page}-all`} className="library-section">
        <h2 className="section-title" id={`${page}-all`}>
          {filtering ? "Matches" : copy.all}
        </h2>
        {/* Announces the result of a search or a chip without moving focus. */}
        <p className="visually-hidden" role="status">
          {filtering ? `${shown.length} ${shown.length === 1 ? "match" : "matches"}` : ""}
        </p>
        {shown.length === 0 ? (
          <div className="empty empty-small">
            <h3>Nothing matches</h3>
            <p>Try a different word, or clear the filter.</p>
            <button
              type="button"
              className="btn btn-secondary"
              onClick={() => onFilter({ query: "", chip: null })}
            >
              Clear search
            </button>
          </div>
        ) : (
          <ul className="activity-grid">
            {shown.map((a) => (
              <ActivityCard
                key={a.id}
                activity={a}
                now={now}
                onStart={() => onStart(a)}
                onStartOver={() => onStartOver(a)}
                onHelp={() => onHelp(a)}
              />
            ))}
          </ul>
        )}
      </section>
    </>
  );
}

function ActivityCard({
  activity: a,
  now,
  onStart,
  onStartOver,
  onHelp,
}: {
  activity: ActivityInfo;
  now: number;
  onStart: () => void;
  onStartOver: () => void;
  onHelp: () => void;
}) {
  const preview = usePreview(a.id, a.preview);
  return (
    <li className="card activity-card">
      {a.preview && (
        <div className="activity-preview" aria-hidden={preview ? undefined : true}>
          {preview && <img src={preview} alt="" />}
        </div>
      )}
      <span className="activity-kind">{kindLabel(a.category)}</span>
      {activitySourceLabel(a.source, a.engine) && (
        <span className="activity-source">{activitySourceLabel(a.source, a.engine)}</span>
      )}
      <h3 className="activity-name">{a.name}</h3>
      <p className="activity-desc">{a.description}</p>
      {a.savedUtc && <p className="activity-saved">Saved {relativeTime(a.savedUtc, now)}</p>}
      <div className="activity-actions">
        <button type="button" className="btn btn-primary" onClick={onStart}>
          <Icon name="play" size={18} />
          {a.savedUtc ? "Resume" : "Start"}
          <span className="visually-hidden"> {a.name}</span>
        </button>
        <button type="button" className="btn btn-quiet" onClick={onHelp}>
          <Icon name="help" size={18} />
          {helpLabel(a.category)}
          <span className="visually-hidden"> {a.name}</span>
        </button>
        {a.savedUtc && (
          <button type="button" className="btn btn-quiet" onClick={onStartOver}>
            Start over
            <span className="visually-hidden"> {a.name}</span>
          </button>
        )}
      </div>
    </li>
  );
}
