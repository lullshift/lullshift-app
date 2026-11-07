import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useStore } from "zustand";
import { pageOf } from "../activity/library";
import { useActivityList } from "../activity/useActivityList";
import type { ActivityInfo } from "../activity/ActivityHost";
import { viewKey } from "../context/panelLogic";
import { createDashboardStore } from "../dashboardStore";
import { homeStatus } from "../home/status";
import {
  healthProblems,
  liveState,
  sortSessions,
  type Dashboard,
  type SessionView,
} from "../sessions";
import {
  dateSeed,
  pickSuggestion,
  suggestionAction,
  waitFor,
  waitLine,
  type WaitEstimate,
} from "../suggest";
import { Banner } from "../ui/Banner";
import { Icon } from "../ui/Icon";
import { Mascot } from "../ui/Mascot";
import { reloadUiPrefs } from "../uiPrefsStore";
import { hideTray, openMain, type OpenMainRequest } from "./api";
import { needing, needsWho, quickChips } from "./logic";

/** Re-derive relative times and stale states even when no events arrive. */
const REFRESH_MS = 30_000;

const dashboardStore = createDashboardStore(() => invoke<Dashboard>("dashboard"));

/**
 * The tray window (ADR-026 section 6): what the person needs to know and one thing to do, in a
 * small window. It only reads; every action asks the main window to do it (`open_main`), so no
 * activity ever runs here.
 */
