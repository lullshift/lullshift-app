import { useEffect, useRef } from "react";
import type { ToolCallRecord, ToolNoticeReason } from "./api";
import {
  approvalPrompt,
  plainLines,
  toolDecisionNote,
  toolNoticeText,
  type PendingTool,
} from "./logic";

/** Plain text, one block per line. Never HTML, Markdown or links. */
function Lines({ text, className }: { text: string; className: string }) {
  return (
    <div className={className}>
      {plainLines(text).map((line, i) => (
        <span key={i} className="msg-line">
          {line === "" ? " " : line}
        </span>
      ))}
    </div>
  );
}

interface ApprovalProps {
  pending: PendingTool;
  /** True while the answer is being sent. */
  busy: boolean;
  onDecide: (approve: boolean) => void;
}

/**
 * A lookup the model asked for. It shows the exact text that would be sent and waits: nothing
 * goes to the provider until Approve or Decline is pressed. Focus moves here when it appears.
 */
export function ToolApprovalCard({ pending, busy, onDecide }: ApprovalProps) {
  const ref = useRef<HTMLElement>(null);
  useEffect(() => {
    ref.current?.focus();
  }, [pending.approvalId]);
  return (
    <section
      ref={ref}
      tabIndex={-1}
      className="card tool-approval"
      aria-label={`Lookup waiting for your decision: ${pending.label}`}
    >
      <p className="card-title">{pending.label}</p>
      <p className="field-note">{approvalPrompt(pending)}</p>
      {pending.arguments.trim() !== "" && pending.arguments.trim() !== "{}" && (
        <p className="field-note">
          Request: <code>{pending.arguments}</code>
        </p>
      )}
      <pre className="preview-text" tabIndex={0} aria-label="Exact text that will be sent">
        {pending.resultText}
      </pre>
      <div className="actions">
        <button
          type="button"
          className="btn btn-primary"
          disabled={busy}
          onClick={() => onDecide(true)}
        >
          Approve
        </button>
        <button
          type="button"
          className="btn btn-secondary"
          disabled={busy}
          onClick={() => onDecide(false)}
        >
          Decline
        </button>
      </div>
    </section>
  );
}

interface RecordsProps {
  records: readonly ToolCallRecord[];
  /** Starting an activity is only ever the user's click. */
  onStartActivity?: (activityId: string) => void;
  /** Activity ids that are still installed; a card for any other is shown without a button. */
  installed?: ReadonlySet<string>;
}

/** What the lookups of one message sent, as the user approved or declined them. */
export function ToolRecords({ records, onStartActivity, installed }: RecordsProps) {
  if (records.length === 0) return null;
  return (
    <ul className="tool-records">
      {records.map((r) => (
        <li key={r.id} className="tool-record">
          <details>
            <summary>
              Lookup: {r.label} ({r.status === "approved" ? "approved" : "declined"})
            </summary>
            <p className="field-note">{toolDecisionNote(r)}</p>
            <Lines text={r.resultText} className="msg-attached" />
          </details>
          {r.card && (
            <div className="card break-card">
              <p className="card-title">{r.card.name}</p>
              <p className="field-note">
                About {r.card.minutes} minutes. Suggested by the Companion.
              </p>
              {onStartActivity && (installed === undefined || installed.has(r.card.activityId)) ? (
                <button
                  type="button"
                  className="btn btn-secondary"
                  onClick={() => onStartActivity(r.card?.activityId ?? "")}
                >
                  Start {r.card.name}
                </button>
              ) : (
                <p className="field-note">This activity isn't available any more.</p>
              )}
            </div>
          )}
        </li>
      ))}
    </ul>
  );
}

interface NoticesProps {
  notices: readonly { reason: ToolNoticeReason; tool: string | null }[];
}

/**
 * Lookups the model asked for that were not run. Plain text in the message flow, with no button:
 * there is nothing to approve. It is not a live region; the polite status announces it once.
 */
export function ToolNotices({ notices }: NoticesProps) {
  if (notices.length === 0) return null;
  return (
    <ul className="tool-notices">
      {notices.map((n, i) => (
        <li key={i} className="msg-note tool-notice">
          {toolNoticeText(n.reason, n.tool)}
        </li>
      ))}
    </ul>
  );
}
