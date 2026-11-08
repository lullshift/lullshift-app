import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useStore } from "zustand";
import ActivitiesView from "./activity/ActivitiesView";
import type { ActivityInfo } from "./activity/ActivityHost";
import { onPage, pageOf, savedNewestFirst, type Page } from "./activity/library";
import { useActivityList } from "./activity/useActivityList";
import CompanionView from "./companion/CompanionView";
import { onOpenThread } from "./companion/events";
import { useBackgroundCount } from "./companion/inflight";
import { shouldMountCompanion } from "./companion/logic";
import { ToastRegion } from "./ui/Toasts";
import type { PauseRequest } from "./activity/ActivityHost";
import { createDashboardStore } from "./dashboardStore";
import HomeView from "./home/HomeView";
import NotesView from "./notes/NotesView";
import { openItemCount } from "./notes/notesLogic";
import { overviewOf, useNotesOverview } from "./notes/useNotesOverview";
import SessionsView from "./SessionsView";
import type { SessionKey } from "./context/api";
import { viewKey } from "./context/panelLogic";
import { healthProblems, liveState, sortSessions, type Dashboard } from "./sessions";
import Settings from "./Settings";
import { Onboarding } from "./Onboarding";
import { onOpenMain, type OpenMainEvent } from "./openMain";
import { loadUiPrefs, updateUiPrefs, useShowWelcome, useUiPrefs } from "./uiPrefsStore";
import { Icon, type IconName } from "./ui/Icon";
import { Mascot } from "./ui/Mascot";
import { SkipLink } from "./ui/SkipLink";

/** Re-derive stale/unknown states and relative times even when no events arrive. */
const REFRESH_MS = 30_000;
const CHANGE_DEBOUNCE_MS = 250;

const dashboardStore = createDashboardStore(() => invoke<Dashboard>("dashboard"));

type ViewId =
  "home" | "sessions" | "notes" | "play" | "rest" | "continue" | "companion" | "settings";

interface NavItem {
  id: ViewId;
  label: string;
  icon: IconName;
}

// ADR-026 sections 1 and 7: Home, Sessions and Notes, then the "While you wait" group, then Settings last.
const NAV_TOP: NavItem[] = [
  { id: "home", label: "Home", icon: "home" },
  { id: "sessions", label: "Sessions", icon: "sessions" },
  { id: "notes", label: "Notes", icon: "notes" },
];
const NAV_WAIT: NavItem[] = [
  { id: "play", label: "Play", icon: "play" },
  { id: "rest", label: "Rest", icon: "activities" },
  { id: "continue", label: "Continue", icon: "resume" },
  { id: "companion", label: "Companion", icon: "chat" },
];
const NAV_BOTTOM: NavItem[] = [{ id: "settings", label: "Settings", icon: "settings" }];

const isLibraryView = (v: ViewId): v is Page | "continue" =>
  v === "play" || v === "rest" || v === "continue";

