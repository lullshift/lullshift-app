import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useMemo, useRef, useState } from "react";
import { useStore } from "zustand";
import { createDashboardStore } from "./dashboardStore";
import Integrations from "./Integrations";
import SessionsView from "./SessionsView";
import { healthProblems, sortSessions, type Dashboard } from "./sessions";
import Settings from "./Settings";
import { Icon, type IconName } from "./ui/Icon";
import { Mascot } from "./ui/Mascot";

/** Re-derive stale/unknown states and relative times even when no events arrive. */
const REFRESH_MS = 30_000;
const CHANGE_DEBOUNCE_MS = 250;

const dashboardStore = createDashboardStore(() => invoke<Dashboard>("dashboard"));

type ViewId = "sessions" | "integrations" | "settings";

const NAV: { id: ViewId; label: string; icon: IconName }[] = [
  { id: "sessions", label: "Sessions", icon: "sessions" },
  { id: "integrations", label: "Integrations", icon: "plug" },
  { id: "settings", label: "Settings", icon: "settings" },
];

export default function App() {
  const state = useStore(dashboardStore, (s) => s.state);
  const refresh = useStore(dashboardStore, (s) => s.refresh);
  const [view, setView] = useState<ViewId>("sessions");
  const [now, setNow] = useState(() => Date.now());
  const headingRef = useRef<HTMLHeadingElement>(null);
  const firstRender = useRef(true);

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

  // Move focus to the new view's heading so keyboard and screen-reader users land in it.
  useEffect(() => {
    if (firstRender.current) {
      firstRender.current = false;
      return;
    }
    headingRef.current?.focus();
  }, [view]);

  const dashboard =
    state.status === "ready" ? state.dashboard : state.status === "error" ? state.last : null;
  const problems = useMemo(() => (dashboard ? healthProblems(dashboard) : []), [dashboard]);
  const sessions = useMemo(() => (dashboard ? sortSessions(dashboard.sessions) : []), [dashboard]);
  const needsYou = sessions.filter((s) => s.state === "needs_attention").length;

  return (
    <div className="shell">
      <aside className="sidebar">
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
      </aside>

      <main className="content">
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
          />
        )}
        {view === "integrations" && <Integrations headingRef={headingRef} />}
        {view === "settings" && <Settings headingRef={headingRef} />}
      </main>
    </div>
  );
}
