import { useCallback, useEffect, useId, useRef, useState, type ReactNode } from "react";
import { providerName, relativeTime, sessionName, shortId } from "../format";
import { reasonText, sessionStateView, type SessionView } from "../sessions";
import { Banner } from "../ui/Banner";
import { Dialog } from "../ui/Dialog";
import { Icon } from "../ui/Icon";
import { StatePill } from "../ui/StatePill";
import {
  addSessionItem,
  clearCapturedContext,
  deleteSession,
  deleteSessionItem,
  exportSession,
  linkSessions,
  markReviewed,
  moveSessionItem,
  setSessionIntent,
  setSessionNotes,
  setSessionTitle,
  unlinkSessions,
  updateSessionItem,
  type ExportFormat,
  type ItemKind,
  type SessionDetail,
  type SessionKey,
} from "./api";
import { ItemList } from "./ItemList";
import {
  askFallback,
  checklistText,
  itemsOfKind,
  itemsSummary,
  keyString,
  localTime,
  panelMode,
  timelineLabel,
  viewKey,
} from "./panelLogic";
import { SavedText } from "./SavedText";
import { contextErrorText, useSessionDetail } from "./useSessionDetail";

interface Props {
  session: SessionView;
  /** Every known session, for the link picker and names of linked sessions. */
  sessions: readonly SessionView[];
  now: number;
  staleAfterSeconds: number;
  /** Bumps on each sessions-changed event so the open panel re-reads its detail. */
  tick: number;
  pinned: boolean;
  floating: boolean;
  onTogglePin: () => void;
  onClose: () => void;
  onOpenSession: (key: SessionKey) => void;
  onOpenSettings: () => void;
  /** Something changed that the board shows (for example the Review pill). */
  onChanged: () => void;
  onDeleted: () => void;
}

