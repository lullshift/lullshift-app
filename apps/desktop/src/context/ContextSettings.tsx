import { useEffect, useId, useState } from "react";
import { Banner } from "../ui/Banner";
import { Dialog } from "../ui/Dialog";
import {
  clearAllCapturedContext,
  contextPrefs,
  setContextPrefs,
  type ContextPrefs,
  type RetentionDays,
} from "./api";

const RETENTION: RetentionDays[] = [7, 30, 90, 365];

type Load = { status: "loading" } | { status: "ready"; prefs: ContextPrefs } | { status: "error" };
type Message = { tone: "success" | "danger"; text: string };

/** Settings > Context: how long events are kept, and clearing captured text everywhere. */
export function ContextSettings() {
  const selectId = useId();
  const [load, setLoad] = useState<Load>({ status: "loading" });
  const [message, setMessage] = useState<Message | null>(null);
  const [confirm, setConfirm] = useState(false);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    contextPrefs()
      .then((prefs) => setLoad({ status: "ready", prefs }))
      .catch(() => setLoad({ status: "error" }));
  }, []);

  const changeRetention = async (days: RetentionDays) => {
    if (load.status !== "ready") return;
    setBusy(true);
    setMessage(null);
    try {
      // Re-read first so a capture switch changed elsewhere isn't overwritten.
      const current = await contextPrefs();
      const next = { ...current, retentionDays: days };
      await setContextPrefs(next);
      setLoad({ status: "ready", prefs: next });
    } catch {
      setMessage({ tone: "danger", text: "The retention setting wasn't saved. Try again." });
    } finally {
      setBusy(false);
    }
  };

  const clearAll = async () => {
    setBusy(true);
    try {
      await clearAllCapturedContext();
      setMessage({ tone: "success", text: "All captured text was cleared. Your own notes stay." });
    } catch {
      setMessage({ tone: "danger", text: "Captured text wasn't cleared. Try again." });
    } finally {
      setBusy(false);
      setConfirm(false);
    }
  };

  return (
    <>
      <h2 className="section-title">Context</h2>
      <div className="card rows">
        <div className="row">
          <div className="row-text">
            <label htmlFor={selectId} className="row-label">
              Keep events for
            </label>
            <span className="row-desc">
              {load.status === "error"
                ? "The setting couldn't be read. Lullshift keeps events 30 days."
                : "Older agent events are deleted. Your notes and items stay until you delete them."}
            </span>
          </div>
          <select
            id={selectId}
            className="field"
            disabled={busy || load.status !== "ready"}
            value={load.status === "ready" ? load.prefs.retentionDays : 30}
            onChange={(e) => void changeRetention(Number(e.target.value) as RetentionDays)}
          >
            {RETENTION.map((d) => (
              <option key={d} value={d}>
                {d} days
              </option>
            ))}
          </select>
        </div>
        <div className="row">
          <div className="row-text">
            <span className="row-label">Captured text</span>
            <span className="row-desc">
              Text Lullshift kept from your agents, only if you turned on task context. Clearing it
              doesn't touch what you wrote.
            </span>
          </div>
          <button
            type="button"
            className="btn btn-secondary"
            disabled={busy}
            onClick={() => setConfirm(true)}
          >
            Clear all captured text
          </button>
        </div>
        {message && <Banner tone={message.tone}>{message.text}</Banner>}
      </div>

      <Dialog
        open={confirm}
        title="Clear all captured text?"
        busy={busy}
        onClose={() => setConfirm(false)}
        actions={
          <>
            <button
              type="button"
              className="btn btn-quiet"
              disabled={busy}
              onClick={() => setConfirm(false)}
            >
              Cancel
            </button>
            <button
              type="button"
              className="btn btn-danger"
              disabled={busy}
              onClick={() => void clearAll()}
            >
              {busy ? "Clearing…" : "Clear captured text"}
            </button>
          </>
        }
      >
        <p>
          This removes the intents, requests and questions Lullshift captured from your agents, for
          every session. It can't be undone.
        </p>
        <p>Your own notes, items and intents stay.</p>
      </Dialog>
    </>
  );
}
