// Several models per provider, picked per message (decision 36). Pure: no React, no invoke.
//
// Default rule: the default model of the default provider. A pick in the composer applies to the
// next message only; after it is sent the composer goes back to the default.

import type { Message, ProviderModel, ProviderProfile } from "./api";

/** One provider/model pair a message can be sent with. */
export interface ModelChoice {
  profileId: string;
  model: string;
}

export const sameChoice = (a: ModelChoice | null, b: ModelChoice | null): boolean =>
  !!a && !!b && a.profileId === b.profileId && a.model === b.model;

/** A provider's models; an older backend without a list still has its one model. */
export function modelsOf(p: ProviderProfile): ProviderModel[] {
  return p.models?.length ? p.models : [{ model: p.model, label: null, isDefault: true }];
}

/** The default: the default provider's default model (else the first provider's). */
export function defaultChoice(profiles: readonly ProviderProfile[]): ModelChoice | null {
  const p = profiles.find((x) => x.isDefault) ?? profiles[0];
  if (!p) return null;
  const m = modelsOf(p).find((x) => x.isDefault)?.model ?? p.model;
  return m ? { profileId: p.id, model: m } : null;
}

/** Whether `choice` is still configured. */
export function isConfigured(
  profiles: readonly ProviderProfile[],
  choice: ModelChoice | null,
): boolean {
  if (!choice) return false;
  const p = profiles.find((x) => x.id === choice.profileId);
  return !!p && modelsOf(p).some((m) => m.model === choice.model);
}

/** The pair the next message goes to: the per-message pick if still configured, else the default. */
export function effectiveChoice(
  profiles: readonly ProviderProfile[],
  picked: ModelChoice | null,
): ModelChoice | null {
  return isConfigured(profiles, picked) ? picked : defaultChoice(profiles);
}

/** A model's display name: its label, else its id. */
export function modelName(
  profiles: readonly ProviderProfile[],
  profileId: string | null,
  model: string,
): string {
  const p = profiles.find((x) => x.id === profileId);
  return (p && modelsOf(p).find((m) => m.model === model)?.label) || model;
}

/** The badge under an answer: which model (and provider) wrote it, or an honest unknown. */
export function answerBadgeText(
  m: Pick<Message, "model" | "profileId">,
  profiles: readonly ProviderProfile[],
): string {
  if (!m.model) return "Model unknown";
  const name = modelName(profiles, m.profileId, m.model);
  const p = profiles.find((x) => x.id === m.profileId);
  return p ? `${name} · ${p.label}` : name;
}

/**
 * The pair a failed message is tried again with: the one it was sent with if it is still set
 * up, else the composer's current pair.
 */
export function retryChoice(
  profiles: readonly ProviderProfile[],
  m: Pick<Message, "model" | "profileId">,
  current: ModelChoice | null,
): ModelChoice | null {
  const own = m.profileId && m.model ? { profileId: m.profileId, model: m.model } : null;
  return isConfigured(profiles, own) ? own : current;
}

/** Same rule as the core's `valid_model_id`, so a name that passes here also saves. */
export function modelIdProblem(id: string): string | null {
  const t = id.trim();
  if (t.length === 0) return "Enter the model name.";
  if (t.length > 200 || !/^[A-Za-z0-9._:/@+-]+$/.test(t)) {
    return "The model name may use letters, digits and . _ : / @ + - only (up to 200 characters).";
  }
  return null;
}

// ---- composer keys ---------------------------------------------------------------------------

export interface KeyLike {
  key: string;
  shiftKey: boolean;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey?: boolean;
  /** `nativeEvent.isComposing`: an input method is composing text. */
  isComposing: boolean;
  keyCode?: number;
}

/**
 * Enter sends, Shift+Enter adds a line, Ctrl/Cmd+Enter also sends. Nothing is sent while an input
 * method is composing (Enter then confirms the composition), including the keyCode 229 some
 * browsers report for it.
 */
export function composerKeyAction(e: KeyLike): "send" | "newline" | "none" {
  if (e.key !== "Enter") return "none";
  if (e.isComposing || e.keyCode === 229) return "none";
  if (e.shiftKey) return "newline";
  if (e.altKey) return "none";
  return "send";
}

export const COMPOSER_HINT = "Enter sends. Shift+Enter starts a new line.";