/** The session detail panel (S04-T07). Shows only data the user consented to (ADR-013). */
export default function DetailPanel(props: Props) {
  const { session, now, staleAfterSeconds, tick, pinned, floating } = props;
  const key = viewKey(session);
  const { load, reload } = useSessionDetail(key, tick);
  const [error, setError] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const headingRef = useRef<HTMLHeadingElement>(null);
  const { onChanged } = props;

  // Land in the panel when it opens, so keyboard users start inside it.
  useEffect(() => {
    headingRef.current?.focus();
  }, []);

  const detail = load.status === "ready" ? load.detail : load.status === "error" ? load.last : null;
  const mode = panelMode(session);
  const view = sessionStateView(session);
  const sname = sessionName(session);
  const [renaming, setRenaming] = useState(false);
  const renameBtn = useRef<HTMLButtonElement>(null);
  const wasRenaming = useRef(false);

  // Put focus back on "Rename" when the editor closes.
  useEffect(() => {
    if (wasRenaming.current && !renaming) renameBtn.current?.focus();
    wasRenaming.current = renaming;
  }, [renaming]);
  const name = providerName(session.provider);
  const why = reasonText(session, staleAfterSeconds);

  /** Runs one command, then re-reads the detail. Returns false (and shows why) if it failed. */
  const run = useCallback(
    async (fn: () => Promise<unknown>, fallback: string): Promise<boolean> => {
      try {
        await fn();
        setError(null);
        await reload();
        return true;
      } catch (err: unknown) {
        setError(contextErrorText(err, fallback));
        void reload();
        return false;
      }
    },
    [reload],
  );

  const fallbackTitle =
    mode === "needs_you"
      ? `${name} needs you`
      : mode === "working"
        ? `${name} is working`
        : `${name} session`;

  return (
    <aside
      id="session-detail"
      className={`detail-panel${floating ? " is-floating" : " is-docked"}`}
      aria-label="Session details"
    >
      <header className="detail-head">
        <div className="detail-titles">
          {renaming ? (
            <TitleEditor
              initial={session.userTitle ?? ""}
              onCancel={() => setRenaming(false)}
              onSave={async (text) => {
                if (
                  await run(() => setSessionTitle(key, text), "The name wasn't saved. Try again.")
                ) {
                  setRenaming(false);
                  onChanged();
                }
              }}
            />
          ) : (
            <div className="detail-title-row">
              <h2 ref={headingRef} tabIndex={-1} className="detail-title">
                {sname.source === "tool" ? fallbackTitle : sname.primary}
              </h2>
              <button
                ref={renameBtn}
                type="button"
                className="head-btn"
                onClick={() => setRenaming(true)}
              >
                Rename
                <span className="visually-hidden"> session</span>
              </button>
            </div>
          )}
          <p className="detail-sub">
            <StatePill view={view} />
            <span className="session-id" title={session.providerSessionId}>
              <span className="visually-hidden">Session </span>
              {sname.secondary ?? shortId(session.providerSessionId)}
            </span>
            {sname.source === "agent" && (
              <span className="field-note">Title from the agent · captured with your OK</span>
            )}
            <span className="session-time">{relativeTime(session.lastReceivedUtc, now)}</span>
          </p>
        </div>
        <button
          type="button"
          className="head-btn pin-toggle"
          aria-pressed={pinned}
          title={
            pinned
              ? "Pinned to the side. In narrow windows the panel floats anyway."
              : "Floating over the list. Pin it to dock it at the side."
          }
          onClick={props.onTogglePin}
        >
          {pinned ? "Pinned" : "Pin"}
        </button>
        <button
          type="button"
          className="icon-btn"
          aria-label="Close details"
          onClick={props.onClose}
        >
          <Icon name="close" />
        </button>
      </header>

      <div className="detail-scroll">
        {why && <p className="detail-note">{why}</p>}
        {error && <Banner tone="danger">{error}</Banner>}

        {load.status === "loading" && (
          <p role="status" className="detail-note">
            Loading…
          </p>
        )}
        {load.status === "error" && (
          <Banner tone="danger">
            {detail
              ? "Couldn't refresh this session. Showing what was loaded before."
              : "Couldn't load this session's details. Nothing was changed."}{" "}
            <button type="button" className="link-btn" onClick={() => void reload()}>
              Try again
            </button>
          </Banner>
        )}

        {detail && (
          <Body
            {...props}
            detail={detail}
            mode={mode}
            name={name}
            run={run}
            onChanged={onChanged}
            onAskDelete={() => setConfirmDelete(true)}
            setError={setError}
          />
        )}
      </div>

      <Dialog
        open={confirmDelete}
        title="Delete this session?"
        busy={deleting}
        onClose={() => setConfirmDelete(false)}
        actions={
          <>
            <button
              type="button"
              className="btn btn-quiet"
              disabled={deleting}
              onClick={() => setConfirmDelete(false)}
            >
              Cancel
            </button>
            <button
              type="button"
              className="btn btn-danger"
              disabled={deleting}
              onClick={async () => {
                setDeleting(true);
                try {
                  await deleteSession(key);
                  setConfirmDelete(false);
                  props.onDeleted();
                } catch (err: unknown) {
                  setConfirmDelete(false);
                  setError(contextErrorText(err, "The session wasn't deleted. Try again."));
                } finally {
                  setDeleting(false);
                }
              }}
            >
              {deleting ? "Deleting…" : "Delete session"}
            </button>
          </>
        }
        description={
          <>
            <p>
              This removes {name} session {shortId(session.providerSessionId)} from Lullshift: its
              timeline, your notes, items and links, and any captured text.
            </p>
            <p>It can't be undone. Files and the agent itself aren't touched.</p>
          </>
        }
      />
    </aside>
  );
}

type Run = (fn: () => Promise<unknown>, fallback: string) => Promise<boolean>;

interface BodyProps extends Props {
  detail: SessionDetail;
  mode: ReturnType<typeof panelMode>;
  name: string;
  run: Run;
  onAskDelete: () => void;
  setError: (message: string | null) => void;
}

