// Model-written activity sets (Sprint 13, ADR-028): the commands, the engine facts the dialog
// needs, and the host's own wording. Pure apart from `invoke`.
//
// Privacy rules for this file: the request holds only what the dialog collected (topic,
// difficulty, size, breathing choice, the avoid switch, the pack's id and the provider choice).
// A pack's title, description or tags are never put in it; the dialog may suggest the title as
// the topic only for a built-in pack, and the user sees and can change it. Errors are reduced to
// `{ kind, message, providerReason }`; nothing the caller passed in is copied into an error.

import { Channel, invoke } from "@tauri-apps/api/core";
import type { ActivityInfo } from "../activity/ActivityHost";
import { toProviderReason, type ProviderReason } from "./api";
import { errorText } from "./logic";

// ---- engines -------------------------------------------------------------------------------

export type EngineShort =
  "quiz" | "teaser" | "cards" | "reflection" | "typing" | "movement" | "breathing" | "pause";

export interface GenerationEngine {
  short: EngineShort;
  /** Movement, breathing and pause: behind the separate switch (ADR-028 section 2a). */
  body: boolean;
  /** The item counts a person may pick. */
  sizes: readonly number[];
  /** What one item is called, for the size choice ("8 questions"). */
  noun: string;
  /** Plural name of the sets, for headings ("Quiz sets"). */
  label: string;
  /** "This looks wrong" is offered for these (a set has items the host can leave out). */
  markable: boolean;
}

/** The eight engines that can have model-written sets, by engine reference. */
export const GENERATION_ENGINES: Readonly<Record<string, GenerationEngine>> = {
  "lullshift.quiz@1": {
    short: "quiz",
    body: false,
    sizes: [5, 8, 12],
    noun: "questions",
    label: "Quiz sets",
    markable: true,
  },
  "lullshift.teaser@1": {
    short: "teaser",
    body: false,
    sizes: [3, 5, 8],
    noun: "teasers",
    label: "Brain teaser sets",
    markable: true,
  },
  "lullshift.cards@1": {
    short: "cards",
    body: false,
    sizes: [5, 8, 12],
    noun: "cards",
    label: "Calm card sets",
    markable: false,
  },
  "lullshift.reflection@1": {
    short: "reflection",
    body: false,
    sizes: [3, 5, 8],
    noun: "prompts",
    label: "Reflection sets",
    markable: false,
  },
  "lullshift.typing@1": {
    short: "typing",
    body: false,
    sizes: [5, 8, 12],
    noun: "lines",
    label: "Typing sets",
    markable: false,
  },
  "lullshift.movement@1": {
    short: "movement",
    body: true,
    sizes: [3, 5, 8],
    noun: "moves",
    label: "Movement routines",
    markable: false,
  },
  "lullshift.breathing@1": {
    short: "breathing",
    body: true,
    sizes: [1],
    noun: "routine",
    label: "Breathing routines",
    markable: false,
  },
  "lullshift.pause@1": {
    short: "pause",
    body: true,
    sizes: [3, 5, 8],
    noun: "steps",
    label: "Pause routines",
    markable: false,
  },
};

/** The engine facts for an engine reference, or null when it can't have model-written sets. */
export function generationEngine(engineRef: string | null | undefined): GenerationEngine | null {
  return (engineRef && GENERATION_ENGINES[engineRef]) || null;
}

/** The order the groups appear in. */
export const GENERATION_ENGINE_ORDER: readonly string[] = Object.keys(GENERATION_ENGINES);

export type Difficulty = "gentle" | "medium" | "tricky";
export const DIFFICULTIES: readonly { id: Difficulty; label: string }[] = [
  { id: "gentle", label: "Gentle" },
  { id: "medium", label: "Medium" },
  { id: "tricky", label: "Tricky" },
];

export type BreathingPatternId = "even" | "longerOut" | "boxGentle";
export const BREATHING_PATTERNS: readonly { id: BreathingPatternId; label: string }[] = [
  { id: "even", label: "Even (breathe in 4, out 4)" },
  { id: "longerOut", label: "Longer out (in 4, out 6)" },
  { id: "boxGentle", label: "Box, gentle (in 4, hold 2, out 4, hold 2)" },
];
export const BREATHING_ROUNDS: readonly number[] = [4, 6, 8];

