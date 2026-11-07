// Runs one activity in a sandboxed iframe and owns its interruption (S03-T02, S03-T03, ADR-012).
//
// Pausing never depends on the activity: the host asks it for its state, waits briefly, then
// removes the iframe, so nothing in it keeps running. Resuming loads a fresh copy and hands it
// the last saved state. Every state the activity reports (on request or on its own) is saved
// to the local database right away, so a crash or restart loses at most the last few moves.

import type { Help } from "@lullshift/activity-sdk";
import {
  RateLimiter,
  hostEnvelope,
  newNonce,
  parseActivityMessage,
  type HostMessage,
  type SummaryItem,
} from "@lullshift/activity-sdk/host";
import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState, type MutableRefObject } from "react";
import { providerName } from "../format";
import { activitySourceLabel } from "../marketplace/logic";
import { effectiveTheme } from "../theme";
import { uiPrefsStore } from "../uiPrefsStore";
import { Icon } from "../ui/Icon";
import { Mascot } from "../ui/Mascot";
import { loadContent, type ContentProblem } from "./content";
import { HelpDialog } from "./HelpDialog";
import { helpLabel } from "./library";
import { PausedStats } from "./PausedStats";
import { usePreview } from "./usePreview";

export interface ActivityInfo {
  id: string;
  name: string;
  description: string;
  category: string;
  /** Manifest tags (untrusted): searchable and matched to the host's own chips, never shown. */
  tags?: string[];
  version: string;
  storage: boolean;
  savedUtc: string | null;
  /** How-to-play help from the manifest; null if it has none. */
  help: Help | null;
  /** The manifest names a preview image (fetched with `activity_preview`). */
  preview: boolean;
  /** Total time played before this visit, in seconds. */
  playedSeconds: number;
  /** Content packs: the engine that runs them (`id@major`); null for apps. */
  engine: string | null;
  /** Where it came from. Absent in older builds. */
  source?: { marketplaceId: string; builtin: boolean; commit: string | null };
}

/** A pause the host forced; `seq` changes for every new request. */
export interface PauseRequest {
  seq: number;
  reason: "attention";
  provider: string;
}

type PauseReason = "attention" | "user" | "away" | "unresponsive" | "navigated";

type Phase =
  | { kind: "loading" }
  | { kind: "running" }
  | { kind: "pausing" }
  | { kind: "paused"; reason: PauseReason; provider?: string; saved: boolean }
  | { kind: "failed" }
  /** A content pack whose data or engine failed the host's check; never started. */
  | { kind: "invalid"; problem: ContentProblem };

/** How long an activity has to answer a save request before the host stops waiting. */
const SAVE_TIMEOUT_MS = 800;
/** How long it has to say it's ready after loading. */
const READY_TIMEOUT_MS = 8000;
/** While running, the host asks for state this often (on top of the activity's own saves). */
const AUTOSAVE_MS = 15_000;
/** Missed save requests in a row before the host treats the activity as stuck. */
const MAX_MISSED = 3;

const FRAME_ALLOW = [
  "camera",
  "microphone",
  "geolocation",
  "clipboard-read",
  "clipboard-write",
  "display-capture",
  "fullscreen",
  "payment",
  "usb",
  "serial",
  "hid",
  "bluetooth",
]
  .map((f) => `${f} 'none'`)
  .join("; ");

interface Props {
  activity: ActivityInfo;
  /** False while another view is showing: the activity pauses. */
  active: boolean;
  pause: PauseRequest | null;
  onClose: () => void;
  onGoToSessions: () => void;
  /** Lets the page above pause this launch (and wait for its save) or resume it directly. */
  controlRef?: MutableRefObject<HostControl | null>;
}

/** What the page above may ask of a running host. */
export interface HostControl {
  /** Pauses and saves (the normal path), resolving once the save finished or timed out. */
  pauseNow: () => Promise<void>;
  /** Resumes a paused launch in one step; does nothing in any other phase. */
  resume: () => void;
}

