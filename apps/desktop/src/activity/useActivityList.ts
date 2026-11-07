import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
import type { ActivityInfo } from "./ActivityHost";

export type ActivityLoad =
  { status: "loading" } | { status: "ready"; list: ActivityInfo[] } | { status: "error" };

/**
 * The enabled, installed activities, read from the Rust core. One list feeds the sidebar counts,
 * Home, Play, Rest and Continue; `refresh` re-reads it (after a close, a manage change, a visit).
 */
export function useActivityList(): { load: ActivityLoad; refresh: () => void } {
  const [load, setLoad] = useState<ActivityLoad>({ status: "loading" });
  const refresh = useCallback(() => {
    invoke<ActivityInfo[]>("activities_list")
      .then((list) => setLoad({ status: "ready", list }))
      .catch(() => setLoad({ status: "error" }));
  }, []);
  useEffect(refresh, [refresh]);
  return { load, refresh };
}