export const TOPIC_MAX = 80;

// ---- the switches --------------------------------------------------------------------------

export interface ContentGenerationPrefs {
  enabled: boolean;
  bodyEnabled: boolean;
}

export const GENERATION_OFF: ContentGenerationPrefs = { enabled: false, bodyEnabled: false };

/** The switches from a `companion_prefs` reply; an older core without them reads as off. */
export function generationPrefsOf(prefs: unknown): ContentGenerationPrefs {
  const g = (prefs as { contentGeneration?: Partial<ContentGenerationPrefs> } | null)
    ?.contentGeneration;
  const enabled = g?.enabled === true;
  return { enabled, bodyEnabled: enabled && g?.bodyEnabled === true };
}

/** Whether the Fresh set action is offered for an engine under the current switches. */
export function canGenerate(engineRef: string | null | undefined, g: ContentGenerationPrefs) {
  const e = generationEngine(engineRef);
  return !!e && g.enabled && (!e.body || g.bodyEnabled);
}

// ---- the request ---------------------------------------------------------------------------

export interface GenerateRequest {
  engine: EngineShort;
  topic: string;
  difficulty: Difficulty;
  size: number;
  breathing: { pattern: BreathingPatternId; rounds: number } | null;
  avoid: boolean;
  originPack: string | null;
  profileId: string | null;
  model: string | null;
}

export type GenerationPhase = "waiting" | "checking" | "retrying";

export type GenerationEvent =
  | { type: "phase"; requestId: string; phase: GenerationPhase }
  | { type: "done"; requestId: string; setId: string }
  | { type: "failed"; requestId: string; kind: string };

/** One polite sentence per phase. No token counts, no countdowns. */
export const PHASE_TEXT: Record<GenerationPhase, string> = {
  waiting: "Your model is writing the set. This can take up to a minute.",
  checking: "Checking that the set fits the activity.",
  retrying: "The first try didn't fit, so Lullshift is asking once more.",
};

/** The sample request both consent dialogs show: a quiz on a fixed topic. */
export const SAMPLE_REQUEST: GenerateRequest = {
  engine: "quiz",
  topic: "Sea creatures",
  difficulty: "medium",
  size: 5,
  breathing: null,
  avoid: false,
  originPack: null,
  profileId: null,
  model: null,
};

/** The sample the body-routines consent dialog shows: a movement routine on a fixed topic. */
export const SAMPLE_BODY_REQUEST: GenerateRequest = {
  ...SAMPLE_REQUEST,
  engine: "movement",
  topic: "Desk stretches",
  difficulty: "gentle",
};

// ---- errors --------------------------------------------------------------------------------

export interface GenerationError {
  kind: string;
  message: string;
  providerReason?: ProviderReason | null;
}

/** Whatever `invoke` rejected with, reduced to a kind and the backend's fixed sentence. */
export function toGenerationError(err: unknown): GenerationError {
  if (err && typeof err === "object" && "kind" in err) {
    const e = err as { kind?: unknown; message?: unknown; providerReason?: unknown };
    if (typeof e.kind === "string" && /^[A-Za-z]{1,32}$/.test(e.kind)) {
      const providerReason = toProviderReason(e.providerReason);
      return {
        kind: e.kind,
        message: typeof e.message === "string" ? e.message.slice(0, 300) : "",
        ...(providerReason ? { providerReason } : {}),
      };
    }
  }
  const text = typeof err === "string" ? err : err instanceof Error ? err.message : "";
  if (/not found|not allowed|unknown command|not registered/i.test(text)) {
    return { kind: "unavailable", message: "" };
  }
  return { kind: "internal", message: "" };
}

export const DID_NOT_FIT_TEXT =
  "Your model's set didn't fit this activity, so Lullshift didn't start it. Nothing was lost.";

