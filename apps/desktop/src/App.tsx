import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { sortSessions, stateView, type SessionSnapshot } from "./sessions";

type Feed =
  | { status: "loading" }
  | { status: "ready"; sessions: SessionSnapshot[] }
  | { status: "error"; message: string };

export default function App() {
  const [feed, setFeed] = useState<Feed>({ status: "loading" });

  useEffect(() => {
    let cancelled = false;
    invoke<SessionSnapshot[]>("mock_sessions")
      .then((sessions) => !cancelled && setFeed({ status: "ready", sessions }))
      .catch((err: unknown) => !cancelled && setFeed({ status: "error", message: String(err) }));
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <main className="app">
      <header>
        <h1>Lullshift</h1>
        <p className="note">
          Development scaffold. The sessions below come from a <strong>mock agent feed</strong>, not
          from real agents.
        </p>
      </header>

      <section aria-labelledby="sessions-heading">
        <h2 id="sessions-heading">Agent sessions</h2>
        {feed.status === "loading" && <p role="status">Loading sessions…</p>}
        {feed.status === "error" && (
          <p role="alert">
            Couldn't reach the Lullshift core ({feed.message}). Agent status is unknown.
          </p>
        )}
        {feed.status === "ready" && feed.sessions.length === 0 && <p>No agent sessions yet.</p>}
        {feed.status === "ready" && feed.sessions.length > 0 && (
          <ul className="sessions">
            {sortSessions(feed.sessions).map((s) => {
              const view = stateView(s.state);
              return (
                <li key={`${s.provider}:${s.providerSessionId}`} className="session">
                  <span className="title">{s.title}</span>
                  <span className="provider">{s.provider}</span>
                  <span className={`state tone-${view.tone}`}>{view.label}</span>
                  <span className="time">
                    {s.lastEventUtc ? new Date(s.lastEventUtc).toLocaleTimeString() : "no events"}
                  </span>
                </li>
              );
            })}
          </ul>
        )}
      </section>
    </main>
  );
}
