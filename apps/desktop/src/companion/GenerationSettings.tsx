// Settings > Companion: "Let my model write activity content" (ADR-028 section 3), the separate
// body-routines switch with its own safety consent, and the list of saved model-written sets.
//
// Off by default. Turning a switch on first shows the exact text that would be sent for one
// example and asks for consent; turning one off needs no question. Saved sets are never deleted
// by turning a switch off.

import { useCallback, useEffect, useId, useState } from "react";
import { AiTag } from "../activity/GeneratedBadge";
import type { ActivityInfo } from "../activity/ActivityHost";
import { Banner } from "../ui/Banner";
import { Dialog } from "../ui/Dialog";
import { Switch } from "../ui/Switch";
import "../styles/generated.css";
import { companionPrefs } from "./api";
import {
  GENERATION_OFF,
  SAMPLE_BODY_REQUEST,
  SAMPLE_REQUEST,
  generatedSetDelete,
  generatedSetsDeleteAll,
  generatedSetsList,
  generationEngine,
  generationErrorText,
  generationPrefsOf,
  generationPreview,
  generationSetSettings,
  toGenerationError,
  type ContentGenerationPrefs,
  type GenerateRequest,
} from "./generate";

export const GENERATION_LABEL = "Let my model write activity content";
export const GENERATION_BODY_LABEL =
  "Also let my model write movement, breathing and pause routines";

type Notice = { tone: "success" | "danger" | "info"; text: string };

/** Which switch's consent dialog is open. */
type Consent = "content" | "body" | null;

const CONTENT_CONSENT = (
  <>
    <p>
      When you press <strong>Fresh set from my model</strong> on a quiz, teaser, calm cards,
      reflection or typing pack, Lullshift sends one request to the provider and model you set up
      for the Companion. It counts toward your monthly budget. Nothing is sent when you open
      Activities, on a timer, or from the tray.
    </p>
    <p>
      The request holds a topic you confirm, the difficulty, how many items, an optional list of
      earlier items to avoid, and Lullshift&apos;s fixed rules. It never holds your sessions, tasks,
      notes, code or file names, and never text from other people&apos;s packs. The set is saved on
      this computer and always labelled <em>AI-generated</em>, because it can be wrong.
    </p>
    <p>The exact text for one example request is below.</p>
  </>
);

const BODY_CONSENT = (
  <>
    <p>
      Movement, breathing and pause routines are instructions you follow with your body. The content
      is AI-generated and can be wrong. Lullshift limits the timings, fixes the breathing patterns
      and refuses risky wording, but it cannot guarantee that a routine suits your body.
    </p>
    <p>
      Stop if anything feels uncomfortable. If you have an injury or a health condition, check with
      a professional first. These routines are never suggested automatically; you start them
      yourself from Activities.
    </p>
    <p>The exact text for one example request is below.</p>
  </>
);

/** The exact example request, read from the core. */
function ExactExample({ request }: { request: GenerateRequest }) {
  const [state, setState] = useState<
    { status: "loading" } | { status: "ready"; text: string } | { status: "error"; text: string }
  >({ status: "loading" });
  useEffect(() => {
    let live = true;
    generationPreview(request)
      .then((text) => {
        if (live) setState({ status: "ready", text });
      })
      .catch((err: unknown) => {
        if (live) setState({ status: "error", text: generationErrorText(toGenerationError(err)) });
      });
    return () => {
      live = false;
    };
  }, [request]);
  if (state.status === "loading") return <p className="field-note">Reading the example…</p>;
  if (state.status === "error") return <Banner tone="danger">{state.text}</Banner>;
  return (
    <pre
      className="exact-text"
      tabIndex={0}
      aria-label="The exact text that would be sent for the example"
      data-testid="consent-text"
    >
      {state.text}
    </pre>
  );
}

function dateText(iso: string): string {
  const t = Date.parse(iso);
  return Number.isNaN(t) ? "unknown date" : new Date(t).toLocaleDateString();
}

