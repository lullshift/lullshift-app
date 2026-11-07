import { invoke } from "@tauri-apps/api/core";
import { useEffect, useMemo, useState, type RefObject } from "react";
import type { ActivityInfo } from "../activity/ActivityHost";
import { pageOf, savedNewestFirst, type Page } from "../activity/library";
import { ResumeCard } from "../activity/ResumeCard";
import type { ActivityLoad } from "../activity/useActivityList";
import { providerName, relativeTime, sessionName, shortId } from "../format";
import { reasonText, sessionStateView, liveState, type SessionView } from "../sessions";
import {
  dateSeed,
  pickSuggestion,
  suggestionAction,
  waitFor,
  waitLine,
  type Suggestion,
  type WaitEstimate,
} from "../suggest";
import { Banner } from "../ui/Banner";
import { Icon, type IconName } from "../ui/Icon";
import { Mascot } from "../ui/Mascot";
import { StatePill } from "../ui/StatePill";
import { homeStatus, liveSessions } from "./status";

/** The strip shows at most this many live sessions. */
const STRIP_MAX = 4;
/** "Pick up where you left off" shows at most this many saved activities. */
const SAVED_MAX = 4;

interface Props {
  headingRef: RefObject<HTMLHeadingElement | null>;
  loading: boolean;
  coreError: boolean;
  hasLast: boolean;
  problems: string[];
  /** Every session, needs-you first. */
  sessions: SessionView[];
  staleAfterSeconds: number;
  now: number;
  activities: ActivityLoad;
  onRefresh: () => void;
  onGoToSessions: () => void;
  /** Opens Sessions with this session's detail panel. */
  onOpenSession: (s: SessionView) => void;
  onConnect: () => void;
  onOpenPage: (page: Page | "continue" | "companion") => void;
  onStartActivity: (a: ActivityInfo) => void;
}

