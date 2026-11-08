// "Fresh set from my model" (ADR-028 sections 3 to 5): the dialog that asks for the topic and the
// choices, shows exactly what would be sent, writes one set on the button press, and ends in a
// start button or a calm failure card. Nothing is sent before "Write the set".

import { useEffect, useId, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { companionProfiles, newRequestId, type ProviderProfile } from "../companion/api";
import { ModelChoiceDialog, choiceText } from "../companion/Composer";
import {
  BREATHING_PATTERNS,
  BREATHING_ROUNDS,
  DIFFICULTIES,
  PHASE_TEXT,
  TOPIC_MAX,
  generateSet,
  generationCancel,
  generationEngine,
  generationErrorText,
  generationPreview,
  toGenerationError,
  type BreathingPatternId,
  type Difficulty,
  type GenerateRequest,
  type GenerationEngine,
  type GenerationError,
  type GenerationPhase,
} from "../companion/generate";
import { sendsToText } from "../companion/logic";
import {
  defaultChoice,
  effectiveChoice,
  sameChoice,
  type ModelChoice,
} from "../companion/modelChoice";
import { ProviderSaid } from "../companion/ProviderSaid";
import { Banner } from "../ui/Banner";
import { Dialog } from "../ui/Dialog";
import type { ActivityInfo } from "./ActivityHost";
import { AiNotice } from "./GeneratedBadge";

type Stage =
  | { kind: "form"; note: string | null }
  | { kind: "running"; requestId: string; phase: GenerationPhase }
  | { kind: "failed"; error: GenerationError }
  | { kind: "done"; info: ActivityInfo };

type Exact =
  | { status: "closed" }
  | { status: "loading" }
  | { status: "ready"; text: string }
  | { status: "error"; text: string };

interface Props {
  open: boolean;
  /** The engine reference to write for (`lullshift.quiz@1`). */
  engineRef: string;
  /** The pack the dialog was opened from; null when opened from an engine's group. */
  pack: ActivityInfo | null;
  onClose: () => void;
  /** The set was written and saved: re-read the lists. */
  onWritten: () => void;
  /** Start the new set. */
  onStart: (set: ActivityInfo) => void;
  /** Start the pack the dialog was opened from (the failure card's fallback). */
  onPlayPack: (pack: ActivityInfo) => void;
}

/** The topic the field starts with: a built-in pack's own title, never an installed pack's. */
export function initialTopic(pack: ActivityInfo | null): string {
  return pack?.source?.builtin === true ? Array.from(pack.name).slice(0, TOPIC_MAX).join("") : "";
}

export function FreshSetDialog(props: Props) {
  const engine = generationEngine(props.engineRef);
  return engine ? <FreshSetDialogFor {...props} engine={engine} /> : null;
}

function FreshSetDialogFor({
  open,
  engine,
  pack,
  onClose,
  onWritten,
  onStart,
  onPlayPack,
}: Props & { engine: GenerationEngine }) {
  const ids = useId();
  const [stage, setStage] = useState<Stage>({ kind: "form", note: null });
  const [topic, setTopic] = useState("");
  const [difficulty, setDifficulty] = useState<Difficulty>("medium");
  const [size, setSize] = useState(0);
  const [pattern, setPattern] = useState<BreathingPatternId>("even");
  const [rounds, setRounds] = useState(6);
  const [avoid, setAvoid] = useState(true);
  const [picked, setPicked] = useState<ModelChoice | null>(null);
  const [profiles, setProfiles] = useState<ProviderProfile[] | null>(null);
  const [choosing, setChoosing] = useState(false);
  const [exact, setExact] = useState<Exact>({ status: "closed" });
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  // Each time the dialog opens it starts from the pack's own defaults (adjusting state while
  // rendering, as ActivitiesView does).
  const [wasOpen, setWasOpen] = useState(false);
  if (open !== wasOpen) {
    setWasOpen(open);
    if (open) {
      setStage({ kind: "form", note: null });
      setTopic(initialTopic(pack));
      setDifficulty("medium");
      setSize(engine.sizes[1] ?? engine.sizes[0] ?? 0);
      setPattern("even");
      setRounds(6);
      setAvoid(true);
      setPicked(null);
      setChoosing(false);
      setExact({ status: "closed" });
    }
  }

  useEffect(() => {
    if (!open) return;
    let live = true;
    companionProfiles()
      .then((p) => {
        if (live) setProfiles(p);
      })
      .catch(() => {
        if (live) setProfiles([]);
      });
    return () => {
      live = false;
    };
  }, [open]);

  const choice = profiles ? effectiveChoice(profiles, picked) : null;
  const profile = profiles?.find((p) => p.id === choice?.profileId) ?? null;
  const cleanTopic = topic.replace(/\s+/g, " ").trim();
  const topicLength = Array.from(cleanTopic).length;
  const topicOk = topicLength >= 1 && topicLength <= TOPIC_MAX;
  const isBreathing = engine.short === "breathing";

  const request: GenerateRequest = {
    engine: engine.short,
    topic: cleanTopic,
    difficulty: engine.body ? "gentle" : difficulty,
    size: engine.sizes.includes(size) ? size : (engine.sizes[0] ?? 1),
    breathing: isBreathing ? { pattern, rounds } : null,
    avoid: isBreathing ? false : avoid,
    originPack: pack?.id ?? null,
    profileId: choice?.profileId ?? null,
    model: choice?.model ?? null,
  };
  const requestKey = JSON.stringify(request);

  // "Show exact text" reads the real request body from the core, and again when a choice changes.
  // The effect depends on the key, not the object, so it runs once per distinct request.
  const exactOpen = exact.status !== "closed";
  useEffect(() => {
    if (!exactOpen || !open) return;
    let live = true;
    const req = JSON.parse(requestKey) as GenerateRequest;
    generationPreview(req)
      .then((text) => {
        if (live) setExact({ status: "ready", text });
      })
      .catch((err: unknown) => {
        if (live) setExact({ status: "error", text: generationErrorText(toGenerationError(err)) });
      });
    return () => {
      live = false;
    };
  }, [requestKey, exactOpen, open]);

  const showExact = () => {
    if (exact.status === "closed") setExact({ status: "loading" });
    else setExact({ status: "closed" });
  };

  const write = async () => {
    const requestId = newRequestId();
    setStage({ kind: "running", requestId, phase: "waiting" });
    try {
      const info = await generateSet(requestId, request, (e) => {
        if (e.type === "phase" && e.requestId === requestId && mounted.current) {
          setStage((s) =>
            s.kind === "running" && s.requestId === requestId ? { ...s, phase: e.phase } : s,
          );
        }
      });
      if (!mounted.current) return;
      setStage({ kind: "done", info });
      onWritten();
    } catch (err) {
      if (!mounted.current) return;
      const error = toGenerationError(err);
      setStage(
        error.kind === "cancelled"
          ? { kind: "form", note: generationErrorText(error) }
          : { kind: "failed", error },
      );
    }
  };

  const cancel = () => {
    if (stage.kind === "running") void generationCancel(stage.requestId).catch(() => undefined);
  };

  const host = profile?.host ?? null;
  const noProvider = profiles !== null && profiles.length === 0;
  const canWrite = topicOk && !!choice && !noProvider;
  const title =
    stage.kind === "done"
      ? "Your set is ready"
      : stage.kind === "failed"
        ? "The set wasn't started"
        : `Fresh set from my model: ${engine.label.toLowerCase()}`;

  const actions =
    stage.kind === "form" ? (
      <>
        <button type="button" className="btn btn-quiet" onClick={onClose}>
          Close
        </button>
        <button
          type="button"
          className="btn btn-primary"
          disabled={!canWrite}
          onClick={() => void write()}
        >
          Write the set
        </button>
      </>
    ) : stage.kind === "running" ? (
      <button type="button" className="btn btn-secondary" autoFocus onClick={cancel}>
        Cancel
      </button>
    ) : stage.kind === "failed" ? (
      <>
        <button type="button" className="btn btn-quiet" onClick={onClose}>
          Close
        </button>
        <button
          type="button"
          className="btn btn-secondary"
          onClick={() => setStage({ kind: "form", note: null })}
        >
          Change and try again
        </button>
        {pack && (
          <button
            type="button"
            className="btn btn-primary"
            autoFocus
            onClick={() => {
              onClose();
              onPlayPack(pack);
            }}
          >
            Play the built-in set
          </button>
        )}
      </>
    ) : (
      <>
        <button type="button" className="btn btn-quiet" onClick={onClose}>
          Not now
        </button>
        <button
          type="button"
          className="btn btn-primary"
          autoFocus
          onClick={() => {
            onClose();
            onStart(stage.info);
          }}
        >
          Start it
        </button>
      </>
    );

  return (
    <>
      <Dialog
        open={open}
        title={title}
        onClose={onClose}
        busy={stage.kind === "running"}
        description={
          stage.kind === "form" ? (
            <p>
              Your own model writes a new set when you press <strong>Write the set</strong>. Nothing
              is sent before that.
            </p>
          ) : stage.kind === "done" ? (
            <p>It was saved on this computer, so you can play it again offline.</p>
          ) : undefined
        }
        actions={actions}
      >
        {stage.kind === "form" && (
          <div className="fresh-form">
            {stage.note && (
              <Banner tone="info">
                <span role="status">{stage.note}</span>
              </Banner>
            )}
            {noProvider && (
              <Banner tone="attention">
                No provider is set up. Add one in Settings &gt; Companion first.
              </Banner>
            )}
            <div className="fresh-field">
              <label htmlFor={`${ids}-topic`}>Topic</label>
              <input
                id={`${ids}-topic`}
                type="text"
                value={topic}
                maxLength={TOPIC_MAX + 40}
                autoComplete="off"
                aria-describedby={`${ids}-topic-note`}
                aria-invalid={topic !== "" && !topicOk ? true : undefined}
                onChange={(e) => setTopic(e.target.value)}
              />
              <span className="field-note" id={`${ids}-topic-note`}>
                {pack?.source?.builtin === true
                  ? "Starts with this pack's title. Change it to anything you like. "
                  : "What the set should be about. "}
                1 to {TOPIC_MAX} characters.
              </span>
            </div>
            {!engine.body && (
              <div className="fresh-field">
                <label htmlFor={`${ids}-difficulty`}>Difficulty</label>
                <select
                  id={`${ids}-difficulty`}
                  value={difficulty}
                  onChange={(e) => setDifficulty(e.target.value as Difficulty)}
                >
                  {DIFFICULTIES.map((d) => (
                    <option key={d.id} value={d.id}>
                      {d.label}
                    </option>
                  ))}
                </select>
              </div>
            )}
            {engine.body && (
              <p className="field-note">
                Gentle, desk-friendly routines only. Lullshift sets the timings and refuses risky
                wording.
              </p>
            )}
            {!isBreathing && (
              <div className="fresh-field">
                <label htmlFor={`${ids}-size`}>How many {engine.noun}</label>
                <select
                  id={`${ids}-size`}
                  value={String(request.size)}
                  onChange={(e) => setSize(Number(e.target.value))}
                >
                  {engine.sizes.map((n) => (
                    <option key={n} value={String(n)}>
                      {n} {engine.noun}
                    </option>
                  ))}
                </select>
              </div>
            )}
            {isBreathing && (
              <>
                <div className="fresh-field">
                  <label htmlFor={`${ids}-pattern`}>Breathing pattern</label>
                  <select
                    id={`${ids}-pattern`}
                    value={pattern}
                    onChange={(e) => setPattern(e.target.value as BreathingPatternId)}
                  >
                    {BREATHING_PATTERNS.map((p) => (
                      <option key={p.id} value={p.id}>
                        {p.label}
                      </option>
                    ))}
                  </select>
                  <span className="field-note">
                    The pattern is fixed by Lullshift. Your model writes only the wording.
                  </span>
                </div>
                <div className="fresh-field">
                  <label htmlFor={`${ids}-rounds`}>Rounds</label>
                  <select
                    id={`${ids}-rounds`}
                    value={String(rounds)}
                    onChange={(e) => setRounds(Number(e.target.value))}
                  >
                    {BREATHING_ROUNDS.map((n) => (
                      <option key={n} value={String(n)}>
                        {n} rounds
                      </option>
                    ))}
                  </select>
                </div>
              </>
            )}
            <div className="fresh-field">
              <span id={`${ids}-model-label`}>Model</span>
              <div className="actions">
                <span>
                  {profiles === null ? "Reading your models…" : choiceText(profiles, choice)}
                  {profiles && choice && !sameChoice(choice, defaultChoice(profiles)) && (
                    <span className="composer-once"> (this set only)</span>
                  )}
                </span>
                <button
                  type="button"
                  className="btn btn-quiet"
                  disabled={!profiles || profiles.length === 0}
                  onClick={() => setChoosing(true)}
                >
                  Change model
                </button>
              </div>
            </div>
            {!isBreathing && (
              <label className="check-row">
                <input
                  type="checkbox"
                  checked={avoid}
                  onChange={(e) => setAvoid(e.target.checked)}
                />{" "}
                Don&apos;t repeat earlier {engine.noun}
                <span className="field-note">
                  Sends up to 20 earlier {engine.noun} from your saved sets on this topic
                  {pack?.source?.builtin === true ? " and from this built-in pack" : ""}, so the new
                  set is different. These are text your model or Lullshift wrote, never yours.
                </span>
              </label>
            )}
            <p className="fresh-summary">
              {host ? `${sendsToText(host)}: ` : "Sent to your provider: "}
              the topic, the difficulty, how many, the list above if it is on, and Lullshift&apos;s
              fixed rules for this activity. Never your sessions, tasks, notes, code or file names,
              and never text from other packs.
            </p>
            <div>
              <button
                type="button"
                className="btn btn-quiet"
                aria-expanded={exact.status !== "closed"}
                aria-controls={`${ids}-exact`}
                onClick={showExact}
              >
                {exact.status === "closed" ? "Show exact text" : "Hide exact text"}
              </button>
              <div id={`${ids}-exact`}>
                {exact.status === "loading" && <p className="field-note">Reading the request…</p>}
                {exact.status === "error" && <Banner tone="danger">{exact.text}</Banner>}
                {exact.status === "ready" && (
                  <pre
                    className="exact-text"
                    tabIndex={0}
                    aria-label="The exact text that would be sent"
                    data-testid="exact-text"
                  >
                    {exact.text}
                  </pre>
                )}
              </div>
            </div>
          </div>
        )}
        {stage.kind === "running" && (
          <div className="fresh-progress">
            <p role="status" aria-live="polite" data-testid="phase">
              {PHASE_TEXT[stage.phase]}
            </p>
            <p className="field-note">You can cancel any time. Nothing is saved unless it fits.</p>
          </div>
        )}
        {stage.kind === "failed" && (
          <div role="alert" className="fresh-progress" data-testid="fresh-failed">
            <p>{generationErrorText(stage.error, host)}</p>
            <ProviderSaid reason={stage.error.providerReason} />
            {pack && <p className="field-note">You can play the built-in set instead.</p>}
          </div>
        )}
        {stage.kind === "done" && (
          <div className="fresh-progress">
            <p>
              <strong>{stage.info.name}</strong>
            </p>
            <AiNotice info={stage.info} />
          </div>
        )}
      </Dialog>
      {choosing &&
        profiles &&
        createPortal(
          <ModelChoiceDialog
            open
            forSet
            profiles={profiles}
            current={choice}
            onPick={(c) => setPicked(c)}
            onClose={() => setChoosing(false)}
          />,
          document.body,
        )}
    </>
  );
}
