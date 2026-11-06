import { useEffect, useState } from "react";
import { Banner } from "../ui/Banner";
import { Dialog } from "../ui/Dialog";
import { Switch } from "../ui/Switch";
import { contextPrefs, setContextPrefs } from "./api";
import type { AgentId } from "../adapters";
import {
  agentDisplayName,
  CONSENT_SCOPE_NOTE,
  consentItems,
  consentSwitchDescription,
} from "./consentCopy";

/** The opt-in "Task context" switch on an agent card (ADR-013). Off until the user agrees. */
export function TaskContextSwitch({ agent }: { agent: AgentId }) {
  const name = agentDisplayName(agent);
  const [on, setOn] = useState<boolean | null>(null); // null: not read yet or unknown
  const [failed, setFailed] = useState<string | null>(null);
  const [asking, setAsking] = useState(false);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    contextPrefs()
      .then((p) => setOn(p.capture[agent] === true))
      .catch(() => setFailed("Task context status is unknown. Nothing is captured."));
  }, [agent]);

  const save = async (next: boolean) => {
    setBusy(true);
    setFailed(null);
    try {
      // Re-read first so the retention setting changed elsewhere isn't overwritten.
      const current = await contextPrefs();
      await setContextPrefs({ ...current, capture: { ...current.capture, [agent]: next } });
      setOn(next);
    } catch {
      setFailed("The task context setting wasn't changed. Try again.");
    } finally {
      setBusy(false);
      setAsking(false);
    }
  };

  return (
    <>
      <div className="integration-switch">
        <Switch
          label="Task context"
          description={consentSwitchDescription(agent)}
          checked={on === true}
          disabled={busy || on === null}
          onChange={(next) => (next ? setAsking(true) : void save(false))}
        />
      </div>
      {failed && <Banner tone="danger">{failed}</Banner>}

      <Dialog
        open={asking}
        title="Turn on task context?"
        busy={busy}
        onClose={() => setAsking(false)}
        actions={
          <>
            <button
              type="button"
              className="btn btn-quiet"
              disabled={busy}
              onClick={() => setAsking(false)}
            >
              Cancel
            </button>
            <button
              type="button"
              className="btn btn-primary"
              disabled={busy}
              onClick={() => void save(true)}
            >
              Turn on
            </button>
          </>
        }
        description={<p>Here is exactly what Lullshift keeps from {name}:</p>}
      >
        <ul className="checklist">
          {consentItems(agent).map((item) => (
            <li key={item}>
              <span>{item}</span>
            </li>
          ))}
        </ul>
        <p className="card-sub">{CONSENT_SCOPE_NOTE}</p>
      </Dialog>
    </>
  );
}