function Body(p: BodyProps) {
  const { session, detail, mode, name, run, now } = p;
  const key = viewKey(session);
  const ctx = detail.context;
  const card = session.needsReview;

  const items = {
    decision: itemsOfKind(detail.items, "decision"),
    checklist: itemsOfKind(detail.items, "checklist"),
    next_move: itemsOfKind(detail.items, "next_move"),
  };

  const list = (kind: ItemKind, title: string, emptyText: string, placeholder: string) => (
    <ItemList
      title={title}
      kind={kind}
      items={items[kind]}
      categories={kind === "next_move"}
      copy={kind === "next_move"}
      emptyText={emptyText}
      placeholder={placeholder}
      onAdd={(category, text) =>
        run(() => addSessionItem(key, kind, category, text), "That wasn't added. Try again.")
      }
      onUpdate={(id, change) =>
        run(() => updateSessionItem(id, change), "That change wasn't saved. Try again.")
      }
      onDelete={(id) => run(() => deleteSessionItem(id), "That wasn't deleted. Try again.")}
      onMove={(id, position) =>
        run(() => moveSessionItem(id, position), "That wasn't moved. Try again.")
      }
    />
  );

  // Captured intent is shown read-only; the editable box only ever holds the user's own text.
  const captured = ctx.intent?.source === "agent" ? ctx.intent : null;
  const own = ctx.intent?.source === "user" ? ctx.intent.text : "";
  const intent = (readLabel = "Intent") => (
    <div className="intent-block">
      {captured && (
        <div className="detail-section">
          <p className="field-label">
            {readLabel}
            <span className="field-note"> · captured with your OK</span>
          </p>
          <blockquote className="detail-quote">{captured.text}</blockquote>
          <div>
            <button
              type="button"
              className="btn-mini"
              onClick={() =>
                void run(
                  () => setSessionIntent(key, captured.text),
                  "Your intent wasn't saved. Try again.",
                )
              }
            >
              Copy into my intent
            </button>
          </div>
        </div>
      )}
      <SavedText
        label={captured ? "Your intent" : readLabel === "Intent" ? "Your intent" : readLabel}
        value={own}
        maxLength={2000}
        rows={3}
        placeholder="What are you trying to get done?"
        failText="Your intent wasn't saved."
        onSave={async (text) => {
          await setSessionIntent(key, text);
          await run(async () => undefined, "");
        }}
      />
    </div>
  );

  const notes = (
    <SavedText
      label="Notes"
      value={ctx.notes}
      maxLength={20000}
      rows={4}
      placeholder="Anything to remember when you come back."
      failText="Your note wasn't saved. Try again."
      onSave={async (text) => {
        await setSessionNotes(key, text);
        await run(async () => undefined, "");
      }}
    />
  );

  return (
    <>
      {ctx.hasCaptured && <CapturedRow name={name} run={run} sessionKey={key} />}

      {card && (
        <ReturnCard
          intentField={intent("You set out to")}
          decisions={list(
            "decision",
            "Open decisions",
            "No open decisions.",
            "Add a decision to make",
          )}
          checklist={list(
            "checklist",
            "Review checklist",
            "Nothing on the checklist yet.",
            "Add something to check",
          )}
          notesField={notes}
          checklistCopy={checklistText(detail.items)}
          hasChecklist={items.checklist.length > 0}
          onDone={() =>
            run(() => markReviewed(key), "Couldn't mark this as reviewed. Try again.").then(
              (ok) => ok && p.onChanged(),
            )
          }
        />
      )}

      {mode === "needs_you" && (
        <>
          <section className="detail-section" aria-labelledby="ask-heading">
            <h3 id="ask-heading" className="detail-heading">
              What it's asking
            </h3>
            {ctx.ask ? (
              <>
                <blockquote className="detail-quote">{ctx.ask.text}</blockquote>
                <p className="detail-source">
                  from {name} · {relativeTime(ctx.ask.capturedUtc, now)}
                </p>
              </>
            ) : (
              <p>{askFallback(detail.timeline)}</p>
            )}
            <p className="detail-note">
              Answer in your {name} window. Lullshift can't answer for you.
            </p>
          </section>
          {!card && (
            <>
              {intent()}
              {list(
                "checklist",
                "Before you answer",
                "Nothing on the checklist yet.",
                "Add something to check",
              )}
              {notes}
            </>
          )}
        </>
      )}

      {mode === "working" && (
        <>
          {!card && (
            <>
              {intent()}
              {notes}
            </>
          )}
          {list(
            "next_move",
            "Next Move",
            "No follow-ups yet.",
            "A question, criterion or test idea",
          )}
        </>
      )}

      {mode === "history" && (
        <>
          {!card && (
            <>
              {intent()}
              {notes}
            </>
          )}
          <section className="detail-section" aria-labelledby="items-heading">
            <h3 id="items-heading" className="detail-heading">
              Items
            </h3>
            <p>{itemsSummary(detail.items)}</p>
          </section>
          {!card && (
            <>
              {list("decision", "Open decisions", "No open decisions.", "Add a decision to make")}
              {list(
                "checklist",
                "Review checklist",
                "Nothing on the checklist yet.",
                "Add something to check",
              )}
            </>
          )}
          {list(
            "next_move",
            "Next Move",
            "No follow-ups yet.",
            "A question, criterion or test idea",
          )}
          <Timeline detail={detail} />
          <Links {...p} />
          <Manage {...p} />
        </>
      )}
    </>
  );
}