export function GenerationSettings({
  hasModel,
  say,
}: {
  /** A Companion provider with a model is set up. */
  hasModel: boolean;
  say: (n: Notice) => void;
}) {
  const id = useId();
  const [prefs, setPrefs] = useState<ContentGenerationPrefs>(GENERATION_OFF);
  const [sets, setSets] = useState<ActivityInfo[] | null>(null);
  const [consent, setConsent] = useState<Consent>(null);
  const [confirmAll, setConfirmAll] = useState(false);
  const [busy, setBusy] = useState(false);

  const loadSets = useCallback(async () => {
    try {
      setSets((await generatedSetsList()) ?? []);
    } catch {
      setSets([]);
    }
  }, []);

  useEffect(() => {
    let live = true;
    companionPrefs()
      .then((p) => {
        if (live) setPrefs(generationPrefsOf(p));
      })
      .catch(() => undefined);
    generatedSetsList()
      .then((s) => {
        if (live) setSets(s ?? []);
      })
      .catch(() => {
        if (live) setSets([]);
      });
    return () => {
      live = false;
    };
  }, []);

  const apply = async (enabled: boolean, bodyEnabled: boolean, ok: string) => {
    setBusy(true);
    try {
      setPrefs(await generationSetSettings(enabled, bodyEnabled));
      say({ tone: "success", text: ok });
    } catch (err) {
      say({ tone: "danger", text: generationErrorText(toGenerationError(err)) });
    } finally {
      setBusy(false);
    }
  };

  const toggleContent = (next: boolean) => {
    if (next) setConsent("content");
    else void apply(false, false, "Model-written content is off. Saved sets are kept.");
  };
  const toggleBody = (next: boolean) => {
    if (next) setConsent("body");
    else void apply(true, false, "Model-written routines are off. Saved sets are kept.");
  };

  const accept = async () => {
    const which = consent;
    setConsent(null);
    if (which === "content") {
      await apply(
        true,
        prefs.bodyEnabled,
        "Your model can now write activity content when you ask.",
      );
    } else if (which === "body") {
      await apply(true, true, "Your model can now write movement, breathing and pause routines.");
    }
  };

  const remove = async (set: ActivityInfo) => {
    try {
      await generatedSetDelete(set.id);
      say({ tone: "success", text: `${set.name} was deleted.` });
    } catch (err) {
      say({ tone: "danger", text: generationErrorText(toGenerationError(err)) });
    }
    await loadSets();
  };

  const removeAll = async () => {
    setConfirmAll(false);
    try {
      const n = await generatedSetsDeleteAll();
      say({
        tone: "success",
        text:
          n === 1 ? "The model-written set was deleted." : `${n} model-written sets were deleted.`,
      });
    } catch (err) {
      say({ tone: "danger", text: generationErrorText(toGenerationError(err)) });
    }
    await loadSets();
  };

  return (
    <section className="generation-setting" aria-labelledby={`${id}-h`}>
      <h3 className="section-title" id={`${id}-h`}>
        Activity content from your model
      </h3>
      <Switch
        checked={prefs.enabled}
        disabled={busy || (!hasModel && !prefs.enabled)}
        onChange={toggleContent}
        label={GENERATION_LABEL}
        description={
          hasModel || prefs.enabled
            ? "Off by default. When on, quizzes, brain teasers, calm cards, reflections and typing packs get a Fresh set from my model button. Each press sends one request to your provider."
            : "Set up a provider and a model above first. Until then nothing can be written."
        }
      />
      <Switch
        checked={prefs.bodyEnabled}
        disabled={busy || !prefs.enabled}
        onChange={toggleBody}
        label={GENERATION_BODY_LABEL}
        description={
          prefs.enabled
            ? "Off by default, and separate because these are instructions for your body. Gentle desk routines only, with timings Lullshift controls."
            : "Turn on the switch above first."
        }
      />

      <h4 className="section-title" id={`${id}-sets`}>
        Model-written sets
      </h4>
      {sets === null && <p className="card-sub">Loading saved sets…</p>}
      {sets !== null && sets.length === 0 && (
        <p className="card-sub">No model-written sets are saved.</p>
      )}
      {sets !== null && sets.length > 0 && (
        <>
          <ul className="generated-list" aria-labelledby={`${id}-sets`}>
            {sets.map((s) => (
              <li key={s.id} className="generated-row">
                <div className="generated-row-text">
                  <span className="generated-row-name">{s.name}</span>
                  <AiTag info={s} />
                  <span className="generated-row-meta">
                    {s.generated?.topic ? `Topic: ${s.generated.topic} · ` : ""}
                    {generationEngine(s.generated?.engine)?.label ?? "Activity"} ·{" "}
                    {s.generated ? dateText(s.generated.createdUtc) : ""}
                    {s.generated?.unusable ? " · Can't be used any more" : ""}
                  </span>
                </div>
                <button
                  type="button"
                  className="btn btn-danger"
                  aria-label={`Delete ${s.name}`}
                  onClick={() => void remove(s)}
                >
                  Delete
                </button>
              </li>
            ))}
          </ul>
          <div className="actions">
            <button type="button" className="btn btn-danger" onClick={() => setConfirmAll(true)}>
              Delete all model-written sets
            </button>
          </div>
        </>
      )}

      <Dialog
        open={consent !== null}
        title={consent === "body" ? GENERATION_BODY_LABEL : GENERATION_LABEL}
        onClose={() => setConsent(null)}
        description={consent === "body" ? BODY_CONSENT : CONTENT_CONSENT}
        actions={
          <>
            <button
              type="button"
              className="btn btn-quiet"
              autoFocus
              onClick={() => setConsent(null)}
            >
              Not now
            </button>
            <button type="button" className="btn btn-primary" onClick={() => void accept()}>
              Turn on
            </button>
          </>
        }
      >
        {consent !== null && (
          <ExactExample request={consent === "body" ? SAMPLE_BODY_REQUEST : SAMPLE_REQUEST} />
        )}
      </Dialog>

      <Dialog
        open={confirmAll}
        title="Delete all model-written sets?"
        onClose={() => setConfirmAll(false)}
        description={
          <p>
            Every set your model wrote is deleted from this computer, with its saved progress and
            play time. This can&apos;t be undone. Built-in and installed packs are not touched.
          </p>
        }
        actions={
          <>
            <button
              type="button"
              className="btn btn-quiet"
              autoFocus
              onClick={() => setConfirmAll(false)}
            >
              Keep them
            </button>
            <button type="button" className="btn btn-danger" onClick={() => void removeAll()}>
              Delete all
            </button>
          </>
        }
      />
    </section>
  );
}
