import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState, type RefObject } from "react";
import { relativeTime } from "../format";
import { Banner } from "../ui/Banner";
import { Icon } from "../ui/Icon";
import { ActivityHost, type ActivityInfo, type PauseRequest } from "./ActivityHost";
import { HelpDialog } from "./HelpDialog";
import { formatPlayed } from "./playTime";
import { usePreview } from "./usePreview";

type Load = { status: "loading" } | { status: "ready"; list: ActivityInfo[] } | { status: "error" };

const CATEGORY: Record<string, string> = {
  break: "Break",
  arcade: "Game",
  productivity: "Focus",
  interactive: "Play",
};

interface Props {
  headingRef: RefObject<HTMLHeadingElement | null>;
  /** This view is showing. When it isn't, a running activity pauses. */
  active: boolean;
  pause: PauseRequest | null;
  now: number;
  onGoToSessions: () => void;
}

/** Disconnect while agents work: bundled activities, each in the sandboxed host. */
export default function ActivitiesView({ headingRef, active, pause, now, onGoToSessions }: Props) {
  const [load, setLoad] = useState<Load>({ status: "loading" });
  const [running, setRunning] = useState<ActivityInfo | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [helpFor, setHelpFor] = useState<ActivityInfo | null>(null);

  const refresh = useCallback(() => {
    invoke<ActivityInfo[]>("activities_list")
      .then((list) => setLoad({ status: "ready", list }))
      .catch(() => setLoad({ status: "error" }));
  }, []);

  useEffect(refresh, [refresh]);

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

  if (running) {
    return (
      <section aria-labelledby="activities-heading" className="view view-fill">
        <h1 id="activities-heading" ref={headingRef} tabIndex={-1} className="visually-hidden">
          Activities
        </h1>
        <ActivityHost
          activity={running}
          active={active}
          pause={pause}
          onClose={() => {
            setRunning(null);
            refresh();
          }}
          onGoToSessions={onGoToSessions}
        />
      </section>
    );
  }

  return (
    <section aria-labelledby="activities-heading" className="view">
      <header className="view-header">
        <h1 id="activities-heading" ref={headingRef} tabIndex={-1}>
          Activities
        </h1>
        <p className="view-lede">
          Step away while your agents work. Lullshift pauses and saves when one needs you.
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
      {load.status === "ready" && (
        <ul className="activity-grid">
          {load.list.map((a) => (
            <ActivityCard
              key={a.id}
              activity={a}
              now={now}
              onStart={() => {
                setMessage(null);
                setRunning(a);
              }}
              onStartOver={() => void startOver(a)}
              onHelp={() => setHelpFor(a)}
            />
          ))}
        </ul>
      )}
      <HelpDialog
        open={helpFor !== null}
        name={helpFor?.name ?? ""}
        description={helpFor?.description ?? ""}
        help={helpFor?.help ?? null}
        onClose={() => setHelpFor(null)}
      />
    </section>
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
      <span className="activity-kind">{CATEGORY[a.category] ?? a.category}</span>
      <h2 className="activity-name">{a.name}</h2>
      <p className="activity-desc">{a.description}</p>
      {(a.savedUtc || a.playedSeconds >= 60) && (
        <p className="activity-saved">
          {[
            a.savedUtc ? `Saved ${relativeTime(a.savedUtc, now)}` : null,
            a.playedSeconds >= 60 ? `Played ${formatPlayed(a.playedSeconds)}` : null,
          ]
            .filter(Boolean)
            .join(" · ")}
        </p>
      )}
      <div className="activity-actions">
        <button type="button" className="btn btn-primary" onClick={onStart}>
          <Icon name="play" size={18} />
          {a.savedUtc ? "Resume" : "Start"}
          <span className="visually-hidden"> {a.name}</span>
        </button>
        <button type="button" className="btn btn-quiet" onClick={onHelp}>
          <Icon name="help" size={18} />
          How to play
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
