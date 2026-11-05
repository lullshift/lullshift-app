import { useId } from "react";
import { useSessionDetail } from "../context/useSessionDetail";
import { ellipsize, providerName, sessionName } from "../format";
import type { SessionView } from "../sessions";
import { Banner } from "../ui/Banner";
import type { ScopeKind } from "./api";
import { SCOPE_KINDS, scopeHelp, scopeLabel, type ScopeDraft } from "./logic";

interface Props {
  draft: ScopeDraft;
  onChange: (next: ScopeDraft) => void;
  sessions: readonly SessionView[];
  disabled: boolean;
}

const ITEM_KIND: Record<string, string> = {
  decision: "Decision",
  checklist: "Checklist",
  next_move: "Next move",
};

function NotesPicker({
  draft,
  onChange,
  disabled,
}: {
  draft: ScopeDraft;
  onChange: (next: ScopeDraft) => void;
  disabled: boolean;
}) {
  const { load, reload } = useSessionDetail(
    { provider: draft.provider, sessionId: draft.sessionId },
    0,
  );
  if (load.status === "loading") return <p className="card-sub">Loading this session's notes…</p>;
  if (load.status === "error" && !load.last) {
    return (
      <Banner tone="danger">
        This session's notes couldn't be read, so nothing can be picked from it.{" "}
        <button type="button" className="btn-link" onClick={() => void reload()}>
          Try again
        </button>
      </Banner>
    );
  }
  const detail = load.status === "ready" ? load.detail : load.status === "error" ? load.last : null;
  if (!detail) return null;
  const hasNotes = detail.context.notes.trim().length > 0;
  if (!hasNotes && detail.items.length === 0) {
    return <p className="card-sub">This session has no notes or items to share.</p>;
  }
  const toggleItem = (id: number, on: boolean) =>
    onChange({
      ...draft,
      itemIds: on ? [...draft.itemIds, id] : draft.itemIds.filter((x) => x !== id),
    });
  return (
    <fieldset className="pick-list" disabled={disabled}>
      <legend>Tick what to share</legend>
      {hasNotes && (
        <label className="pick">
          <input
            type="checkbox"
            checked={draft.notes}
            onChange={(e) => onChange({ ...draft, notes: e.target.checked })}
          />
          <span>
            Your notes{" "}
            <span className="field-note">
              ({detail.context.notes.length.toLocaleString("en-US")} characters)
            </span>
          </span>
        </label>
      )}
      {detail.items.map((item) => (
        <label key={item.id} className="pick">
          <input
            type="checkbox"
            checked={draft.itemIds.includes(item.id)}
            onChange={(e) => toggleItem(item.id, e.target.checked)}
          />
          <span>
            <span className="field-note">
              {ITEM_KIND[item.kind] ?? "Item"}
              {item.done ? ", done" : ""}:{" "}
            </span>
            {ellipsize(item.text.replace(/\s+/g, " "), 160)}
          </span>
        </label>
      ))}
    </fieldset>
  );
}

/** The explicit scope picker. It always starts at "No context" (the parent resets it per send). */
export function ContextPicker({ draft, onChange, sessions, disabled }: Props) {
  const groupName = useId();
  const selectId = useId();
  const sessionIndex = sessions.findIndex(
    (s) => s.provider === draft.provider && s.providerSessionId === draft.sessionId,
  );

  const setKind = (kind: ScopeKind) => onChange({ ...draft, kind, notes: false, itemIds: [] });

  const setSession = (value: string) => {
    const s = sessions[Number(value)];
    onChange({
      ...draft,
      provider: s?.provider ?? "",
      sessionId: s?.providerSessionId ?? "",
      notes: false,
      itemIds: [],
    });
  };

  return (
    <fieldset className="scope-picker" disabled={disabled}>
      <legend>Context to share with this message</legend>
      <div className="scope-options">
        {SCOPE_KINDS.map((kind) => (
          <label key={kind} className="scope-option">
            <input
              type="radio"
              name={groupName}
              checked={draft.kind === kind}
              onChange={() => setKind(kind)}
            />
            <span>
              <span className="scope-name">{scopeLabel(kind)}</span>
              <span className="field-note">{scopeHelp(kind)}</span>
            </span>
          </label>
        ))}
      </div>

      {draft.kind !== "none" && (
        <div className="scope-session">
          <label className="field-label" htmlFor={selectId}>
            Session
          </label>
          {sessions.length === 0 ? (
            <p className="card-sub">No sessions to choose from yet.</p>
          ) : (
            <select
              id={selectId}
              className="field"
              value={sessionIndex >= 0 ? String(sessionIndex) : ""}
              onChange={(e) => setSession(e.target.value)}
            >
              <option value="" disabled>
                Choose a session…
              </option>
              {sessions.map((s, i) => (
                <option key={`${s.provider}:${s.providerSessionId}`} value={String(i)}>
                  {ellipsize(sessionName(s).primary, 70)} ({providerName(s.provider)})
                </option>
              ))}
            </select>
          )}
          {draft.kind === "selectedNotes" && sessionIndex >= 0 && (
            <NotesPicker
              key={`${draft.provider}:${draft.sessionId}`}
              draft={draft}
              onChange={onChange}
              disabled={disabled}
            />
          )}
        </div>
      )}
    </fieldset>
  );
}
