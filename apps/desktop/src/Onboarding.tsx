import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import type { ActivityInfo } from "./activity/ActivityHost";
import { SUPPORTED_AGENTS, allAgentsConnected, statusCommand, type AdapterView } from "./adapters";
import {
  CONNECT_EXPLAINER,
  CONNECT_LABEL,
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
  /** Opens the library page for the suggested activity (its category), or Play if none. */
  onTryActivity: (category: string | null) => void;
}

/**
 * Shown once, on first launch. Everything is optional and Skip (or Esc) closes it for good.
 * "Connect" only goes to Settings > Integrations: the consent dialog there names the exact file.
 */
export function Onboarding({ open, onDone, onConnect, onTryActivity }: Props) {
  const [suggestion, setSuggestion] = useState<string | null>(null);
  const [suggestionCategory, setSuggestionCategory] = useState<string | null>(null);
  // Unknown until read. Unknown or unreadable keeps the Connect button, so it is never hidden
  // on a guess.
  const [allConnected, setAllConnected] = useState(false);

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    invoke<ActivityInfo[]>("activities_list")
      .then((list) => {
        if (cancelled) return;
        const first = suggestFirstActivity(list);
        setSuggestion(first?.name ?? null);
        setSuggestionCategory(first?.category ?? null);
      })
      .catch(() => undefined); // No suggestion: the generic wording below still works.
    Promise.all(
      SUPPORTED_AGENTS.map((agent) =>
        invoke<AdapterView>(statusCommand(agent))
          .then((v) => v.status)
          .catch(() => null),
      ),
    )
      .then((statuses) => !cancelled && setAllConnected(allAgentsConnected(statuses)))
      .catch(() => undefined);
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
              onTryActivity(suggestionCategory);
            }}
          >
            {suggestion ? `Try ${suggestion}` : "Browse activities"}
          </button>
          {!allConnected && (
            <button
              type="button"
              className="btn btn-primary"
              onClick={() => {
                onDone();
                onConnect();
              }}
            >
              {CONNECT_LABEL}
            </button>
          )}
        </>
      }
    >
      {WELCOME_LINES.map((line) => (
        <p key={line}>{line}</p>
      ))}
      <ul className="checklist">
        {!allConnected && (
          <li>
            <Icon name="plug" size={18} />
            <span>{CONNECT_EXPLAINER}</span>
          </li>
        )}
        <li>
          <Icon name="play" size={18} />
          <span>
            {suggestion
              ? `A good first step: ${suggestion}, a short activity that pauses and saves when an agent needs you.`
              : "Pick a short game or rest. It pauses and saves when an agent needs you."}
          </span>
        </li>
      </ul>
      <p className="dialog-note">You can find all of this later under Settings, Play and Rest.</p>
    </Dialog>
  );
}
