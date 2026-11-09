// "This looks wrong" (ADR-028 section 8, quiz and teaser sets): the user picks the item that
// looks wrong from the set's own list and it is left out the next time the set starts. The
// sandboxed engine doesn't tell the host which item is on screen, so the user picks it here.
// Everything stays on this computer: nothing is sent and the model is not told.

import { useEffect, useId, useState } from "react";
import {
  generatedSetItems,
  generatedSetSkip,
  generationErrorText,
  toGenerationError,
} from "../companion/generate";
import { Banner } from "../ui/Banner";
import { Dialog } from "../ui/Dialog";

interface Props {
  open: boolean;
  setId: string;
  onClose: () => void;
}

type Load =
  { status: "loading" } | { status: "ready"; items: string[] } | { status: "error"; text: string };

export function ThisLooksWrongDialog({ open, setId, onClose }: Props) {
  const name = useId();
  const [load, setLoad] = useState<Load>({ status: "loading" });
  const [pick, setPick] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const [done, setDone] = useState(false);

  // Each time the dialog opens it starts clean (adjusting state while rendering).
  const [wasOpen, setWasOpen] = useState(false);
  if (open !== wasOpen) {
    setWasOpen(open);
    if (open) {
      setLoad({ status: "loading" });
      setPick(null);
      setProblem(null);
      setDone(false);
    }
  }

  useEffect(() => {
    if (!open) return;
    let live = true;
    generatedSetItems(setId)
      .then((items) => {
        if (live) setLoad({ status: "ready", items });
      })
      .catch((err: unknown) => {
        if (live) setLoad({ status: "error", text: generationErrorText(toGenerationError(err)) });
      });
    return () => {
      live = false;
    };
  }, [open, setId]);

  const skip = async () => {
    if (pick === null) return;
    setBusy(true);
    setProblem(null);
    try {
      await generatedSetSkip(setId, pick);
      setDone(true);
    } catch (err) {
      setProblem(generationErrorText(toGenerationError(err)));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog
      open={open}
      title="This looks wrong"
      onClose={onClose}
      busy={busy}
      description={
        <p>
          Pick the item that looks wrong. It will be left out the next time this set starts. This
          stays on your computer: nothing is sent and your model isn&apos;t told.
        </p>
      }
      actions={
        done ? (
          <button type="button" className="btn btn-primary" autoFocus onClick={onClose}>
            Done
          </button>
        ) : (
          <>
            <button type="button" className="btn btn-quiet" onClick={onClose}>
              Cancel
            </button>
            <button
              type="button"
              className="btn btn-primary"
              disabled={pick === null || busy}
              onClick={() => void skip()}
            >
              Leave it out
            </button>
          </>
        )
      }
    >
      {load.status === "loading" && <p className="field-note">Reading the set…</p>}
      {load.status === "error" && <Banner tone="danger">{load.text}</Banner>}
      {problem && <Banner tone="danger">{problem}</Banner>}
      {done && (
        <Banner tone="success">
          <span role="status">It will be left out the next time this set starts.</span>
        </Banner>
      )}
      {load.status === "ready" && !done && (
        <fieldset className="wrong-list">
          <legend className="visually-hidden">Items in this set</legend>
          {load.items.map((text, i) => (
            <label key={i} className="pick">
              <input type="radio" name={name} checked={pick === i} onChange={() => setPick(i)} />
              <span>{text || `Item ${i + 1}`}</span>
            </label>
          ))}
        </fieldset>
      )}
    </Dialog>
  );
}
