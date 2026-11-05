import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useMemo } from "react";
import { useStore } from "zustand";
import { createDashboardStore } from "./dashboardStore";
import { healthProblems, reasonText, sortSessions, stateView, type Dashboard } from "./sessions";
import Settings from "./Settings";

/** Re-derive stale/unknown states even when no events arrive. */
const REFRESH_MS = 30_000;
const CHANGE_DEBOUNCE_MS = 250;

const dashboardStore = createDashboardStore(() => invoke<Dashboard>("dashboard"));

export default function App() {
  const state = useStore(dashboardStore, (s) => s.state);
  const refresh = useStore(dashboardStore, (s) => s.refresh);

  useEffect(() => {
    void refresh();
    const timer = window.setInterval(() => void refresh(), REFRESH_MS);
    let unlisten: (() => void) | undefined;
    let disposed = false;
    // Coalesce bursts of events into one refresh.
    let pending: number | undefined;
    const onChanged = () => {
      pending ??= window.setTimeout(() => {
        pending = undefined;
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

  const dashboard =
    state.status === "ready" ? state.dashboard : state.status === "error" ? state.last : null;
  const problems = useMemo(() => (dashboard ? healthProblems(dashboard) : []), [dashboard]);
  const sessions = useMemo(() => (dashboard ? sortSessions(dashboard.sessions) : []), [dashboard]);

  return (
    <main className="app">
      <header>
        <h1>Lullshift</h1>
        <p className="note">Your agent sessions, observed locally.</p>
      </header>

      {state.status === "error" && (
        <p role="alert" className="problem">
          Couldn't reach the Lullshift core. Agent status is unknown
          {state.last ? "; showing the last list received." : "."}
        </p>
      )}
      {problems.map((p) => (
        <p role="alert" className="problem" key={p}>
          {p}
        </p>
      ))}

      <section aria-labelledby="sessions-heading">
        <h2 id="sessions-heading">Agent sessions</h2>
        {state.status === "loading" && <p role="status">Loading sessions…</p>}
        {dashboard && sessions.length === 0 && (
          <p className="empty">
            No agent sessions yet. Sessions appear here once a connected agent reports activity.
          </p>
        )}
        {dashboard && sessions.length > 0 && (
          <ul className="sessions">
            {sessions.map((s) => {
              const view = stateView(s.state);
              const why = reasonText(s, dashboard.staleAfterSeconds);
              return (
                <li key={`${s.provider}:${s.providerSessionId}`} className="session">
                  <span className="title" title={s.providerSessionId}>
                    {s.providerSessionId}
                  </span>
                  <span className="provider">{s.provider}</span>
                  <span className={`state tone-${view.tone}`}>{view.label}</span>
                  <time className="time" dateTime={s.lastReceivedUtc}>
                    {new Date(s.lastReceivedUtc).toLocaleTimeString()}
                  </time>
                  {why && <span className="reason">{why}</span>}
                </li>
              );
            })}
          </ul>
        )}
      </section>

      <Settings />
    </main>
  );
}