function CapturedRow({
  name,
  run,
  sessionKey,
}: {
  name: string;
  run: Run;
  sessionKey: SessionKey;
}) {
  return (
    <div className="captured-row">
      <p>Some text here was captured from {name} with your OK. It stays on this device.</p>
      <button
        type="button"
        className="btn-mini"
        onClick={() =>
          void run(
            () => clearCapturedContext(sessionKey),
            "Captured text wasn't cleared. Try again.",
          )
        }
      >
        Clear captured text
      </button>
    </div>
  );
}

function ReturnCard({
  intentField,
  decisions,
  checklist,
  notesField,
  checklistCopy,
  hasChecklist,
  onDone,
}: {
  intentField: ReactNode;
  decisions: ReactNode;
  checklist: ReactNode;
  notesField: ReactNode;
  checklistCopy: string;
  hasChecklist: boolean;
  onDone: () => Promise<unknown>;
}) {
  const [status, setStatus] = useState("");
  return (
    <section className="return-card" aria-labelledby="return-heading">
      <h3 id="return-heading" className="return-title">
        Welcome back
      </h3>
      <p>The session ended. Lullshift doesn't know whether the task is done; check the result.</p>
      {intentField}
      {decisions}
      {checklist}
      {notesField}
      <div className="actions">
        <button type="button" className="btn btn-primary btn-small" onClick={() => void onDone()}>
          Done reviewing
        </button>
        <button
          type="button"
          className="btn btn-secondary btn-small"
          disabled={!hasChecklist}
          onClick={async () => {
            try {
              await navigator.clipboard.writeText(checklistCopy);
              setStatus("Copied");
            } catch {
              setStatus("Couldn't copy. Select the text and copy it yourself.");
            }
          }}
        >
          Copy checklist
        </button>
      </div>
      <p className="copy-status" role="status">
        {status}
      </p>
    </section>
  );
}

function Timeline({ detail }: { detail: SessionDetail }) {
  const entries = detail.timeline;
  return (
    <section className="detail-section" aria-labelledby="timeline-heading">
      <h3 id="timeline-heading" className="detail-heading">
        Timeline
      </h3>
      {entries.length === 0 ? (
        <p className="detail-none">
          No events are kept for this session. They may be older than the retention period.
        </p>
      ) : (
        <ol className="timeline">
          {entries.map((e, i) => (
            <li key={`${e.timestampUtc}-${i}`}>
              <time dateTime={e.timestampUtc}>{localTime(e.timestampUtc)}</time>
              <span>{timelineLabel(e)}</span>
            </li>
          ))}
        </ol>
      )}
    </section>
  );
}

/** Inline rename: Enter saves, Esc cancels. An empty name removes the user's name. */
function TitleEditor({
  initial,
  onSave,
  onCancel,
}: {
  initial: string;
  onSave: (text: string) => Promise<void>;
  onCancel: () => void;
}) {
  const id = useId();
  const [text, setText] = useState(initial);
  const [saving, setSaving] = useState(false);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => {
    input.current?.focus();
    input.current?.select();
  }, []);
  const save = async () => {
    if (saving) return;
    setSaving(true);
    try {
      await onSave(text);
    } finally {
      setSaving(false);
    }
  };
  return (
    <div className="title-editor">
      <label htmlFor={id} className="visually-hidden">
        Session name
      </label>
      <input
        ref={input}
        id={id}
        className="field"
        type="text"
        maxLength={120}
        value={text}
        placeholder="Name this session"
        disabled={saving}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            void save();
          } else if (e.key === "Escape") {
            // Handled here, so Esc doesn't also close the panel.
            e.preventDefault();
            onCancel();
          }
        }}
      />
      <p className="field-note">
        Enter to save, Esc to cancel. Leave it empty to remove your name.
      </p>
      <span className="item-actions">
        <button type="button" className="btn-mini" disabled={saving} onClick={() => void save()}>
          Save
        </button>
        <button type="button" className="btn-mini" disabled={saving} onClick={onCancel}>
          Cancel
        </button>
      </span>
    </div>
  );
}

