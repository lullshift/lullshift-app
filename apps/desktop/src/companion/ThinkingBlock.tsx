// The model's reasoning summary, above its reply (ADR-020, "Thinking"). Model output is
// untrusted: it is shown as plain React text, never as HTML, links or formatted blocks.
//
// Open while the model is still thinking (no reply text yet), collapsed once the reply starts or
// has finished. A click on the header overrides that for good. The header is a real button with
// `aria-expanded`, so Enter and Space work and a screen reader hears the state.

import { useId, useState } from "react";
import { stripInvisible } from "../format";
import { getChoice, setChoice } from "./thinkingChoice";
import "./thinking.css";

/** "Thought for 12 s", "Thought for 2 min 5 s", or just "Thinking" when no time is known. */
export function thinkingHeader(ms: number | null | undefined, active: boolean): string {
  if (active) return "Thinking…";
  if (ms === null || ms === undefined || !Number.isFinite(ms) || ms < 0) return "Thinking";
  const total = Math.max(1, Math.round(ms / 1000));
  if (total < 60) return `Thought for ${total} s`;
  const min = Math.floor(total / 60);
  const sec = total % 60;
  return sec === 0 ? `Thought for ${min} min` : `Thought for ${min} min ${sec} s`;
}

export function ThinkingBlock({
  text,
  ms = null,
  active = false,
  choiceKey,
}: {
  /** The reasoning summary; plain text. Nothing renders when it is blank. */
  text: string;
  /** How long it took, in milliseconds, if known. */
  ms?: number | null;
  /** The model is still thinking: no reply text has started yet. Shown open, header "Thinking…". */
  active?: boolean;
  /** Where the user's open/closed choice is remembered (memory only), so it survives the swap
   * from the live block to the stored reply and leaving the conversation. */
  choiceKey?: string;
}) {
  const bodyId = useId();
  // null: follow the default (open while thinking, collapsed after). Set once the user clicks.
  const [chosen, setChosen] = useState<boolean | null>(() =>
    choiceKey ? getChoice(choiceKey) : null,
  );
  const clean = stripInvisible(text);
  if (clean.trim() === "") return null;
  const open = chosen ?? active;
  return (
    <section className="thinking" aria-label="Thinking">
      <button
        type="button"
        className="thinking-head"
        aria-expanded={open}
        aria-controls={bodyId}
        onClick={() => {
          setChosen(!open);
          if (choiceKey) setChoice(choiceKey, !open);
        }}
      >
        <span className="thinking-chevron" aria-hidden="true" />
        <span>{thinkingHeader(ms, active)}</span>
      </button>
      <div id={bodyId} className="thinking-body" hidden={!open} tabIndex={0}>
        {clean}
      </div>
    </section>
  );
}
