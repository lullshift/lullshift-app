// "How to play" (S05-T07). Help comes from the activity's manifest and is untrusted: it is
// rendered only as React text, never as HTML or Markdown. Rust and the schema already limit it
// to plain text.

import type { Help } from "@lullshift/activity-sdk";
import { Dialog } from "../ui/Dialog";
import { helpLabel } from "./library";

interface Props {
  open: boolean;
  name: string;
  description: string;
  help: Help | null;
  /** The activity's category: breaks say "How it works", the rest "How to play". */
  category: string;
  onClose: () => void;
}

export function HelpDialog({ open, name, description, help, category, onClose }: Props) {
  const label = helpLabel(category);
  return (
    <Dialog
      open={open}
      title={`${label} ${name}`}
      onClose={onClose}
      actions={
        <button type="button" className="btn btn-primary" onClick={onClose}>
          Got it
        </button>
      }
    >
      {help ? <HelpBody help={help} label={label} /> : <NoHelp description={description} />}
    </Dialog>
  );
}

function HelpBody({ help, label }: { help: Help; label: string }) {
  return (
    <div className="help">
      <p className="help-summary">{help.summary}</p>
      {help.goal && (
        <section className="help-section">
          <h3 className="help-heading">Goal</h3>
          <p>{help.goal}</p>
        </section>
      )}
      <section className="help-section">
        <h3 className="help-heading">{label}</h3>
        <ol className="help-steps">
          {help.howToPlay.map((step, i) => (
            <li key={i}>{step}</li>
          ))}
        </ol>
      </section>
      {help.controls && help.controls.length > 0 && (
        <section className="help-section">
          <h3 className="help-heading">Controls</h3>
          <dl className="help-controls">
            {help.controls.map((c, i) => (
              <div key={i} className="help-control">
                <dt>
                  <kbd>{c.keys}</kbd>
                </dt>
                <dd>{c.action}</dd>
              </div>
            ))}
          </dl>
        </section>
      )}
      {help.tips && help.tips.length > 0 && (
        <section className="help-section">
          <h3 className="help-heading">Tips</h3>
          <ul className="help-tips">
            {help.tips.map((t, i) => (
              <li key={i}>{t}</li>
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}

function NoHelp({ description }: { description: string }) {
  return (
    <div className="help">
      {description && <p className="help-summary">{description}</p>}
      <p className="help-missing">This activity doesn't include help yet.</p>
    </div>
  );
}
