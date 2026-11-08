import { useId, useState } from "react";
import { useSessionDetail } from "../context/useSessionDetail";
import { ellipsize, providerName, sessionName } from "../format";
import type { SessionView } from "../sessions";
import { Banner } from "../ui/Banner";
import { Dialog } from "../ui/Dialog";
import type { ScopeKind } from "./api";
import {
  filterPickerSessions,
  MAX_SCOPE_SESSIONS,
  SCOPE_KINDS,
  sameSession,
  scopeHelp,
  scopeLabel,
  type DraftSession,
  type ScopeDraft,
} from "./logic";

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

/** A session's display name for chips, lists and group headings. */
export function pickedName(
  entry: { provider: string; sessionId: string },
  sessions: readonly SessionView[],
): string {
  const s = sessions.find(
    (x) => x.provider === entry.provider && x.providerSessionId === entry.sessionId,
  );
  return s
    ? `${ellipsize(sessionName(s).primary, 60)} (${providerName(s.provider)})`
    : "A session that is no longer listed";
}

function NotesPicker({
  entry,
  label,
  onChange,
  disabled,
}: {
  entry: DraftSession;
  label: string;
  onChange: (next: DraftSession) => void;
  disabled: boolean;
}) {
  const { load, reload } = useSessionDetail(
    { provider: entry.provider, sessionId: entry.sessionId },
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
      ...entry,
      itemIds: on ? [...entry.itemIds, id] : entry.itemIds.filter((x) => x !== id),
    });
  return (
    <fieldset className="pick-list" disabled={disabled}>
      <legend>Tick what to share from {label}</legend>
      {hasNotes && (
        <label className="pick">
          <input
            type="checkbox"
            checked={entry.notes}
            onChange={(e) => onChange({ ...entry, notes: e.target.checked })}
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
            checked={entry.itemIds.includes(item.id)}
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

/**
 * The second modal: search the sessions (locally, nothing is sent) and tick one or more. The
 * choice only applies when "Use these sessions" is pressed; Cancel keeps the previous choice.
 */
function SessionPickerDialog({
  open,
  initial,
  sessions,
  onCancel,
  onConfirm,
}: {
  open: boolean;
  initial: readonly DraftSession[];
  sessions: readonly SessionView[];
  onCancel: () => void;
  onConfirm: (picked: readonly { provider: string; sessionId: string }[]) => void;
}) {
  const searchId = useId();
  const [query, setQuery] = useState("");
  const [picked, setPicked] = useState<readonly { provider: string; sessionId: string }[]>(
    initial.map((s) => ({ provider: s.provider, sessionId: s.sessionId })),
  );
  const shown = filterPickerSessions(sessions, query);
  const full = picked.length >= MAX_SCOPE_SESSIONS;
  const toggle = (s: SessionView, on: boolean) => {
    const ref = { provider: s.provider, sessionId: s.providerSessionId };
    setPicked((cur) => (on ? [...cur, ref] : cur.filter((x) => !sameSession(x, ref))));
  };
  return (
    <Dialog
      open={open}
      title="Choose sessions"
      onClose={onCancel}
      actions={
        <>
          <button type="button" className="btn" onClick={onCancel}>
            Cancel
          </button>
          <button
            type="button"
            className="btn btn-primary"
            disabled={picked.length === 0}
            onClick={() => onConfirm(picked)}
          >
            Use these sessions
          </button>
        </>
      }
      description={
        <p>
          Tick one or more sessions, up to {MAX_SCOPE_SESSIONS}. Searching only filters this list on
          your computer; nothing is sent.
        </p>
      }
    >
      <div className="session-picker">
        <label className="field-label" htmlFor={searchId}>
          Search sessions
        </label>
        <input
          id={searchId}
          type="search"
          className="field session-search"
          placeholder="Title, tool or session ID"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <p className="field-note" aria-live="polite">
          {picked.length} of {MAX_SCOPE_SESSIONS} chosen
          {full ? ". That is the most; untick one to choose another." : ""}
        </p>
        {sessions.length === 0 ? (
          <p className="card-sub">No sessions to choose from yet.</p>
        ) : shown.length === 0 ? (
          <p className="card-sub">No session matches “{query.trim()}”.</p>
        ) : (
          <ul className="session-pick-list" aria-label="Sessions">
            {shown.map((s) => {
              const isOn = picked.some(
                (x) => x.provider === s.provider && x.sessionId === s.providerSessionId,
              );
              return (
                <li key={`${s.provider}:${s.providerSessionId}`}>
                  <label className="pick">
                    <input
                      type="checkbox"
                      checked={isOn}
                      disabled={!isOn && full}
                      onChange={(e) => toggle(s, e.target.checked)}
                    />
                    <span className="session-pick-text">
                      <span className="scope-name">{sessionName(s).primary}</span>
                      <span className="field-note">{sessionName(s).secondary}</span>
                    </span>
                  </label>
                </li>
              );
            })}
          </ul>
        )}
      </div>
    </Dialog>
  );
}

/** The explicit scope picker. It always starts at "No context" (the parent resets it per send). */
export function ContextPicker({ draft, onChange, sessions, disabled }: Props) {
  const groupName = useId();
  const [pickerOpen, setPickerOpen] = useState(false);
  // A new key on each open gives the second modal a fresh copy of the current choice.
  const [pickerRun, setPickerRun] = useState(0);

  const setKind = (kind: ScopeKind) =>
    onChange({
      kind,
      sessions: draft.sessions.map((s) => ({ ...s, notes: false, itemIds: [] })),
    });

  const confirmPicked = (picked: readonly { provider: string; sessionId: string }[]) => {
    const kept = draft.sessions.filter((s) => picked.some((p) => sameSession(p, s)));
    const added = picked
      .filter((p) => !kept.some((s) => sameSession(s, p)))
      .map((p) => ({ ...p, notes: false, itemIds: [] as number[] }));
    onChange({ ...draft, sessions: [...kept, ...added] });
    setPickerOpen(false);
  };

  const openPicker = () => {
    setPickerRun((n) => n + 1);
    setPickerOpen(true);
  };

  const remove = (entry: DraftSession) =>
    onChange({ ...draft, sessions: draft.sessions.filter((s) => !sameSession(s, entry)) });

  const update = (next: DraftSession) =>
    onChange({
      ...draft,
      sessions: draft.sessions.map((s) => (sameSession(s, next) ? next : s)),
    });

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
          <span className="field-label">Sessions</span>
          {sessions.length === 0 ? (
            <p className="card-sub">No sessions to choose from yet.</p>
          ) : (
            <div>
              <button type="button" className="btn" onClick={openPicker}>
                {draft.sessions.length === 0 ? "Choose sessions…" : "Change sessions…"}
              </button>
            </div>
          )}
          {draft.sessions.length > 0 && (
            <ul className="session-chips" aria-label="Chosen sessions">
              {draft.sessions.map((s) => {
                const name = pickedName(s, sessions);
                return (
                  <li key={`${s.provider}:${s.sessionId}`} className="session-chip">
                    <span className="session-chip-text" title={name}>
                      {name}
                    </span>
                    <button
                      type="button"
                      className="session-chip-remove"
                      aria-label={`Remove ${name}`}
                      onClick={() => remove(s)}
                    >
                      <span aria-hidden="true">×</span>
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
          {draft.kind === "selectedNotes" &&
            draft.sessions.map((s) => (
              <div key={`${s.provider}:${s.sessionId}`} className="notes-group">
                <h3 className="notes-group-title">{pickedName(s, sessions)}</h3>
                <NotesPicker
                  entry={s}
                  label={pickedName(s, sessions)}
                  onChange={update}
                  disabled={disabled}
                />
              </div>
            ))}
        </div>
      )}

      <SessionPickerDialog
        key={pickerRun}
        open={pickerOpen}
        initial={draft.sessions}
        sessions={sessions}
        onCancel={() => setPickerOpen(false)}
        onConfirm={confirmPicked}
      />
    </fieldset>
  );
}
