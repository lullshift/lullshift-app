import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useMemo, useRef, useState } from "react";
import { useStore } from "zustand";
import ActivitiesView from "./activity/ActivitiesView";
import CompanionView from "./companion/CompanionView";
import { shouldMountCompanion } from "./companion/logic";
import type { PauseRequest } from "./activity/ActivityHost";
import { createDashboardStore } from "./dashboardStore";
import Integrations from "./Integrations";
import SessionsView from "./SessionsView";
import { healthProblems, liveState, sortSessions, type Dashboard } from "./sessions";
import Settings from "./Settings";
import { Onboarding } from "./Onboarding";
import { loadUiPrefs, updateUiPrefs, useShowWelcome, useUiPrefs } from "./uiPrefsStore";
import { Icon, type IconName } from "./ui/Icon";
import { Mascot } from "./ui/Mascot";
import { SkipLink } from "./ui/SkipLink";

/** Re-derive stale/unknown states and relative times even when no events arrive. */
const REFRESH_MS = 30_000;
const CHANGE_DEBOUNCE_MS = 250;

const dashboardStore = createDashboardStore(() => invoke<Dashboard>("dashboard"));

type ViewId = "sessions" | "activities" | "companion" | "integrations" | "settings";

const NAV: { id: ViewId; label: string; icon: IconName }[] = [
  { id: "sessions", label: "Sessions", icon: "sessions" },
  { id: "activities", label: "Activities", icon: "activities" },
  { id: "companion", label: "Companion", icon: "chat" },
  { id: "integrations", label: "Integrations", icon: "plug" },
  { id: "settings", label: "Settings", icon: "settings" },
];

export default function App() {
  const state = useStore(dashboardStore, (s) => s.state);
  const refresh = useStore(dashboardStore, (s) => s.refresh);
  const [view, setView] = useState<ViewId>("sessions");
  const { contentWidth } = useUiPrefs();
  const showWelcome = useShowWelcome();
  const welcomeWas = useRef(false);
  useEffect(() => {
    void loadUiPrefs();
  }, []);
  const [now, setNow] = useState(() => Date.now());
  const [pause, setPause] = useState<PauseRequest | null>(null);
  // An activity the user chose to start from a Companion suggestion (their click, never the model).
  const [launch, setLaunch] = useState<{ id: string; nonce: number } | null>(null);
  // Bumps on each sessions-changed event so an open detail panel re-reads its session.
  const [changeTick, setChangeTick] = useState(0);
  const headingRef = useRef<HTMLHeadingElement>(null);
  const mainRef = useRef<HTMLElement>(null);
  // The Activities heading while that view is hidden (only the visible view gets focus).
  const unusedHeading = useRef<HTMLHeadingElement>(null);
  const unusedCompanionHeading = useRef<HTMLHeadingElement>(null);
  const firstRender = useRef(true);
  // Companion's "Provider settings" link: Settings opens with its Companion section focused, so
  // the view heading must not take focus that time.
  const [companionFocus, setCompanionFocus] = useState(0);
  const skipHeadingFocus = useRef(false);
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

  return (
    <div className="shell">
      {/* First in the tab order; hidden until focused. Skips the five-item nav. */}
      <SkipLink targetId="main-content" targetRef={mainRef} />
      <header className="sidebar">
        <div className="brand">
          <Mascot face="watching" size={36} />
          <span className="brand-name">Lullshift</span>
        </div>
        <nav aria-label="Main">
          <ul className="nav">
            {NAV.map((item) => (
              <li key={item.id}>
                <button
                  type="button"
                  className="nav-item"
                  aria-current={view === item.id ? "page" : undefined}
                  onClick={() => setView(item.id)}
                >
                  <Icon name={item.icon} />
                  <span className="nav-label">{item.label}</span>
                  {item.id === "sessions" && needsYou > 0 && (
                    <span className="nav-badge">
                      {needsYou}
                      <span className="visually-hidden"> need you</span>
                    </span>
                  )}
                </button>
              </li>
            ))}
          </ul>
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
            onConnect={() => setView("integrations")}
            changeTick={changeTick}
            onRefresh={() => void refresh()}
            onOpenSettings={() => setView("settings")}
          />
        )}
        {/* Stays mounted while hidden so a running activity can save before it pauses. */}
        <div hidden={view !== "activities"} className="view-slot">
          <ActivitiesView
            headingRef={view === "activities" ? headingRef : unusedHeading}
            active={view === "activities"}
            pause={pause}
            now={now}
            launch={launch}
            onGoToSessions={() => setView("sessions")}
          />
        </div>
        {/* Mounted on first visit (it reads the keychain), then stays mounted while hidden so a
            reply in flight isn't lost when you switch views. */}
        {shouldMountCompanion(view, companionOpened) && (
          <div hidden={view !== "companion"} className="view-slot">
            <CompanionView
              headingRef={view === "companion" ? headingRef : unusedCompanionHeading}
              sessions={sessions}
              now={now}
              onStartActivity={(id) => {
                setLaunch({ id, nonce: Date.now() });
                setView("activities");
              }}
              onOpenSettings={() => {
                skipHeadingFocus.current = view !== "settings";
                setCompanionFocus((n) => n + 1);
                setView("settings");
              }}
            />
          </div>
        )}
        {view === "integrations" && <Integrations headingRef={headingRef} />}
        {view === "settings" && (
          <Settings headingRef={headingRef} companionFocus={companionFocus} />
        )}
      </main>
      <Onboarding
        open={showWelcome}
        onDone={() => void updateUiPrefs({ onboardingDone: true })}
        onConnect={() => setView("integrations")}
        onTryActivity={() => setView("activities")}
      />
    </div>
  );
}
