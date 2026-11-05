import { useCallback, useEffect, useRef, useState } from "react";
import { sessionDetail, type SessionDetail, type SessionKey } from "./api";

export type DetailLoad =
  | { status: "loading" }
  | { status: "ready"; detail: SessionDetail }
  | { status: "error"; code: string; last: SessionDetail | null };

/** Short user-facing text for an error code from the context commands (never task text). */
export function contextErrorText(err: unknown, fallback: string): string {
  switch (String(err)) {
    case "not_found":
      return "This session isn't in Lullshift's local data any more. Nothing was changed.";
    case "store_unavailable":
      return "Local data is unavailable right now. Nothing was changed.";
    case "too_many":
      return "There are too many items here. Delete some, then try again.";
    default:
      return fallback;
  }
}

/**
 * Loads one session's detail from the Rust core and reloads it when `tick` changes (the
 * sessions-changed event) or `reload()` is called after an edit. Rust stays authoritative: this
 * only holds the last snapshot. A failed reload keeps showing the last good one.
 */
export function useSessionDetail(session: SessionKey, tick: number) {
  const [load, setLoad] = useState<DetailLoad>({ status: "loading" });
  const generation = useRef(0);
  const { provider, sessionId } = session;

  const reload = useCallback(async () => {
    const mine = ++generation.current;
    try {
      const detail = await sessionDetail({ provider, sessionId });
      if (mine === generation.current) setLoad({ status: "ready", detail });
    } catch (err: unknown) {
      if (mine !== generation.current) return;
      setLoad((prev) => ({
        status: "error",
        code: String(err),
        last: prev.status === "ready" ? prev.detail : prev.status === "error" ? prev.last : null,
      }));
    }
  }, [provider, sessionId]);

  useEffect(() => {
    void reload();
  }, [reload, tick]);

  return { load, reload };
}
