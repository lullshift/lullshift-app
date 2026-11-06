// A content pack's data, fetched for `init.content` before the pack starts (ADR-016).
//
// The check itself runs in Rust (`activity_content`): it resolves the pack's engine (`id@major`)
// among the bundled engines and validates the data against the engine's data schema. It can't
// run here: Ajv compiles schemas with `new Function`, which the app's CSP forbids, and that
// CSP must not be loosened. If the check fails, the host shows a calm card and never creates
// the engine's iframe (the scheme handler also refuses to serve it).

import { invoke } from "@tauri-apps/api/core";

/** What Rust reports when a pack can't start. */
interface ContentError {
  code: string;
  detail: string;
}

export interface ContentProblem {
  /** One calm sentence for the user. */
  message: string;
  /** What exactly is wrong, for the curious or the pack's creator; may be empty. */
  detail: string;
}

export type ContentResult = { ok: true; content: unknown } | { ok: false; problem: ContentProblem };

type Invoke = <T>(cmd: string, args: Record<string, unknown>) => Promise<T>;

const isContentError = (e: unknown): e is ContentError =>
  typeof e === "object" &&
  e !== null &&
  typeof (e as ContentError).code === "string" &&
  typeof (e as ContentError).detail === "string";

/** The user-facing words for a failed check. Never shows raw error objects. */
export function describeContentError(error: unknown, engine: string | null): ContentProblem {
  if (!isContentError(error)) {
    return {
      message: "This pack's data couldn't be checked, so Lullshift didn't start it.",
      detail: "",
    };
  }
  // Rust builds the detail from bounded plain text; keep it short anyway.
  const detail = error.detail.slice(0, 600);
  switch (error.code) {
    case "data_invalid":
      return {
        message: "This pack's data doesn't match its engine, so Lullshift didn't start it.",
        detail,
      };
    case "engine_missing":
      return {
        message: `This pack needs ${engine ? `the ${engine} engine` : "an engine"}, which this version of Lullshift doesn't have.`,
        detail,
      };
    case "engine_unusable":
      return { message: "This pack's engine couldn't be loaded.", detail };
    case "image_invalid":
      return {
        message: "This pack has a picture that didn't pass Lullshift's checks, so it didn't start.",
        detail,
      };
    case "data_missing":
    case "data_not_json":
    case "data_too_large":
      return { message: "This pack's data couldn't be read.", detail };
    default:
      return {
        message: "This pack's data couldn't be checked, so Lullshift didn't start it.",
        detail: "",
      };
  }
}

/**
 * The data to hand the activity as `init.content`: null for an app; for a pack, its checked
 * data, or the problem to show instead of starting it.
 */
export async function loadContent(
  activity: { id: string; engine: string | null },
  call: Invoke = invoke,
): Promise<ContentResult> {
  if (!activity.engine) return { ok: true, content: null };
  try {
    const content = await call<unknown>("activity_content", { id: activity.id });
    // A pack always has data; null here would start the engine empty.
    if (content === null || typeof content !== "object") {
      return { ok: false, problem: describeContentError(null, activity.engine) };
    }
    return { ok: true, content };
  } catch (error) {
    return { ok: false, problem: describeContentError(error, activity.engine) };
  }
}
