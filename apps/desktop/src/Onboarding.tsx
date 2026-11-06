import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import type { ActivityInfo } from "./activity/ActivityHost";
import {
  CONNECT_EXPLAINER,
  WELCOME_LINES,
  WELCOME_TITLE,
  suggestFirstActivity,
} from "./welcomeCopy";
import { Dialog } from "./ui/Dialog";
import { Icon } from "./ui/Icon";

interface Props {
  open: boolean;
  /** Closes the welcome for good (saved), whichever way it was closed. */
  onDone: () => void;
  onConnect: () => void;
  onTryActivity: () => void;
}

/**
 * Shown once, on first launch. Everything is optional and Skip (or Esc) closes it for good.
 * "Connect" only goes to Integrations: the consent dialog there names the exact file.
 */
export function Onboarding({ open, onDone, onConnect, onTryActivity }: Props) {
  const [suggestion, setSuggestion] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    invoke<ActivityInfo[]>("activities_list")
      .then((list) => !cancelled && setSuggestion(suggestFirstActivity(list)?.name ?? null))
      .catch(() => undefined); // No suggestion: the generic wording below still works.
    return () => {
      cancelled = true;
    };
  }, [open]);

  return (
    <Dialog
      open={open}
      title={WELCOME_TITLE}
      onClose={onDone}
      actions={
        <>
          <button type="button" className="btn btn-quiet" onClick={onDone}>
            Skip for now
          </button>
          <button
            type="button"
            className="btn btn-secondary"
            onClick={() => {
              onDone();
              onTryActivity();
            }}
          >
            {suggestion ? `Try ${suggestion}` : "Browse activities"}
          </button>
          <button
            type="button"
            className="btn btn-primary"
            onClick={() => {
              onDone();
              onConnect();
            }}
          >
            Set up Claude Code
          </button>
        </>
      }
    >
      {WELCOME_LINES.map((line) => (
        <p key={line}>{line}</p>
      ))}
      <ul className="checklist">
        <li>
          <Icon name="plug" size={18} />
          <span>{CONNECT_EXPLAINER}</span>
        </li>
        <li>
          <Icon name="play" size={18} />
          <span>
            {suggestion
              ? `A good first step: ${suggestion}, a short activity that pauses and saves when an agent needs you.`
              : "Pick a short activity from the library. It pauses and saves when an agent needs you."}
          </span>
        </li>
      </ul>
      <p className="dialog-note">
        You can find all of this later under Integrations and Activities.
      </p>
    </Dialog>
  );
}
