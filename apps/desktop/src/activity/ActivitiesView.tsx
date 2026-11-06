import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState, type RefObject } from "react";
import { relativeTime } from "../format";
import { Banner } from "../ui/Banner";
import { focusIfLost } from "../ui/focus";
import { Icon } from "../ui/Icon";
import { truncate } from "../marketplace/logic";
import { ManagePanel } from "../marketplace/ManagePanel";
import { MarketplacesPanel } from "../marketplace/MarketplacesPanel";
import { ActivityHost, type ActivityInfo, type PauseRequest } from "./ActivityHost";
import { HelpDialog } from "./HelpDialog";
import { formatPlayed } from "./playTime";
import { usePreview } from "./usePreview";

type Load = { status: "loading" } | { status: "ready"; list: ActivityInfo[] } | { status: "error" };

type Tab = "library" | "manage" | "marketplaces";

const TABS: { id: Tab; label: string }[] = [
  { id: "library", label: "Library" },
  { id: "manage", label: "Manage" },
  { id: "marketplaces", label: "Marketplaces" },
];

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
  const [tab, setTab] = useState<Tab>("library");

  const refresh = useCallback(() => {
    invoke<ActivityInfo[]>("activities_list")
      .then((list) => setLoad({ status: "ready", list }))
      .catch(() => setLoad({ status: "error" }));
  }, []);

  useEffect(refresh, [refresh]);

  // Closing an activity removes the player, so focus would drop to the page: put it back on the
  // Activities heading. Only when it was lost, and only after a real close.
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

      <nav aria-label="Activities sections">
        <ul className="subtabs">
          {TABS.map((t) => (
            <li key={t.id}>
              <button
                type="button"
                className="subtab"
                aria-current={tab === t.id ? "page" : undefined}
                onClick={() => {
                  setTab(t.id);
                  if (t.id === "library") refresh();
                }}
              >
                {t.label}
              </button>
            </li>
          ))}
        </ul>
      </nav>

      {tab === "manage" && <ManagePanel onChanged={refresh} />}
      {tab === "marketplaces" && <MarketplacesPanel now={now} onChanged={refresh} />}
      {tab === "library" && message && <Banner tone="success">{message}</Banner>}
      {tab === "library" && load.status === "loading" && (
        <div className="skeletons" role="status">
          <span className="visually-hidden">Loading activities…</span>
          <div className="skeleton" />
        </div>
      )}
      {tab === "library" && load.status === "error" && (
        <Banner tone="danger">
          Activities couldn't be listed. Restart Lullshift to try again.
        </Banner>
      )}
      {tab === "library" && load.status === "ready" && load.list.length === 0 && (
        <Banner tone="info">
          No activities are turned on. Turn some on under Manage, or add more under Marketplaces.
        </Banner>
      )}
      {tab === "library" && load.status === "ready" && (
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
      {a.source && !a.source.builtin && (
        <span className="activity-source">From {truncate(a.source.marketplaceId, 40)}</span>
      )}
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