/** Home (ADR-026 section 2): one question, what do I do while my agents work. */
export default function HomeView({
  headingRef,
  loading,
  coreError,
  hasLast,
  problems,
  sessions,
  staleAfterSeconds,
  now,
  activities,
  onRefresh,
  onGoToSessions,
  onOpenSession,
  onConnect,
  onOpenPage,
  onStartActivity,
}: Props) {
  const hasDashboard = !loading && (!coreError || hasLast);
  const status = homeStatus({ loading, coreError, hasDashboard, problems, sessions });
  const needs = sessions.filter((s) => liveState(s) === "needs_attention");
  const live = liveSessions(sessions);
  const working = sessions.filter((s) => liveState(s) === "working");

  const estimates = useWaitEstimates();
  const waiting = waitFor(
    working.map((s) => s.provider),
    estimates ?? [],
  );

  const list = activities.status === "ready" ? activities.list : null;
  const [shuffles, setShuffles] = useState(0);
  const seed = dateSeed(new Date(now));
  const suggestion = useMemo(
    () =>
      list
        ? pickSuggestion(
            {
              activities: list,
              estimateSeconds: waiting?.medianSeconds ?? null,
              anyWorking: working.length > 0,
              seed,
            },
            shuffles,
          )
        : null,
    [list, waiting?.medianSeconds, working.length, seed, shuffles],
  );
  const saved = list ? savedNewestFirst(list) : [];

  return (
    <section aria-labelledby="home-heading" className="view home">
      <header
        className={needs.length > 0 ? "home-hero home-needs" : "home-hero"}
        aria-labelledby="home-heading"
      >
        <Mascot face={status.face} size={needs.length > 0 ? 72 : 92} />
        <div className="home-hero-text">
          <h1 id="home-heading" ref={headingRef} tabIndex={-1} className="home-title">
            {status.title}
          </h1>
          {needs.length > 0 && <p className="home-needs-who">{needsLine(needs, now)}</p>}
          <p className="view-lede">{status.line}</p>
        </div>
        {needs.length > 0 && (
          <button
            type="button"
            className="btn btn-primary"
            onClick={() => (needs.length === 1 ? onOpenSession(needs[0]!) : onGoToSessions())}
          >
            {needs.length === 1 ? "Go to session" : "Go to sessions"}
          </button>
        )}
      </header>

      {loading && (
        <div className="skeletons" role="status">
          <span className="visually-hidden">Loading your agents…</span>
          <div className="skeleton" />
        </div>
      )}
      {!loading && coreError && (
        <Banner tone="danger">
          {hasLast
            ? "Lullshift couldn't refresh your agents just now. What you see may be out of date."
            : "Lullshift couldn't read your agents."}{" "}
          <button type="button" className="link-btn" onClick={onRefresh}>
            Try again
          </button>
        </Banner>
      )}
      {problems.map((p) => (
        <Banner key={p} tone="danger">
          {p}
        </Banner>
      ))}

      {hasDashboard && status.tone === "calm" && sessions.length === 0 && (
        <div>
          <button type="button" className="btn btn-secondary" onClick={onConnect}>
            <Icon name="plug" size={18} />
            Connect an agent
          </button>
        </div>
      )}

      {live.length > 0 && (
        <section aria-label="Your agents" className="home-strip-wrap">
          <ul className="home-strip">
            {live.slice(0, STRIP_MAX).map((s) => (
              <AgentChip
                key={`${s.provider}/${s.providerSessionId}`}
                session={s}
                now={now}
                staleAfterSeconds={staleAfterSeconds}
              />
            ))}
          </ul>
          {live.length > STRIP_MAX && (
            <button type="button" className="link-quiet" onClick={onGoToSessions}>
              All sessions
            </button>
          )}
        </section>
      )}

      <FitsYourWait
        activities={activities}
        line={waitLine(waiting, working.length > 0)}
        suggestion={suggestion}
        onStart={onStartActivity}
        onShuffle={() => setShuffles((n) => n + 1)}
        announce={shuffles > 0}
        canShuffle={(list?.length ?? 0) > 1}
        onManage={() => onOpenPage("play")}
        onRetry={onRefresh}
      />

      <section aria-labelledby="home-doors" className="home-section">
        <h2 className="section-title" id="home-doors">
          While you wait
        </h2>
        <ul className="home-doors">
          <Door
            icon="play"
            title="Play"
            text="Short games that pause the moment an agent needs you."
            go="Open Play"
            onClick={() => onOpenPage("play")}
          />
          <Door
            icon="activities"
            title="Rest"
            text="Breathe, stretch, look outside or step away for a bit."
            go="Open Rest"
            onClick={() => onOpenPage("rest")}
          />
          <Door
            icon="chat"
            title="Think ahead"
            text="Talk through your next move with Companion."
            go="Open Companion"
            onClick={() => onOpenPage("companion")}
          />
        </ul>
      </section>

      {saved.length > 0 && (
        <section aria-labelledby="home-saved" className="home-section">
          <div className="home-section-head">
            <h2 className="section-title" id="home-saved">
              Pick up where you left off
            </h2>
            {saved.length > SAVED_MAX && (
              <button type="button" className="link-quiet" onClick={() => onOpenPage("continue")}>
                All saved
              </button>
            )}
          </div>
          <ul className="resume-list resume-list-row">
            {saved.slice(0, SAVED_MAX).map((a) => (
              <ResumeCard key={a.id} activity={a} now={now} onResume={() => onStartActivity(a)} />
            ))}
          </ul>
        </section>
      )}
    </section>
  );
}

/** What each tool's wait usually is. Null until read, and null if it can't be: no estimate. */
function useWaitEstimates(): WaitEstimate[] | null {
  const [estimates, setEstimates] = useState<WaitEstimate[] | null>(null);
  useEffect(() => {
    let live = true;
    invoke<WaitEstimate[]>("wait_estimates")
      .then((e) => live && setEstimates(e))
      .catch(() => live && setEstimates(null));
    return () => {
      live = false;
    };
  }, []);
  return estimates;
}

function needsLine(needs: SessionView[], now: number): string {
  const [first] = needs;
  if (!first) return "";
  if (needs.length > 1) {
    return [...new Set(needs.map((s) => providerName(s.provider)))].join(", ");
  }
  const name = sessionName(first);
  const since = relativeTime(first.lastSeenUtc, now);
  return name.source === "tool" ? `Waiting for you · ${since}` : `${name.primary} · ${since}`;
}

