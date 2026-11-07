import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import {
  CATCH_UP_LABEL,
  catchUpSummary,
  type CatchUpStatus,
  type TrackingPrefs,
} from "./sessionTracking";
import { Banner } from "./ui/Banner";
import { Switch } from "./ui/Switch";

type Load =
  | { status: "loading" }
  | { status: "ready"; prefs: TrackingPrefs; catchUp: CatchUpStatus | null }
  | { status: "error" };

/**
 * Settings > Sessions: the opt-in log catch-up (decision 35, ADR-025). Off by default. The
 * process check that ends sessions whose agent has exited is always on and has no setting.
 */
export function SessionTrackingSettings() {
  const [load, setLoad] = useState<Load>({ status: "loading" });
  const [saveError, setSaveError] = useState(false);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    Promise.all([
      invoke<TrackingPrefs>("session_tracking_prefs"),
      invoke<CatchUpStatus>("catch_up_status").catch(() => null),
    ])
      .then(([prefs, catchUp]) => setLoad({ status: "ready", prefs, catchUp }))
      .catch(() => setLoad({ status: "error" }));
  }, []);

  const save = async (next: TrackingPrefs) => {
    if (load.status !== "ready") return;
    setBusy(true);
    setSaveError(false);
    try {
      const saved = await invoke<TrackingPrefs>("set_session_tracking_prefs", { prefs: next });
      setLoad({ ...load, prefs: saved });
    } catch {
      setSaveError(true);
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <h2 className="section-title">Sessions</h2>
      <div className="card rows">
        {load.status === "error" ? (
          <Banner tone="danger">
            Session settings couldn't be read. Catch-up stays off until they can be.
          </Banner>
        ) : (
          <Switch
            label={CATCH_UP_LABEL}
            description={
              <>
                When Lullshift starts, it reads your agents' own log files to fill in what it missed
                while it was closed. Only times and whether a turn ended are used; titles, prompts
                and code are not kept. It only updates sessions Lullshift already tracks and never
                overrides a newer update from the agent. What it fills in is shown as a guess (From
                log) until the agent reports. Nothing leaves this computer.
                {load.status === "ready" && (
                  <span role="status">
                    {" "}
                    {catchUpSummary(load.prefs.catchUpFromLogs, load.catchUp)}
                  </span>
                )}
              </>
            }
            checked={load.status === "ready" && load.prefs.catchUpFromLogs}
            disabled={busy || load.status !== "ready"}
            onChange={(on) => void save({ catchUpFromLogs: on })}
          />
        )}
        {saveError && <Banner tone="danger">The setting wasn't saved. Try again.</Banner>}
        <p className="row-desc">
          Separately, Lullshift notices when an agent's program has exited without saying goodbye
          and shows the session as "Ended (process gone)". That check is always on, only looks at
          whether the program is still running, and a new update from the agent reopens the session.
        </p>
      </div>
    </>
  );
}
