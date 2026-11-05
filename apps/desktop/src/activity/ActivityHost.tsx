// Runs one activity in a sandboxed iframe and owns its interruption (S03-T02, S03-T03, ADR-012).
//
// Pausing never depends on the activity: the host asks it for its state, waits briefly, then
// removes the iframe, so nothing in it keeps running. Resuming loads a fresh copy and hands it
// the last saved state. Every state the activity reports (on request or on its own) is saved
// to the local database right away, so a crash or restart loses at most the last few moves.

import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState } from "react";
import { providerName } from "../format";
import { Icon } from "../ui/Icon";
import { Mascot } from "../ui/Mascot";
import {
  RateLimiter,
  hostEnvelope,
  newNonce,
  parseActivityMessage,
  type HostMessage,
} from "./protocol";

export interface ActivityInfo {
  id: string;
  name: string;
  description: string;
  category: string;
  version: string;
  url: string;
  storage: boolean;
  savedUtc: string | null;
}

/** A pause the host forced; `seq` changes for every new request. */
export interface PauseRequest {
  seq: number;
  reason: "attention";
  provider: string;
}

type PauseReason = "attention" | "user" | "away" | "unresponsive";

type Phase =
  | { kind: "loading" }
  | { kind: "running" }
  | { kind: "pausing" }
  | { kind: "paused"; reason: PauseReason; provider?: string; saved: boolean }
  | { kind: "failed" };

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
}

export function ActivityHost({ activity, active, pause, onClose, onGoToSessions }: Props) {
  const [phase, setPhaseState] = useState<Phase>({ kind: "loading" });
  // Empty until the saved state is loaded; the iframe is created only after that.
  const [nonce, setNonce] = useState("");
  const frameRef = useRef<HTMLIFrameElement>(null);
  const restoreRef = useRef<unknown>(null);
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
      const current = phaseRef.current.kind;
      if (current === "paused" || current === "failed" || current === "pausing") return;
      let saved = false;
      if (current === "running") {
        setPhase({ kind: "pausing" });
        post({ type: "pause" });
        saved = await requestSave();
      }
      setPhase({ kind: "paused", reason, provider, saved });
    },
    [post, requestSave, setPhase],
  );

  /** Loads the last saved state, then creates a fresh iframe for this launch. */
  const start = useCallback(
    () =>
      saveChain.current
        .then(() => invoke<{ snapshot: string } | null>("activity_snapshot", { id: activity.id }))
        .then((snap) => (snap ? (JSON.parse(snap.snapshot) as unknown) : null))
        .catch(() => null)
        .then((restore) => {
          restoreRef.current = restore;
          missed.current = 0;
          pending.current.clear();
          setNonce(newNonce());
          setPhase({ kind: "loading" });
        }),
    [activity.id, setPhase],
  );

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
        post({
          type: "init",
          restore: restoreRef.current,
          reducedMotion: window.matchMedia("(prefers-reduced-motion: reduce)").matches,
          theme: window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light",
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

  const close = async () => {
    if (phaseRef.current.kind === "running") await freeze("user");
    onClose();
  };

  const live =
    nonce !== "" &&
    (phase.kind === "loading" || phase.kind === "running" || phase.kind === "pausing");

  return (
    <section className="host" aria-label={activity.name}>
      <header className="host-bar">
        <h2 className="host-title">{activity.name}</h2>
        <span className="host-status" role="status">
          {phase.kind === "loading"
            ? "Starting…"
            : phase.kind === "pausing"
              ? "Saving…"
              : phase.kind === "paused"
                ? "Paused"
                : phase.kind === "failed"
                  ? "Didn't start"
                  : ""}
        </span>
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
            src={`${activity.url}#n=${nonce}`}
            sandbox="allow-scripts"
            allow={FRAME_ALLOW}
            referrerPolicy="no-referrer"
          />
        )}
        {phase.kind === "paused" && (
          <PausedCard
            phase={phase}
            name={activity.name}
            onResume={() => void start()}
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
      </div>
    </section>
  );
}

function PausedCard({
  phase,
  name,
  onResume,
  onGoToSessions,
}: {
  phase: Extract<Phase, { kind: "paused" }>;
  name: string;
  onResume: () => void;
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
            : `${name} is paused`}
      </h3>
      <p>
        {attention
          ? `${name} is paused. ${savedText}`
          : phase.reason === "unresponsive"
            ? `Lullshift closed it so it can't slow anything down. ${savedText}`
            : phase.reason === "away"
              ? `Paused while you were away. ${savedText}`
              : savedText}
      </p>
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
      </div>
    </div>
  );
}