function AgentChip({
  session: s,
  now,
  staleAfterSeconds,
}: {
  session: SessionView;
  now: number;
  staleAfterSeconds: number;
}) {
  const name = sessionName(s);
  const why = reasonText(s, staleAfterSeconds);
  return (
    <li className="home-chip">
      <StatePill view={sessionStateView(s)} />
      <span className="home-chip-tool">{providerName(s.provider)}</span>
      <span className="home-chip-sub">
        {name.source === "tool" ? shortId(s.providerSessionId) : name.primary}
      </span>
      <span className="home-chip-time" title={why ?? undefined}>
        {why ? "not confirmed" : relativeTime(s.lastSeenUtc, now)}
      </span>
    </li>
  );
}

function Door({
  icon,
  title,
  text,
  go,
  onClick,
}: {
  icon: IconName;
  title: string;
  text: string;
  go: string;
  onClick: () => void;
}) {
  return (
    <li>
      <button type="button" className="home-door" onClick={onClick}>
        <span className="home-door-icon" aria-hidden="true">
          <Icon name={icon} size={24} />
        </span>
        <span className="home-door-title">{title}</span>
        <span className="home-door-text">{text}</span>
        <span className="home-door-go">
          {go}
          <Icon name="arrow" size={16} />
        </span>
      </button>
    </li>
  );
}

function FitsYourWait({
  activities,
  line,
  suggestion,
  onStart,
  onShuffle,
  announce,
  canShuffle,
  onManage,
  onRetry,
}: {
  activities: ActivityLoad;
  line: string;
  suggestion: Suggestion<ActivityInfo> | null;
  onStart: (a: ActivityInfo) => void;
  onShuffle: () => void;
  /** Say what is suggested now: only after a Shuffle click, never on a recompute. */
  announce: boolean;
  canShuffle: boolean;
  onManage: () => void;
  onRetry: () => void;
}) {
  if (activities.status === "loading") {
    return (
      <div className="skeletons" role="status">
        <span className="visually-hidden">Loading a suggestion…</span>
        <div className="skeleton" />
      </div>
    );
  }
  if (activities.status === "error") {
    return (
      <Banner tone="danger">
        Activities couldn't be listed, so there is no suggestion.{" "}
        <button type="button" className="link-btn" onClick={onRetry}>
          Try again
        </button>
      </Banner>
    );
  }
  if (!suggestion) {
    return (
      <section className="card home-suggest" aria-labelledby="home-suggest-title">
        <p className="home-suggest-line">{line}</p>
        <h2 id="home-suggest-title" className="home-suggest-name">
          No activities are turned on
        </h2>
        <p className="home-suggest-desc">Turn some on, or add more from a marketplace.</p>
        <div className="home-suggest-actions">
          <button type="button" className="btn btn-secondary" onClick={onManage}>
            Manage activities
          </button>
        </div>
      </section>
    );
  }
  const a = suggestion.activity;
  const action = suggestionAction(suggestion.kind);
  return (
    <section className="card home-suggest" aria-labelledby="home-suggest-title">
      <p className="home-suggest-line">{line}</p>
      <h2 id="home-suggest-title" className="home-suggest-name">
        {a.name}
      </h2>
      <p className="home-suggest-desc">
        {a.description}
        {suggestion.kind === "resume" && a.savedUtc ? " You left it saved." : ""}
      </p>
      <div className="home-suggest-actions">
        <button type="button" className="btn btn-primary" onClick={() => onStart(a)}>
          <Icon name={suggestion.kind === "resume" ? "resume" : "play"} size={18} />
          {action}
          <span className="visually-hidden"> {a.name}</span>
        </button>
        {canShuffle && (
          <button type="button" className="btn btn-quiet" onClick={onShuffle}>
            <Icon name="shuffle" size={18} />
            Shuffle
          </button>
        )}
        <span className="visually-hidden" role="status">
          {announce
            ? `Suggesting ${a.name}, ${pageOf(a.category) === "rest" ? "a rest" : "a game"}.`
            : ""}
        </span>
      </div>
    </section>
  );
}