function Links({ session, sessions, detail, run, onOpenSession }: BodyProps) {
  const key = viewKey(session);
  const [pick, setPick] = useState("");
  const linked = new Set(detail.links.map(keyString));
  const options = sessions.filter(
    (s) => keyString(viewKey(s)) !== keyString(key) && !linked.has(keyString(viewKey(s))),
  );
  const label = (k: SessionKey) => {
    const found = sessions.find((s) => keyString(viewKey(s)) === keyString(k));
    const name = found ? sessionName(found) : null;
    return name?.secondary
      ? `${name.primary} (${name.secondary})`
      : `${providerName(k.provider)} · ${shortId(k.sessionId)}`;
  };
  const known = (k: SessionKey) => sessions.some((s) => keyString(viewKey(s)) === keyString(k));

  return (
    <section className="detail-section" aria-labelledby="links-heading">
      <h3 id="links-heading" className="detail-heading">
        Linked sessions
      </h3>
      {detail.links.length === 0 ? (
        <p className="detail-none">Not linked to another session.</p>
      ) : (
        <ul className="links">
          {detail.links.map((l) => (
            <li key={keyString(l)}>
              <span>{label(l)}</span>
              <span className="item-actions">
                {known(l) && (
                  <button type="button" className="btn-mini" onClick={() => onOpenSession(l)}>
                    Open
                  </button>
                )}
                <button
                  type="button"
                  className="btn-mini"
                  aria-label={`Unlink ${label(l)}`}
                  onClick={() =>
                    void run(() => unlinkSessions(key, l), "Couldn't unlink. Try again.")
                  }
                >
                  Unlink
                </button>
              </span>
            </li>
          ))}
        </ul>
      )}
      <div className="item-add">
        <select
          className="field item-add-text"
          aria-label="Link to another session"
          value={pick}
          onChange={(e) => setPick(e.target.value)}
        >
          <option value="">Choose a session…</option>
          {options.map((s) => (
            <option key={keyString(viewKey(s))} value={keyString(viewKey(s))}>
              {label(viewKey(s))} · {sessionStateView(s).label}
            </option>
          ))}
        </select>
        <button
          type="button"
          className="btn btn-secondary btn-small"
          disabled={!pick}
          onClick={async () => {
            const other = options.find((s) => keyString(viewKey(s)) === pick);
            if (!other) return;
            if (await run(() => linkSessions(key, viewKey(other)), "Couldn't link. Try again."))
              setPick("");
          }}
        >
          Link
        </button>
      </div>
    </section>
  );
}

function Manage({ session, detail, onAskDelete, onOpenSettings, setError }: BodyProps) {
  const key = viewKey(session);
  const [saved, setSaved] = useState<ExportFormat | null>(null);
  const [busy, setBusy] = useState(false);

  const doExport = async (format: ExportFormat) => {
    setBusy(true);
    setSaved(null);
    try {
      // The native save dialog is opened by Rust; the webview never sees a path.
      if ((await exportSession(key, format)) === "saved") setSaved(format);
      setError(null);
    } catch (err: unknown) {
      setError(contextErrorText(err, "Couldn't export. Nothing was saved. Try again."));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="detail-section" aria-labelledby="manage-heading">
      <h3 id="manage-heading" className="detail-heading">
        Export and delete
      </h3>
      <div className="actions">
        <button
          type="button"
          className="btn btn-secondary btn-small"
          disabled={busy}
          onClick={() => void doExport("markdown")}
        >
          Export Markdown
        </button>
        <button
          type="button"
          className="btn btn-secondary btn-small"
          disabled={busy}
          onClick={() => void doExport("json")}
        >
          Export JSON
        </button>
        <button type="button" className="btn btn-danger btn-small" onClick={onAskDelete}>
          Delete session
        </button>
      </div>
      <p className="copy-status" role="status">
        {saved ? "Saved" : ""}
      </p>
      <p className="detail-note">
        Events are kept {detail.retentionDays} days. Your notes stay until you delete them.{" "}
        <button type="button" className="link-btn" onClick={onOpenSettings}>
          Change in Settings
        </button>
      </p>
    </section>
  );
}