export function ActivityHost({
  activity,
  active,
  pause,
  onClose,
  onGoToSessions,
  controlRef,
}: Props) {
  const [phase, setPhaseState] = useState<Phase>({ kind: "loading" });
  // Empty until the saved state is loaded; the iframe is created only after that.
  const [nonce, setNonce] = useState("");
  // This launch's address, under a key the host just issued (ADR-012): the scheme handler
  // serves only this activity's files to it, and stops when the launch ends.
  const [frameUrl, setFrameUrl] = useState("");
  // How many times the frame has loaded a document in this launch. A second load means the
  // activity navigated itself (or was sent) somewhere else.
  const frameLoads = useRef(0);
  // The activity's latest stats for the paused view; this visit only, never stored.
  const [summary, setSummary] = useState<SummaryItem[] | null>(null);
  // Seconds played this visit (shown) and not yet added to the stored total.
  const [playedNow, setPlayedNow] = useState(0);
  const unsaved = useRef(0);
  // Bumped by every pause and every start: a start that finds it changed (or the host hidden or
  // unmounted) when its async steps finish must not mount the frame.
  const generation = useRef(0);
  const activeRef = useRef(active);
  const mounted = useRef(true);
  // The generation of the start in flight (0 if none): a resume joins it instead of starting again.
  const starting = useRef(0);
  useEffect(() => {
    activeRef.current = active;
  }, [active]);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  const launched = useRef(false);
  const [helpOpen, setHelpOpen] = useState(false);
  const preview = usePreview(activity.id, activity.preview);
  const frameRef = useRef<HTMLIFrameElement>(null);
  const restoreRef = useRef<unknown>(null);
  // A pack's checked data for `init.content`; null for apps.
  const contentRef = useRef<unknown>(null);
  const phaseRef = useRef<Phase>({ kind: "loading" });
  const pending = useRef(new Map<string, (ok: boolean) => void>());
  const missed = useRef(0);
  const counter = useRef(0);
  // Created on the first message (a clock read isn't allowed during render).
  const limiter = useRef<RateLimiter | null>(null);
  const dropped = useRef(0);
  // The last pause request acted on. Requests from before this activity started don't apply,
  // and each request pauses once: resuming must not replay it.
  const handledPause = useRef(pause?.seq ?? 0);
  // Saves run one at a time, in the order the activity sent them, so an older state never
  // lands after a newer one. Starting a launch waits for this chain before reading.
  const saveChain = useRef<Promise<unknown>>(Promise.resolve());
  // The save request of a pause in progress, so a second caller can wait for the same pause.
  const freezing = useRef<Promise<unknown>>(Promise.resolve());
  // The ref lets async steps (a save finishing) see the phase as it is now, not as rendered.
  const setPhase = useCallback((next: Phase) => {
    phaseRef.current = next;
    setPhaseState(next);
  }, []);

  const post = useCallback(
    (message: HostMessage) => {
      frameRef.current?.contentWindow?.postMessage(hostEnvelope(nonce, message), "*");
    },
    [nonce],
  );

  const persist = useCallback(
    (state: unknown) => {
      const text = JSON.stringify(state);
      if (text === undefined) return Promise.resolve(false);
      const result = saveChain.current.then(() =>
        invoke("activity_save_snapshot", { id: activity.id, snapshot: text }).then(
          () => {
            restoreRef.current = state;
            return true;
          },
          () => false,
        ),
      );
      saveChain.current = result;
      return result;
    },
    [activity.id],
  );

  /** Asks for state; resolves true once it is saved, false on timeout or failure. */
  const requestSave = useCallback(() => {
    counter.current += 1;
    const requestId = `s${counter.current}`;
    return new Promise<boolean>((resolve) => {
      const timer = window.setTimeout(() => {
        pending.current.delete(requestId);
        resolve(false);
      }, SAVE_TIMEOUT_MS);
      pending.current.set(requestId, (ok) => {
        window.clearTimeout(timer);
        resolve(ok);
      });
      post({ type: "save", requestId });
    });
  }, [post]);

  /** Saves if it can, then removes the iframe. Works whether or not the activity cooperates. */
  const freeze = useCallback(
    async (reason: PauseReason, provider?: string) => {
      generation.current += 1;
      const current = phaseRef.current.kind;
      if (current === "pausing") {
        await freezing.current;
        return;
      }
      if (current === "paused" || current === "failed" || current === "invalid") return;
      let saved = false;
      // A frame that navigated away no longer runs the activity's bridge, so there is nothing
      // to ask: keep the last save and remove it at once.
      if (current === "running" && reason !== "navigated") {
        setPhase({ kind: "pausing" });
        post({ type: "pause" });
        const request = requestSave();
        freezing.current = request;
        saved = await request;
      }
      setPhase({ kind: "paused", reason, provider, saved });
    },
    [post, requestSave, setPhase],
  );

  /**
   * Checks a pack's data (Rust validates it against its engine's schema), loads the last saved
   * state, then creates a fresh iframe for this launch. A pack that fails the check never starts.
   */
  const { id: activityId, engine } = activity;
  const start = useCallback(() => {
    const mine = (generation.current += 1);
    starting.current = mine;
    // Still wanted: this launch is the latest, the host is mounted and showing.
    const wanted = () => mounted.current && generation.current === mine && activeRef.current;
    // Not wanted any more: if a pause came in meanwhile it already set the phase; if the host was
    // merely hidden, it stays paused.
    const dropped = () => {
      if (mounted.current && generation.current === mine && phaseRef.current.kind !== "paused") {
        setPhase({ kind: "paused", reason: "away", saved: false });
      }
    };
    return saveChain.current
      .then(() => loadContent({ id: activityId, engine }))
      .then((checked) => {
        if (!wanted()) return dropped();
        if (!checked.ok) {
          setPhase({ kind: "invalid", problem: checked.problem });
          return;
        }
        contentRef.current = checked.content;
        return invoke<{ snapshot: string } | null>("activity_snapshot", { id: activityId })
          .then((snap) => (snap ? (JSON.parse(snap.snapshot) as unknown) : null))
          .catch(() => null)
          .then((restore) => {
            if (!wanted()) return dropped();
            restoreRef.current = restore;
            missed.current = 0;
            pending.current.clear();
            // A fresh key per launch; it replaces any earlier key for this activity.
            return invoke<string>("activity_launch", { id: activityId }).then(
              (url) => {
                if (!wanted()) {
                  // Paused or closed while the key was being issued: end it, stay paused.
                  void invoke("activity_launch_end", { id: activityId }).catch(() => undefined);
                  return dropped();
                }
                frameLoads.current = 0;
                setFrameUrl(url);
                setNonce(newNonce());
                setPhase({ kind: "loading" });
              },
              () => {
                if (wanted()) setPhase({ kind: "failed" });
              },
            );
          });
      })
      .finally(() => {
        if (starting.current === mine) starting.current = 0;
      });
  }, [activityId, engine, setPhase]);

  useEffect(() => {
    if (!controlRef) return;
    controlRef.current = {
      pauseNow: async () => {
        await freeze("user");
        await saveChain.current;
      },
      resume: () => {
        void (async () => {
          // A pause still saving finishes first, so one click is enough.
          if (phaseRef.current.kind === "pausing") {
            await freezing.current;
            await Promise.resolve();
          }
          if (phaseRef.current.kind === "paused" && starting.current !== generation.current) {
            void start();
          }
        })();
      },
    };
    return () => {
      controlRef.current = null;
    };
  }, [controlRef, freeze, start]);

  // First load.
  useEffect(() => {
    void start();
  }, [start]);

  // Messages from the activity: only from this iframe, this launch, valid and within limits.
  useEffect(() => {
    const onMessage = (event: MessageEvent) => {
      const frame = frameRef.current?.contentWindow;
      if (!frame || event.source !== frame || event.origin !== "null") return;
      const clock = performance.now();
      limiter.current ??= new RateLimiter(30, 120, clock);
      if (!limiter.current.take(clock)) {
        dropped.current += 1;
        return;
      }
      const parsed = parseActivityMessage(event.data, nonce);
      if (!parsed.ok) {
        dropped.current += 1;
        // An oversized message from this launch is never an accident: close the activity
        // rather than let it keep the app busy decoding more.
        if (parsed.reason === "size") {
          void freeze("unresponsive");
          return;
        }
        // A storage call with a bad key or value still gets an answer, so the activity
        // doesn't wait forever. Nothing is stored.
        const data = event.data as { type?: unknown; requestId?: unknown } | null;
        if (
          parsed.reason === "field" &&
          data?.type === "storage" &&
          typeof data.requestId === "string" &&
          data.requestId.length <= 64
        ) {
          post({
            type: "storage-result",
            requestId: data.requestId,
            ok: false,
            error: "invalid_request",
          });
        }
        return;
      }
      const m = parsed.message;
      if (m.type === "ready") {
        if (phaseRef.current.kind !== "loading") return;
        // Hidden or paused while it loaded: never run behind the user's back.
        if (!activeRef.current) {
          void freeze("away");
          return;
        }
        post({
          type: "init",
          restore: restoreRef.current,
          reducedMotion: window.matchMedia("(prefers-reduced-motion: reduce)").matches,
          theme: effectiveTheme(uiPrefsStore.getState().prefs.theme),
          content: contentRef.current,
        });
        setPhase({ kind: "running" });
        frameRef.current?.focus();
      } else if (m.type === "state") {
        // Before init the activity hasn't seen its saved state, so what it reports would
        // overwrite good progress with a blank start.
        const now = phaseRef.current.kind;
        if (now !== "running" && now !== "pausing") return;
        void persist(m.state).then((ok) => {
          if (m.requestId) {
            missed.current = 0;
            pending.current.get(m.requestId)?.(ok);
            pending.current.delete(m.requestId);
          }
        });
      } else if (m.type === "summary") {
        const now = phaseRef.current.kind;
        if (now === "running" || now === "pausing") setSummary(m.items);
      } else if (m.type === "storage") {
        const reply = (ok: boolean, value?: string | null, error?: string) =>
          post({ type: "storage-result", requestId: m.requestId, ok, value, error });
        if (!activity.storage) {
          reply(false, undefined, "not_permitted");
          return;
        }
        const call =
          m.op === "get"
            ? invoke<string | null>("activity_storage_get", { id: activity.id, key: m.key })
            : m.op === "set"
              ? invoke<null>("activity_storage_set", {
                  id: activity.id,
                  key: m.key,
                  value: m.value,
                })
              : invoke<null>("activity_storage_remove", { id: activity.id, key: m.key });
        call.then(
          (value) => reply(true, value ?? null),
          (err: unknown) => reply(false, undefined, String(err)),
        );
      }
    };
    window.addEventListener("message", onMessage);
    return () => window.removeEventListener("message", onMessage);
  }, [nonce, post, persist, freeze, activity.id, activity.storage, setPhase]);

  // Time played: measured by the host while the activity runs (it only runs while visible), and
  // added to the stored total whenever it stops running (S05-T09).
  const flushPlay = useCallback(() => {
    const seconds = Math.round(unsaved.current);
    if (seconds < 1) return;
    unsaved.current -= seconds;
    const launch = !launched.current;
    launched.current = true;
    void invoke("activity_add_play", { id: activity.id, seconds, launch }).catch(() => {
      // Not saved: the total is a little short. Nothing else depends on it.
    });
  }, [activity.id]);
  useEffect(() => {
    if (phase.kind !== "running") return;
    const started = performance.now();
    return () => {
      const seconds = (performance.now() - started) / 1000;
      unsaved.current += seconds;
      setPlayedNow((p) => p + seconds);
      flushPlay();
    };
  }, [phase.kind, flushPlay]);

  // An activity that never says it's ready is shown as failed, not left spinning.
  useEffect(() => {
    if (phase.kind !== "loading") return;
    const timer = window.setTimeout(() => {
      if (phaseRef.current.kind === "loading") setPhase({ kind: "failed" });
    }, READY_TIMEOUT_MS);
    return () => window.clearTimeout(timer);
  }, [phase.kind, nonce, setPhase]);

  // Regular saves while running; an activity that stops answering is closed.
  useEffect(() => {
    if (phase.kind !== "running") return;
    const timer = window.setInterval(() => {
      void requestSave().then((ok) => {
        missed.current = ok ? 0 : missed.current + 1;
        if (missed.current >= MAX_MISSED) void freeze("unresponsive");
      });
    }, AUTOSAVE_MS);
    return () => window.clearInterval(timer);
  }, [phase.kind, requestSave, freeze]);

  // An agent needs the user.
  useEffect(() => {
    if (!pause || pause.seq <= handledPause.current) return;
    handledPause.current = pause.seq;
    void freeze(pause.reason, pause.provider);
  }, [pause, freeze]);

  // Another view, or the window hidden or minimized.
  useEffect(() => {
    if (!active) void freeze("away");
  }, [active, freeze]);
  useEffect(() => {
    const onVisibility = () => {
      if (document.visibilityState === "hidden") void freeze("away");
    };
    document.addEventListener("visibilitychange", onVisibility);
    return () => document.removeEventListener("visibilitychange", onVisibility);
  }, [freeze]);

  const live =
    nonce !== "" &&
    frameUrl !== "" &&
    (phase.kind === "loading" || phase.kind === "running" || phase.kind === "pausing");
  // The launch key stops working as soon as the frame is gone (paused, failed, closed or
  // replaced by a newer launch).
  useEffect(() => {
    if (!live) return;
    return () => {
      void invoke("activity_launch_end", { id: activityId }).catch(() => {
        // Nothing to undo: a newer launch already replaced the key.
      });
    };
  }, [live, activityId]);

  // The frame loads its page once per launch. A second load means it navigated (to another
  // activity's page, say): freeze it. Server-side the key already refuses other activities.
  const onFrameLoad = () => {
    frameLoads.current += 1;
    if (frameLoads.current > 1) void freeze("navigated");
  };

  const close = async () => {
    if (phaseRef.current.kind === "running") await freeze("user");
    onClose();
  };

  // Reading help pauses the activity, so nothing happens behind the dialog.
  const openHelp = () => {
    if (phaseRef.current.kind === "running") void freeze("user");
    setHelpOpen(true);
  };

  // Third-party code and packs say where they came from (owner decision 27.5).
  const sourceLabel = activitySourceLabel(activity.source, activity.engine);

  return (
    <section className="host" aria-label={activity.name}>
      <header className="host-bar">
        <h2 className="host-title">{activity.name}</h2>
        {sourceLabel && <span className="host-source">{sourceLabel}</span>}
        <span className="host-status" role="status">
          {phase.kind === "loading"
            ? "Starting…"
            : phase.kind === "pausing"
              ? "Saving…"
              : phase.kind === "paused"
                ? "Paused"
                : phase.kind === "failed" || phase.kind === "invalid"
                  ? "Didn't start"
                  : ""}
        </span>
        <button type="button" className="btn btn-quiet" onClick={openHelp}>
          <Icon name="help" size={18} />
          {helpLabel(activity.category)}
        </button>
        {phase.kind === "running" && (
          <button type="button" className="btn btn-quiet" onClick={() => void freeze("user")}>
            <Icon name="pause" size={18} />
            Pause
          </button>
        )}
        <button type="button" className="btn btn-quiet" onClick={() => void close()}>
          <Icon name="close" size={18} />
          Close
        </button>
      </header>

      <div className="host-stage">
        {live && (
          <iframe
            key={nonce}
            ref={frameRef}
            className="host-frame"
            title={activity.name}
            src={`${frameUrl}#n=${nonce}`}
            onLoad={onFrameLoad}
            sandbox="allow-scripts"
            allow={FRAME_ALLOW}
            referrerPolicy="no-referrer"
          />
        )}
        {phase.kind === "paused" && (
          <PausedCard
            phase={phase}
            name={activity.name}
            category={activity.category}
            summary={summary}
            playedNow={playedNow}
            preview={preview}
            onResume={() => void start()}
            onHelp={openHelp}
            onGoToSessions={onGoToSessions}
          />
        )}
        {phase.kind === "failed" && (
          <div className="host-card" role="alert">
            <h3>{activity.name} didn't start</h3>
            <p>Nothing was lost. Try again, or pick something else.</p>
            <div className="host-actions">
              <button type="button" className="btn btn-primary" onClick={() => void start()}>
                Try again
              </button>
              <button type="button" className="btn btn-quiet" onClick={onClose}>
                Back to activities
              </button>
            </div>
          </div>
        )}
        {phase.kind === "invalid" && (
          <InvalidPackCard name={activity.name} problem={phase.problem} onClose={onClose} />
        )}
      </div>
      <HelpDialog
        open={helpOpen}
        name={activity.name}
        description={activity.description}
        help={activity.help}
        category={activity.category}
        onClose={() => setHelpOpen(false)}
      />
    </section>
  );
}