const COMPANION_KIND: Record<string, string> = {
  offline: "offline",
  timeout: "timeout",
  rateLimited: "rateLimited",
  keyRejected: "keyRejected",
  noKey: "noKey",
  keychainUnavailable: "keychainUnavailable",
  budgetReached: "budgetReached",
  budgetRefused: "budgetReached",
  provider: "providerError",
  providerError: "providerError",
  invalidResponse: "invalidResponse",
  invalidRequest: "invalidInput",
  invalidInput: "invalidInput",
  notFound: "notFound",
  cancelled: "cancelled",
  busy: "busy",
  unavailable: "unavailable",
};

/**
 * The calm sentence for a failed generation: honest about offline, no key, timeout, provider and
 * budget (the same wording Companion uses), and the fixed "didn't fit" card text. The provider's
 * own words are shown apart, by the caller, through `ProviderSaid`.
 */
export function generationErrorText(err: GenerationError, host?: string | null): string {
  switch (err.kind) {
    case "didNotFit":
      return DID_NOT_FIT_TEXT;
    case "generationOff":
      return err.message || "Model-written content is turned off in Settings > Companion.";
    case "generatedUnusable":
      return "This set from your model can't be used any more. You can delete it in Settings.";
    case "cancelled":
      return "Cancelled. The request may already have reached the provider, so it may still count against your usage.";
    default:
      return errorText(
        { kind: COMPANION_KIND[err.kind] ?? "internal", message: err.message },
        host,
      );
  }
}

// ---- the label (ADR-028 section 8) -----------------------------------------------------------

/** The longest model name shown. The core cuts it too; this keeps a stray long one in bounds. */
const MODEL_SHOWN = 40;

/**
 * "AI-generated by <model>. It can be wrong." ("your model" when the name is unknown). Body sets
 * add "Stop if anything feels uncomfortable." Plain text only: callers render it as text.
 */
export function aiLabel(info: ActivityInfo): string | null {
  const g = info.generated;
  if (!g) return null;
  const model = (g.model ?? "").replace(/\s+/g, " ").trim().slice(0, MODEL_SHOWN);
  const body = generationEngine(g.engine)?.body === true;
  const text = `AI-generated by ${model || "your model"}. It can be wrong.`;
  return body ? `${text} Stop if anything feels uncomfortable.` : text;
}

/** The short tag on cards and lists: "AI-generated · <model>". */
export function aiTag(info: ActivityInfo): string | null {
  const g = info.generated;
  if (!g) return null;
  const model = (g.model ?? "").replace(/\s+/g, " ").trim().slice(0, MODEL_SHOWN);
  return model ? `AI-generated · ${model}` : "AI-generated";
}

/** The accessible name of a set's frame and section: starts with "AI-generated". */
export function aiAccessibleName(info: ActivityInfo): string {
  return info.generated ? `AI-generated. ${info.name}` : info.name;
}

// ---- commands ------------------------------------------------------------------------------

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (err) {
    throw toGenerationError(err);
  }
}

export const generationSetSettings = (enabled: boolean, bodyEnabled: boolean) =>
  call<ContentGenerationPrefs>("generation_set_settings", { enabled, bodyEnabled });

/** The exact first request body for the chosen provider and model. Nothing is sent. */
export const generationPreview = (req: GenerateRequest) =>
  call<string>("generation_preview", { req });

/** Writes one set. `onEvent` hears the phases; the promise ends with the new set's info. */
export function generateSet(
  requestId: string,
  req: GenerateRequest,
  onEvent: (e: GenerationEvent) => void,
): Promise<ActivityInfo> {
  const ch = new Channel<GenerationEvent>();
  ch.onmessage = onEvent;
  return call<ActivityInfo>("generate_set", { requestId, req, onEvent: ch });
}

export const generationCancel = (requestId: string) =>
  call<void>("generation_cancel", { requestId });

export const generatedSetsList = (engine?: string) =>
  call<ActivityInfo[]>("generated_sets_list", { engine: engine ?? null });

export const generatedSetDelete = (id: string) => call<boolean>("generated_set_delete", { id });

export const generatedSetsDeleteAll = () => call<number>("generated_sets_delete_all");

export const generatedSetItems = (id: string) => call<string[]>("generated_set_items", { id });

export const generatedSetSkip = (id: string, itemIndex: number) =>
  call<void>("generated_set_skip", { id, itemIndex });
