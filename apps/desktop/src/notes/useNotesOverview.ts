import { useCallback, useEffect, useRef, useState } from "react";
import { notesOverview, type NotesOverview } from "../context/api";

export type NotesLoad =
  | { status: "loading" }
  | { status: "ready"; overview: NotesOverview }
  | { status: "error"; code: string; last: NotesOverview | null };

/**
 * Loads the Notes overview from the Rust core and reloads it when `tick` changes (the
 * sessions-changed event) or `reload()` is called after an edit. Rust stays authoritative: this
 * only holds the last snapshot, and a failed reload keeps showing the last good one.
 */
export function useNotesOverview(tick: number) {
  const [load, setLoad] = useState<NotesLoad>({ status: "loading" });
  const generation = useRef(0);

  const reload = useCallback(async () => {
    const mine = ++generation.current;
    try {
      const overview = await notesOverview();
      if (mine === generation.current) setLoad({ status: "ready", overview });
    } catch (err: unknown) {
      if (mine !== generation.current) return;
      setLoad((prev) => ({
        status: "error",
        code: String(err),
        last: prev.status === "ready" ? prev.overview : prev.status === "error" ? prev.last : null,
      }));
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload, tick]);

  return { load, reload };
}

/** The overview to show: the current one, or the last good one after a failed reload. */
export const overviewOf = (load: NotesLoad): NotesOverview | null =>
  load.status === "ready" ? load.overview : load.status === "error" ? load.last : null;