export default function TrayApp() {
  const state = useStore(dashboardStore, (s) => s.state);
  const refreshDashboard = useStore(dashboardStore, (s) => s.refresh);
  const { load: activityLoad, refresh: refreshActivities } = useActivityList();
  const [estimates, setEstimates] = useState<WaitEstimate[] | null>(null);
  const [now, setNow] = useState(() => Date.now());
  const [shuffles, setShuffles] = useState(0);
  // Bumps each time the window is shown: refresh, and put focus on the first useful control.
  const [shownTick, setShownTick] = useState(0);
  const [problem, setProblem] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const focusedFor = useRef(-1);

  const refreshEstimates = useCallback(() => {
    invoke<WaitEstimate[]>("wait_estimates")
      .then(setEstimates)
      .catch(() => setEstimates(null));
  }, []);

  const refreshAll = useCallback(() => {
    setNow(Date.now());
    void refreshDashboard();
    refreshActivities();
    refreshEstimates();
  }, [refreshDashboard, refreshActivities, refreshEstimates]);

  useEffect(() => {
    const timer = window.setInterval(() => {
      setNow(Date.now());
      void refreshDashboard();
    }, REFRESH_MS);
    return () => window.clearInterval(timer);
  }, [refreshDashboard]);

  useEffect(() => {
    let disposed = false;
    const unlisten: (() => void)[] = [];
    const on = (event: string, handler: () => void) =>
      listen(event, handler)
        .then((fn) => (disposed ? fn() : unlisten.push(fn)))
        .catch(() => undefined);
    void on("tray-shown", () => {
      setShuffles(0);
      setProblem(false);
      setShownTick((n) => n + 1);
      void reloadUiPrefs();
      refreshAll();
    });
    let pending: number | undefined;
    void on("sessions-changed", () => {
      pending ??= window.setTimeout(() => {
        pending = undefined;
        setNow(Date.now());
        void refreshDashboard();
      }, 250);
    });
    return () => {
      disposed = true;
      window.clearTimeout(pending);
      unlisten.forEach((fn) => fn());
    };
  }, [refreshAll, refreshDashboard]);

  // First load (the window is created hidden, so this runs before it is ever shown).
  useEffect(() => {
    void refreshDashboard();
    refreshEstimates();
    void reloadUiPrefs();
  }, [refreshDashboard, refreshEstimates]);

  // Esc hides the window.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        void hideTray().catch(() => undefined);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const dashboard =
    state.status === "ready" ? state.dashboard : state.status === "error" ? state.last : null;
  const loading = state.status === "loading";
  const coreError = state.status === "error";
  const hasDashboard = !loading && (!coreError || dashboard !== null);
  const problems = useMemo(() => (dashboard ? healthProblems(dashboard) : []), [dashboard]);
  const sessions = useMemo(() => (dashboard ? sortSessions(dashboard.sessions) : []), [dashboard]);
  const status = homeStatus({ loading, coreError, hasDashboard, problems, sessions });
  const needs = needing(sessions);
  const working = sessions.filter((s) => liveState(s) === "working");
  const waiting = waitFor(
    working.map((s) => s.provider),
    estimates ?? [],
  );

  const list = activityLoad.status === "ready" ? activityLoad.list : null;
  const seed = dateSeed(new Date(now));
  const suggestInput = useMemo(
    () =>
      list
        ? {
            activities: list,
            estimateSeconds: waiting?.medianSeconds ?? null,
            anyWorking: working.length > 0,
            seed,
          }
        : null,
    [list, waiting?.medianSeconds, working.length, seed],
  );
  const suggestion = useMemo(
    () => (suggestInput ? pickSuggestion(suggestInput, shuffles) : null),
    [suggestInput, shuffles],
  );
  const chips = useMemo(
    () => (suggestInput ? quickChips(suggestInput, suggestion?.activity.id ?? null) : []),
    [suggestInput, suggestion],
  );

  const ready = !loading && activityLoad.status !== "loading";
  useEffect(() => {
    if (!ready || focusedFor.current === shownTick) return;
    focusedFor.current = shownTick;
    const root = rootRef.current;
    const first =
      root?.querySelector<HTMLElement>("[data-first]") ??
      root?.querySelector<HTMLElement>("button");
    first?.focus();
  }, [ready, shownTick]);

  const go = (r: OpenMainRequest) => {
    setProblem(false);
    openMain(r).catch(() => setProblem(true));
  };
  const goSession = (s: SessionView) => go({ view: "sessions", sessionKey: viewKey(s) });
  const start = (a: ActivityInfo) => go({ view: pageOf(a.category), activityId: a.id });
  const retry = () => {
    void refreshDashboard();
    refreshActivities();
  };

  return (
    <div className="tray-shell" ref={rootRef}>
      <main className="tray-main" aria-labelledby="tray-title">
        <header className={needs.length > 0 ? "tray-top tray-top-needs" : "tray-top"}>
          <Mascot face={status.face} size={needs.length > 0 ? 52 : 88} />
          <h1 id="tray-title" tabIndex={-1} className="tray-title">
            {status.title}
          </h1>
          <p className="tray-line">{status.line}</p>
        </header>

        <div className="tray-body">
          {loading && (
            <div className="skeletons" role="status">
              <span className="visually-hidden">Loading your agents…</span>
              <div className="skeleton" />
            </div>
          )}
          {!loading && coreError && (
            <Banner tone="danger">
              {dashboard
                ? "Lullshift couldn't refresh just now. What you see may be out of date."
                : "Lullshift couldn't read your agents."}{" "}
              <button type="button" className="link-btn" onClick={retry}>
                Try again
              </button>
            </Banner>
          )}
          {problem && <Banner tone="danger">Lullshift couldn't open that. Try again.</Banner>}

          {needs.length > 0 && (
            <section className="card tray-needs" aria-labelledby="tray-needs-title">
              <h2 id="tray-needs-title" className="tray-needs-title">
                {needs.length === 1 ? "Needs you now" : "Need you now"}
              </h2>
              <p className="tray-needs-who">{needsWho(needs, now)}</p>
              <button
                type="button"
                className="btn btn-primary"
                data-first=""
                onClick={() =>
                  needs.length === 1 ? goSession(needs[0]!) : go({ view: "sessions" })
                }
              >
                {needs.length === 1 ? "Go to session" : "Go to sessions"}
              </button>
            </section>
          )}

          <Suggestion
            load={activityLoad.status}
            line={waitLine(waiting, working.length > 0)}
            name={suggestion?.activity.name ?? null}
            description={suggestion?.activity.description ?? null}
            kind={suggestion?.kind ?? null}
            canShuffle={(list?.length ?? 0) > 1}
            announce={shuffles > 0}
            first={needs.length === 0}
            onStart={() => suggestion && start(suggestion.activity)}
            onShuffle={() => setShuffles((n) => n + 1)}
            onRetry={retry}
          />

          {chips.length > 0 && (
            <ul className="tray-chips" aria-label="More to try">
              {chips.map((a) => (
                <li key={a.id}>
                  <button type="button" className="chip tray-chip" onClick={() => start(a)}>
                    {a.name}
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      </main>

      <div className="tray-open-row">
        <button
          type="button"
          className="btn btn-secondary tray-open"
          data-first=""
          onClick={() => go({ view: "home" })}
        >
          Open Lullshift
          <Icon name="arrow" size={16} />
        </button>
      </div>

      <nav className="tray-tabs" aria-label="Quick window">
        <button
          type="button"
          className="tray-tab"
          aria-current="page"
          onClick={() => document.getElementById("tray-title")?.focus()}
        >
          Now
        </button>
        <button type="button" className="tray-tab" onClick={() => go({ view: "play" })}>
          Library
        </button>
        <button type="button" className="tray-tab" onClick={() => go({ view: "companion" })}>
          Talk
        </button>
      </nav>
    </div>
  );
}

function Suggestion({
  load,
  line,
  name,
  description,
  kind,
  canShuffle,
  announce,
  first,
  onStart,
  onShuffle,
  onRetry,
}: {
  load: "loading" | "ready" | "error";
  line: string;
  name: string | null;
  description: string | null;
  kind: "resume" | "rest" | "play" | null;
  canShuffle: boolean;
  /** Say what is suggested now: only after a Shuffle click. */
  announce: boolean;
  /** Nothing needs you, so this button is the first useful control. */
  first: boolean;
  onStart: () => void;
  onShuffle: () => void;
  onRetry: () => void;
}) {
  if (load === "loading") {
    return (
      <div className="skeletons" role="status">
        <span className="visually-hidden">Loading a suggestion…</span>
        <div className="skeleton" />
      </div>
    );
  }
  if (load === "error") {
    return (
      <Banner tone="danger">
        Activities couldn't be listed, so there is no suggestion.{" "}
        <button type="button" className="link-btn" onClick={onRetry}>
          Try again
        </button>
      </Banner>
    );
  }
  if (!name || !kind) {
    return (
      <section className="card tray-suggest" aria-labelledby="tray-suggest-title">
        <p className="tray-suggest-line">{line}</p>
        <h2 id="tray-suggest-title" className="tray-suggest-name">
          No activities are turned on
        </h2>
        <p className="tray-suggest-desc">Open Lullshift to turn some on.</p>
      </section>
    );
  }
  return (
    <section className="card tray-suggest" aria-labelledby="tray-suggest-title">
      <p className="tray-suggest-line">{line}</p>
      <h2 id="tray-suggest-title" className="tray-suggest-name">
        {name}
      </h2>
      <p className="tray-suggest-desc">{description}</p>
      <div className="tray-suggest-actions">
        <button
          type="button"
          className="btn btn-primary"
          data-first={first ? "" : undefined}
          onClick={onStart}
        >
          <Icon name={kind === "resume" ? "resume" : "play"} size={18} />
          {suggestionAction(kind)}
          <span className="visually-hidden"> {name}</span>
        </button>
        {canShuffle && (
          <button type="button" className="btn btn-quiet" onClick={onShuffle}>
            <Icon name="shuffle" size={18} />
            Shuffle
          </button>
        )}
        <span className="visually-hidden" role="status">
          {announce ? `Suggesting ${name}.` : ""}
        </span>
      </div>
    </section>
  );
}