/**
 * A pack that failed the host's check (ADR-016). Calm and honest: it says what happened, that
 * nothing ran and nothing was lost. Retrying wouldn't help (bundled data doesn't change), so the
 * only action is going back. The detail is plain text from the host's check.
 */
function InvalidPackCard({
  name,
  problem,
  onClose,
}: {
  name: string;
  problem: ContentProblem;
  onClose: () => void;
}) {
  const back = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    const at = document.activeElement;
    if (!at || at === document.body) back.current?.focus();
  }, []);
  return (
    <div className="host-card" role="alert">
      <Mascot face="asleep" size={72} />
      <h3>{name} can't start</h3>
      <p>{problem.message} Nothing was lost.</p>
      {problem.detail && <p className="host-detail">{problem.detail}</p>}
      <div className="host-actions">
        <button type="button" className="btn btn-primary" onClick={onClose} ref={back}>
          Back to activities
        </button>
      </div>
    </div>
  );
}

function PausedCard({
  phase,
  name,
  category,
  summary,
  playedNow,
  preview,
  onResume,
  onHelp,
  onGoToSessions,
}: {
  phase: Extract<Phase, { kind: "paused" }>;
  name: string;
  category: string;
  summary: SummaryItem[] | null;
  playedNow: number;
  preview: string | null;
  onResume: () => void;
  onHelp: () => void;
  onGoToSessions: () => void;
}) {
  const savedText = phase.saved ? "Your progress is saved." : "Your last saved progress is kept.";
  const attention = phase.reason === "attention";
  const primary = useRef<HTMLButtonElement>(null);
  // Removing the activity frame drops keyboard focus to the page body; catch it here. Focus that
  // is somewhere else on purpose (another view, the sidebar) is left alone.
  useEffect(() => {
    const at = document.activeElement;
    if (!at || at === document.body) primary.current?.focus();
  }, []);
  return (
    <div className="host-card" role={attention ? "alert" : "status"}>
      <Mascot face={attention ? "awake" : "asleep"} size={72} />
      <h3>
        {attention
          ? `${providerName(phase.provider ?? "")} needs you`
          : phase.reason === "unresponsive"
            ? `${name} stopped responding`
            : phase.reason === "navigated"
              ? `${name} was stopped`
              : `${name} is paused`}
      </h3>
      <p>
        {attention
          ? `${name} is paused. ${savedText}`
          : phase.reason === "unresponsive"
            ? `Lullshift closed it so it can't slow anything down. ${savedText}`
            : phase.reason === "navigated"
              ? `It tried to open another page, which Lullshift doesn't allow. ${savedText}`
              : phase.reason === "away"
                ? `Paused while you were away. ${savedText}`
                : savedText}
      </p>
      <PausedStats name={name} summary={summary} playedNow={playedNow} preview={preview} />
      <div className="host-actions">
        {attention && (
          <button type="button" className="btn btn-primary" onClick={onGoToSessions} ref={primary}>
            Go to Sessions
            <Icon name="arrow" size={18} />
          </button>
        )}
        <button
          type="button"
          className={attention ? "btn btn-secondary" : "btn btn-primary"}
          onClick={onResume}
          ref={attention ? undefined : primary}
        >
          <Icon name="play" size={18} />
          {attention ? "Keep playing" : "Resume"}
        </button>
        <button type="button" className="btn btn-quiet" onClick={onHelp}>
          <Icon name="help" size={18} />
          {helpLabel(category)}
        </button>
      </div>
    </div>
  );
}