export default function App() {
  const state = useStore(dashboardStore, (s) => s.state);
  const refresh = useStore(dashboardStore, (s) => s.refresh);
  const [view, setView] = useState<ViewId>("home");
  // The library page that was last open, so a running activity keeps its page while hidden.
  const [libraryPage, setLibraryPage] = useState<Page | "continue">("play");
  if (isLibraryView(view) && view !== libraryPage) setLibraryPage(view);
  const { load: activityLoad, refresh: refreshActivities } = useActivityList();
  const { contentWidth } = useUiPrefs();
  const showWelcome = useShowWelcome();
  const welcomeWas = useRef(false);
  useEffect(() => {
    void loadUiPrefs();
  }, []);
  const [now, setNow] = useState(() => Date.now());
  const [pause, setPause] = useState<PauseRequest | null>(null);
  // An activity the user chose to start from a Companion suggestion (their click, never the model).
  const [launch, setLaunch] = useState<{
    id: string;
    nonce: number;
    activity?: ActivityInfo;
  } | null>(null);
  // Bumps on each click of Play, Rest or Continue in the sidebar.
  const [navNonce, setNavNonce] = useState(0);
  // Home's "Go to session": Sessions opens that session's detail panel.
  const [openSession, setOpenSession] = useState<{ key: SessionKey; nonce: number } | null>(null);
  // Connect from the welcome: Integrations takes focus only once the dialog has closed.
  const [connectPending, setConnectPending] = useState(false);
  // Spent once the user leaves Sessions, so coming back later doesn't reopen the panel.
  if (view !== "sessions" && openSession !== null) setOpenSession(null);
  // Bumps on each sessions-changed event so an open detail panel re-reads its session.
  const [changeTick, setChangeTick] = useState(0);
  const notes = useNotesOverview(changeTick);
  const reloadNotes = notes.reload;
  const headingRef = useRef<HTMLHeadingElement>(null);
  const mainRef = useRef<HTMLElement>(null);
  // The library heading while those views are hidden (only the visible view gets focus).
  const unusedHeading = useRef<HTMLHeadingElement>(null);
  const unusedCompanionHeading = useRef<HTMLHeadingElement>(null);
  const firstRender = useRef(true);
  // Companion's "Provider settings" link: Settings opens with its Companion section focused, so
  // the view heading must not take focus that time.
  const [companionFocus, setCompanionFocus] = useState(0);
  const skipHeadingFocus = useRef(false);
  // Anything that sends the user to Settings > Integrations bumps this; the section takes focus.
  const [integrationsFocus, setIntegrationsFocus] = useState(0);
  // Spent once the user leaves Settings, so a later visit does not scroll back to it.
  if (view !== "settings" && integrationsFocus !== 0) setIntegrationsFocus(0);
  // Companion reads the keychain when it mounts, so it is mounted on first visit, not at launch.
  const [companionOpened, setCompanionOpened] = useState(false);
  // Adjusting state while rendering (React's pattern for state derived from a change).
  if (view === "companion" && !companionOpened) setCompanionOpened(true);

  // An agent needs the user: a running activity pauses whatever the notification settings say.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    listen<string>("agent-attention", (event) =>
      setPause((p) => ({ seq: (p?.seq ?? 0) + 1, reason: "attention", provider: event.payload })),
    )
      .then((fn) => (disposed ? fn() : (unlisten = fn)))
      .catch(() => undefined);
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    void refresh();
    const timer = window.setInterval(() => {
      setNow(Date.now());
      void refresh();
    }, REFRESH_MS);
    let unlisten: (() => void) | undefined;
    let disposed = false;
    // Coalesce bursts of events into one refresh.
    let pending: number | undefined;
    const onChanged = () => {
      pending ??= window.setTimeout(() => {
        pending = undefined;
        setNow(Date.now());
        setChangeTick((n) => n + 1);
        void refresh();
      }, CHANGE_DEBOUNCE_MS);
    };
    listen("sessions-changed", onChanged)
      .then((fn) => (disposed ? fn() : (unlisten = fn)))
      .catch(() => undefined);
    return () => {
      disposed = true;
      window.clearInterval(timer);
      window.clearTimeout(pending);
      unlisten?.();
    };
  }, [refresh]);

  // When the welcome closes, focus would drop to the page body: put it on the view heading.
  // (Choosing Connect or Try changes the view too; its heading then takes focus below.)
  useEffect(() => {
    if (welcomeWas.current && !showWelcome) headingRef.current?.focus();
    welcomeWas.current = showWelcome;
  }, [showWelcome]);

  // The welcome's Connect: once its dialog is gone, send focus to Integrations on the next frame
  // (the dialog's own close would otherwise hand focus back to what opened it).
  useEffect(() => {
    if (!connectPending || showWelcome) return;
    const frame = window.requestAnimationFrame(() => {
      setConnectPending(false);
      setIntegrationsFocus((n) => n + 1);
    });
    return () => window.cancelAnimationFrame(frame);
  }, [connectPending, showWelcome]);

  // Notes shows what was written in session panels: read it fresh on each visit.
  useEffect(() => {
    if (view === "notes") void reloadNotes();
  }, [view, reloadNotes]);

  // Home shows what is saved: read it fresh on each visit.
  useEffect(() => {
    if (view === "home") refreshActivities();
  }, [view, refreshActivities]);

  // Move focus to the new view's heading so keyboard and screen-reader users land in it.
  useEffect(() => {
    if (firstRender.current) {
      firstRender.current = false;
      return;
    }
    if (skipHeadingFocus.current) {
      skipHeadingFocus.current = false;
      return;
    }
    headingRef.current?.focus();
  }, [view]);

  const dashboard =
    state.status === "ready" ? state.dashboard : state.status === "error" ? state.last : null;
  const problems = useMemo(() => (dashboard ? healthProblems(dashboard) : []), [dashboard]);
  const sessions = useMemo(() => (dashboard ? sortSessions(dashboard.sessions) : []), [dashboard]);
  const needsYou = sessions.filter((s) => liveState(s) === "needs_attention").length;
  // Replies that finished while away, and lookups waiting for approval (not when you're there).
  const companionUpdates = useBackgroundCount();

  // Sidebar counts: what is in each library, and how many activities have a save.
  const libraryList = activityLoad.status === "ready" ? activityLoad.list : null;
  const notesOverview = overviewOf(notes.load);
  const counts: Partial<Record<ViewId, number>> = {
    // Open checklist items; no count at all when there are none (and never an orange badge).
    ...(notesOverview &&
      openItemCount(notesOverview) > 0 && { notes: openItemCount(notesOverview) }),
    ...(libraryList && {
      play: onPage(libraryList, "play").length,
      rest: onPage(libraryList, "rest").length,
      continue: savedNewestFirst(libraryList).length,
    }),
  };
  const showPage = useCallback((p: Page) => setView(p), []);
  const startActivity = (a: ActivityInfo) => {
    setLaunch({ id: a.id, nonce: Date.now(), activity: a });
    setView(pageOf(a.category));
  };
  // Integrations lives in Settings: every link that used to open it opens Settings there.
  const openIntegrations = () => {
    skipHeadingFocus.current = view !== "settings";
    setIntegrationsFocus((n) => n + 1);
    setView("settings");
  };

  // A notice's Open button: go to Companion (the view selects the conversation).
  useEffect(
    () =>
      onOpenThread(() => {
        setView("companion");
        // Already in Companion: no view change moves focus, so put it on the heading.
        window.setTimeout(() => headingRef.current?.focus(), 0);
      }),
    [],
  );

  // The tray window asked for a view (Rust already checked the request). The one subscription
  // reads the current handler through a ref, set in an effect.
  const openMainRef = useRef<(r: OpenMainEvent) => void>(() => undefined);
  useEffect(() => {
    openMainRef.current = (r) => {
      // The welcome is open: the window is shown (by Rust), nothing navigates or starts under it.
      if (showWelcome) return;
      if (r.sessionKey) {
        setOpenSession({ key: r.sessionKey, nonce: Date.now() });
        setView("sessions");
      } else if (r.activityId) {
        // Through the normal begin path of the library, like a Companion suggestion.
        const known = libraryList?.find((a) => a.id === r.activityId);
        if (known) startActivity(known);
        else {
          setLaunch({ id: r.activityId, nonce: Date.now() });
          setView(r.view === "rest" ? "rest" : "play");
        }
      } else {
        setView(r.view);
        if (isLibraryView(r.view)) setNavNonce((n) => n + 1);
      }
      window.setTimeout(() => headingRef.current?.focus(), 0);
    };
  });
  useEffect(() => onOpenMain((r) => openMainRef.current(r)), []);

  const navItem = (item: NavItem) => (
    <li key={item.id}>
      <button
        type="button"
        className="nav-item"
        aria-current={view === item.id ? "page" : undefined}
        onClick={() => {
          setView(item.id);
          if (isLibraryView(item.id)) setNavNonce((n) => n + 1);
        }}
      >
        <Icon name={item.icon} />
        <span className="nav-label">{item.label}</span>
        {counts[item.id] !== undefined && (item.id !== "continue" || counts.continue! > 0) && (
          <span className="nav-count">{counts[item.id]}</span>
        )}
        {item.id === "companion" && companionUpdates > 0 && (
          <span className="nav-badge">
            {companionUpdates}
            <span className="visually-hidden">
              {" "}
              {companionUpdates === 1 ? "Companion update" : "Companion updates"} waiting
            </span>
          </span>
        )}
        {item.id === "sessions" && needsYou > 0 && (
          <span className="nav-badge">
            {needsYou}
            <span className="visually-hidden"> need you</span>
          </span>
        )}
      </button>
    </li>
  );

  return (
    <div className="shell">
      {/* First in the tab order; hidden until focused. Skips the sidebar nav. */}
      <SkipLink targetId="main-content" targetRef={mainRef} />
      <header className="sidebar">
        <div className="brand">
          <Mascot face="watching" size={36} />
          <span className="brand-name">Lullshift</span>
        </div>
        <nav aria-label="Main" className="nav-wrap">
          <ul className="nav">
            {NAV_TOP.map(navItem)}
            <li>
              <span className="nav-group" id="nav-wait">
                While you wait
              </span>
              <ul className="nav" aria-labelledby="nav-wait">
                {NAV_WAIT.map(navItem)}
              </ul>
            </li>
          </ul>
          <ul className="nav nav-bottom">{NAV_BOTTOM.map(navItem)}</ul>
        </nav>
        <p className="sidebar-note">
          <Icon name="lock" size={16} />
          <span>Local only. Works offline, no account.</span>
        </p>
      </header>

      <main
        id="main-content"
        ref={mainRef}
        tabIndex={-1}
        className={`content width-${contentWidth}`}
      >
        {view === "home" && (
          <HomeView
            headingRef={headingRef}
            loading={state.status === "loading"}
            coreError={state.status === "error"}
            hasLast={state.status === "error" && state.last !== null}
            problems={problems}
            sessions={sessions}
            staleAfterSeconds={dashboard?.staleAfterSeconds ?? 600}
            now={now}
            activities={activityLoad}
            onRefresh={() => {
              void refresh();
              refreshActivities();
            }}
            onGoToSessions={() => setView("sessions")}
            onOpenSession={(s) => {
              setOpenSession({ key: viewKey(s), nonce: Date.now() });
              setView("sessions");
            }}
            onConnect={openIntegrations}
            onOpenPage={setView}
            onStartActivity={startActivity}
          />
        )}
        {view === "notes" && (
          <NotesView
            headingRef={headingRef}
            load={notes.load}
            reload={notes.reload}
            sessions={sessions}
            now={now}
            onOpenSession={(key) => {
              setOpenSession({ key, nonce: Date.now() });
              setView("sessions");
            }}
          />
        )}
        {view === "sessions" && (
          <SessionsView
            headingRef={headingRef}
            loading={state.status === "loading"}
            coreError={state.status === "error"}
            hasLast={state.status === "error" && state.last !== null}
            problems={problems}
            sessions={sessions}
            staleAfterSeconds={dashboard?.staleAfterSeconds ?? 600}
            now={now}
            onConnect={openIntegrations}
            openRequest={openSession}
            changeTick={changeTick}
            onRefresh={() => void refresh()}
            onOpenSettings={() => setView("settings")}
          />
        )}
        {/* Stays mounted while hidden so a running activity can save before it pauses. */}
        <div hidden={!isLibraryView(view)} className="view-slot">
          <ActivitiesView
            headingRef={isLibraryView(view) ? headingRef : unusedHeading}
            active={isLibraryView(view)}
            page={libraryPage}
            load={activityLoad}
            refresh={refreshActivities}
            pause={pause}
            now={now}
            launch={launch}
            navNonce={navNonce}
            onGoToSessions={() => setView("sessions")}
            onShowPage={showPage}
          />
        </div>
        {/* Mounted on first visit (it reads the keychain), then stays mounted while hidden so a
            reply in flight isn't lost when you switch views. */}
        {shouldMountCompanion(view, companionOpened) && (
          <div hidden={view !== "companion"} className="view-slot">
            <CompanionView
              headingRef={view === "companion" ? headingRef : unusedCompanionHeading}
              active={view === "companion"}
              sessions={sessions}
              now={now}
              onStartActivity={(id) => {
                setLaunch({ id, nonce: Date.now() });
                setView("play");
              }}
              onOpenSettings={() => {
                skipHeadingFocus.current = view !== "settings";
                setCompanionFocus((n) => n + 1);
                setView("settings");
              }}
            />
          </div>
        )}
        {view === "settings" && (
          <Settings
            headingRef={headingRef}
            companionFocus={companionFocus}
            integrationsFocus={integrationsFocus}
          />
        )}
      </main>
      <ToastRegion />
      <Onboarding
        open={showWelcome}
        onDone={() => void updateUiPrefs({ onboardingDone: true })}
        onConnect={() => {
          // Go to Settings now; the focus request waits for the dialog to finish closing.
          skipHeadingFocus.current = view !== "settings";
          setConnectPending(true);
          setView("settings");
        }}
        onTryActivity={(category) => setView(category ? pageOf(category) : "play")}
      />
    </div>
  );
}
