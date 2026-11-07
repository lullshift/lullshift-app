// First-launch welcome: what Lullshift is, the optional agent hookup and a first activity.
// Wording here describes what really happens (crates/agent-adapters, src-tauri/src/adapters.rs):
// Claude Code's user settings file is edited only after two confirmations, a backup of the
// original is saved first, and Disconnect removes the hooks again.

export const WELCOME_TITLE = "Welcome to Lullshift";

export const WELCOME_LINES = [
  "Lullshift watches your coding agents and offers a short break while they work, then tells you when one needs you.",
  "There is no account to make. Everything stays on this computer, and it works offline.",
] as const;

/** Button label. It opens Settings > Integrations, which lists every supported agent. */
export const CONNECT_LABEL = "Connect an agent";

export const CONNECT_EXPLAINER =
  "Connecting is optional. Integrations, in Settings, lists every supported agent: Claude Code, Codex and Cursor (OpenCode 2.x is there too, as an experiment). For the one you pick, Lullshift adds a small hook to that agent's hooks or settings file and saves a backup of the original first. Nothing is changed until you confirm on the next screen and once more in a system dialog. Disconnect removes it again.";

/** The smallest slice of an activity the suggestion needs. */
export interface Suggestible {
  id: string;
  name: string;
  category: string;
  savedUtc?: string | null;
}

/**
 * A calm first pick: a break activity if there is one, else the first in the list. Something
 * already started is not a "first" activity, so it is skipped when anything else is available.
 */
export function suggestFirstActivity<T extends Suggestible>(list: T[]): T | null {
  const fresh = list.filter((a) => !a.savedUtc);
  const pool = fresh.length > 0 ? fresh : list;
  return pool.find((a) => a.category === "break") ?? pool[0] ?? null;
}
