// Zustand view state for the dashboard. Data always comes from the Rust core; this store only
// tracks loading/error and the latest snapshot.

import { createStore } from "zustand/vanilla";
import type { Dashboard } from "./sessions";

export type DashboardState =
  | { status: "loading" }
  | { status: "ready"; dashboard: Dashboard }
  | { status: "error"; message: string; last: Dashboard | null };

export interface DashboardStore {
  state: DashboardState;
  refresh: () => Promise<void>;
}

export function createDashboardStore(load: () => Promise<Dashboard>) {
  let generation = 0;
  return createStore<DashboardStore>()((set, get) => ({
    state: { status: "loading" },
    refresh: async () => {
      const mine = ++generation;
      try {
        const dashboard = await load();
        if (mine === generation) set({ state: { status: "ready", dashboard } });
      } catch (err: unknown) {
        if (mine !== generation) return;
        const prev = get().state;
        const last =
          prev.status === "ready" ? prev.dashboard : prev.status === "error" ? prev.last : null;
        set({ state: { status: "error", message: String(err), last } });
      }
    },
  }));
}
