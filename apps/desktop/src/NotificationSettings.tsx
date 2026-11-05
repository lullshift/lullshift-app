import { invoke } from "@tauri-apps/api/core";
import { useEffect, useId, useState } from "react";
import { Banner } from "./ui/Banner";
import { Switch } from "./ui/Switch";

/** Mirrors `NotificationPrefs` in `src-tauri/src/notify.rs`. */
export interface NotificationPrefs {
  needsYou: boolean;
  finished: boolean;
  quietHours: {
    enabled: boolean;
    start: string;
    end: string;
    allowNeedsYou: boolean;
  };
}

type Load =
  { status: "loading" } | { status: "ready"; prefs: NotificationPrefs } | { status: "error" };

/** What Lullshift tells you about, and when to stay quiet. Saved on every change. */
export function NotificationSettings() {
  const [load, setLoad] = useState<Load>({ status: "loading" });
  const [saveError, setSaveError] = useState(false);
  const [busy, setBusy] = useState(false);
  const startId = useId();
  const endId = useId();

  useEffect(() => {
    invoke<NotificationPrefs>("notification_prefs")
      .then((prefs) => setLoad({ status: "ready", prefs }))
      .catch(() => setLoad({ status: "error" }));
  }, []);

  if (load.status === "loading") {
    return <p className="row-desc">Loading notification settings…</p>;
  }
  if (load.status === "error") {
    return (
      <Banner tone="danger">
        Notification settings couldn't be read. Lullshift keeps using the defaults.
      </Banner>
    );
  }
  const { prefs } = load;
  const q = prefs.quietHours;

  const save = async (next: NotificationPrefs) => {
    setBusy(true);
    setSaveError(false);
    try {
      const saved = await invoke<NotificationPrefs>("set_notification_prefs", { prefs: next });
      setLoad({ status: "ready", prefs: saved });
    } catch {
      setSaveError(true);
    } finally {
      setBusy(false);
    }
  };
  const quiet = (patch: Partial<NotificationPrefs["quietHours"]>) =>
    void save({ ...prefs, quietHours: { ...q, ...patch } });

  return (
    <>
      <Switch
        label="When a session needs you"
        description="A permission prompt or a question. This is the one that matters most."
        checked={prefs.needsYou}
        disabled={busy}
        onChange={(on) => void save({ ...prefs, needsYou: on })}
      />
      <Switch
        label="When a session finishes or fails"
        description="Only when the agent reports it. A reply ending isn't the task finishing."
        checked={prefs.finished}
        disabled={busy}
        onChange={(on) => void save({ ...prefs, finished: on })}
      />
      <Switch
        label="Quiet hours"
        description={
          q.enabled
            ? q.start === q.end
              ? "Start and end are the same time, so nothing is held back."
              : `Quiet from ${q.start} to ${q.end}, your local time.`
            : "Hold notifications back at set times."
        }
        checked={q.enabled}
        disabled={busy}
        onChange={(on) => quiet({ enabled: on })}
      />
      {q.enabled && (
        <div className="row row-nested">
          <div className="time-fields">
            <label htmlFor={startId}>From</label>
            <input
              id={startId}
              type="time"
              className="field"
              value={q.start}
              disabled={busy}
              required
              onChange={(e) => e.target.value && quiet({ start: e.target.value })}
            />
            <label htmlFor={endId}>to</label>
            <input
              id={endId}
              type="time"
              className="field"
              value={q.end}
              disabled={busy}
              required
              onChange={(e) => e.target.value && quiet({ end: e.target.value })}
            />
          </div>
        </div>
      )}
      {q.enabled && (
        <Switch
          label="Still tell me when a session needs me"
          description="During quiet hours, everything else waits."
          checked={q.allowNeedsYou}
          disabled={busy}
          onChange={(on) => quiet({ allowNeedsYou: on })}
        />
      )}
      {saveError && (
        <Banner tone="danger">
          That change couldn't be saved. Your previous settings still apply.
        </Banner>
      )}
    </>
  );
}
