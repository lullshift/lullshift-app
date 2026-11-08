import { useCallback, useEffect, useId, useMemo, useState, type RefObject } from "react";
import {
  deleteSessionItem,
  moveSessionItem,
  setSessionIntent,
  setSessionNotes,
  updateSessionItem,
  type SessionKey,
} from "../context/api";
import { ItemRows } from "../context/ItemRows";
import { SavedText } from "../context/SavedText";
import { contextErrorText } from "../context/useSessionDetail";
import { relativeTime } from "../format";
import type { SessionView } from "../sessions";
import { Banner } from "../ui/Banner";
import { Icon } from "../ui/Icon";
import { Mascot } from "../ui/Mascot";
import { StatePill } from "../ui/StatePill";
import {
  KIND_FILTERS,
  STATE_FILTERS,
  buildGroups,
  filterGroups,
  groupId,
  resultsLine,
  type KindFilter,
  type NotesGroup,
  type StateFilter,
  type VisibleGroup,
} from "./notesLogic";
import { overviewOf, type NotesLoad } from "./useNotesOverview";

interface Props {
  headingRef: RefObject<HTMLHeadingElement | null>;
  load: NotesLoad;
  reload: () => Promise<void>;
  sessions: readonly SessionView[];
  now: number;
  /** Opens Sessions with this session's panel (the same flow as Home's "Go to session"). */
  onOpenSession: (key: SessionKey) => void;
}

/**
 * Notes (ADR-026 section 7): everything the user wrote across sessions, grouped by session,
 * searchable and filterable. Rust stays authoritative; edits go through the same commands as the
 * session panel and the view re-reads afterwards.
 */
export default function NotesView({
  headingRef,
  load,
  reload,
  sessions,
  now,
  onOpenSession,
}: Props) {
  const [text, setText] = useState("");
  const [kind, setKind] = useState<KindFilter>("all");
  const [state, setState] = useState<StateFilter>("all");
  const [error, setError] = useState<string | null>(null);
  const [typing, setTyping] = useState<{ key: string; field: "intent" | "notes" } | null>(null);
  const searchId = useId();

  const overview = overviewOf(load);
  const groups = useMemo(
    () => (overview ? buildGroups(overview, sessions) : []),
    [overview, sessions],
  );
  const visible = useMemo(
    () => filterGroups(groups, { text, kind, state }, typing),
    [groups, text, kind, state, typing],
  );
  const filtered = text.trim() !== "" || kind !== "all" || state !== "all";
  const line = resultsLine(visible.length, groups.length, filtered);
  // The live region is updated after typing pauses, so a screen reader is not interrupted per key.
  const [announced, setAnnounced] = useState(line);
  useEffect(() => {
    const t = window.setTimeout(() => setAnnounced(line), 600);
    return () => window.clearTimeout(t);
  }, [line]);

  /** Runs one command, then re-reads. Returns false (and says why) if it failed. */
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

  const clearFilters = () => {
    setText("");
    setKind("all");
    setState("all");
  };

  return (
    <section aria-labelledby="notes-heading" className="view notes">
      <header className="view-header">
        <h1 id="notes-heading" ref={headingRef} tabIndex={-1}>
          Notes
        </h1>
        <p className="view-lede">What you noted across your sessions, in one place.</p>
      </header>

      {load.status === "loading" && (
        <div className="skeletons" role="status">
          <span className="visually-hidden">Loading your notes…</span>
          <div className="skeleton" />
        </div>
      )}
      {load.status === "error" && (
        <Banner tone="danger">
          {overview
            ? "Couldn't refresh your notes. Showing what was loaded before."
            : "Couldn't read your notes. Nothing was changed."}{" "}
          <button type="button" className="link-btn" onClick={() => void reload()}>
            Try again
          </button>
        </Banner>
      )}
      {error && <Banner tone="danger">{error}</Banner>}
      {overview?.truncated && (
        <Banner tone="info">
          Showing your {overview.sessions.length} most recent sessions with notes. Older notes are
          kept in each session.
        </Banner>
      )}

      {overview && groups.length === 0 && (
        <div className="empty notes-empty">
          <Mascot face="asleep" size={84} />
          <h2>Nothing noted yet</h2>
          <p>
            Notes come from the session panel: open a session in Sessions and write your intent, a
            checklist to read before you answer, or a free note. They collect here.
          </p>
        </div>
      )}

      {groups.length > 0 && (
        <>
          <div className="notes-tools">
            <div className="notes-search">
              <Icon name="search" size={18} />
              <input
                id={searchId}
                type="search"
                className="board-search"
                placeholder="Search your notes…"
                aria-label="Search notes, items, intents, session names and tools"
                value={text}
                onChange={(e) => setText(e.target.value)}
              />
            </div>
            <div className="chips" role="group" aria-label="Show">
              {KIND_FILTERS.map((f) => (
                <button
                  key={f.id}
                  type="button"
                  className="chip"
                  aria-pressed={kind === f.id}
                  onClick={() => setKind(f.id)}
                >
                  {f.label}
                </button>
              ))}
            </div>
            <div className="chips" role="group" aria-label="Session state">
              {STATE_FILTERS.map((f) => (
                <button
                  key={f.id}
                  type="button"
                  className="chip"
                  aria-pressed={state === f.id}
                  onClick={() => setState(f.id)}
                >
                  {f.label}
                </button>
              ))}
            </div>
            <p className="notes-count" role="status">
              {announced}
            </p>
          </div>

          {visible.length === 0 ? (
            <div className="empty empty-small notes-empty">
              <Mascot face="watching" size={64} />
              <h2>No matches</h2>
              <p>Nothing you noted fits that search. Try other words or clear the filters.</p>
              <button type="button" className="btn btn-secondary" onClick={clearFilters}>
                Clear search and filters
              </button>
            </div>
          ) : (
            <ul className="notes-groups">
              {visible.map((v) => (
                <li key={groupId(v.group)}>
                  <Group
                    v={v}
                    now={now}
                    searchId={searchId}
                    run={run}
                    onOpenSession={onOpenSession}
                    onTyping={setTyping}
                  />
                </li>
              ))}
            </ul>
          )}
        </>
      )}
    </section>
  );
}

function Group({
  v,
  now,
  run,
  onOpenSession,
  onTyping,
  searchId,
}: {
  searchId: string;
  v: VisibleGroup;
  now: number;
  run: (fn: () => Promise<unknown>, fallback: string) => Promise<boolean>;
  onOpenSession: (key: SessionKey) => void;
  onTyping: (t: { key: string; field: "intent" | "notes" } | null) => void;
}) {
  const { group, showIntent, showNotes, itemIds } = v;
  const { entry, key } = group;
  const headingId = useId();
  const items = entry.checklist.filter((i) => itemIds.includes(i.id));
  return (
    <article className="notes-group card" aria-labelledby={headingId}>
      <GroupHead group={group} headingId={headingId} now={now} onOpenSession={onOpenSession} />

      {showIntent && (
        <div
          onFocus={() => onTyping({ key: groupId(group), field: "intent" })}
          onBlur={(e) => {
            // Moving between this box's own controls is not leaving it.
            if (!e.currentTarget.contains(e.relatedTarget as Node | null)) onTyping(null);
          }}
        >
          <SavedText
            label="Your intent"
            value={entry.intent ?? ""}
            maxLength={2000}
            rows={2}
            placeholder="What are you trying to get done?"
            failText="Your intent wasn't saved."
            onSave={async (t) => {
              await setSessionIntent(key, t);
              await run(async () => undefined, "");
            }}
          />
        </div>
      )}

      {items.length > 0 && (
        <div className="notes-items">
          <h3 className="detail-heading">Before you answer</h3>
          <ItemRows
            kind="checklist"
            items={items}
            focusFallback={() => document.getElementById(searchId)?.focus()}
            onUpdate={(id, change) =>
              run(() => updateSessionItem(id, change), "That change wasn't saved. Try again.")
            }
            onDelete={(id) => run(() => deleteSessionItem(id), "That wasn't deleted. Try again.")}
            onMove={(id, position) =>
              run(() => moveSessionItem(id, position), "That wasn't moved. Try again.")
            }
          />
        </div>
      )}

      {showNotes && (
        <div
          onFocus={() => onTyping({ key: groupId(group), field: "notes" })}
          onBlur={(e) => {
            // Moving between this box's own controls is not leaving it.
            if (!e.currentTarget.contains(e.relatedTarget as Node | null)) onTyping(null);
          }}
        >
          <SavedText
            label="Notes"
            value={entry.notes}
            maxLength={20000}
            rows={3}
            placeholder="Anything to remember when you come back."
            failText="Your note wasn't saved. Try again."
            onSave={async (t) => {
              await setSessionNotes(key, t);
              await run(async () => undefined, "");
            }}
          />
        </div>
      )}
    </article>
  );
}

function GroupHead({
  group,
  headingId,
  now,
  onOpenSession,
}: {
  group: NotesGroup;
  headingId: string;
  now: number;
  onOpenSession: (key: SessionKey) => void;
}) {
  const listed = group.session !== null;
  return (
    <header className="notes-group-head">
      <div className="notes-group-titles">
        <h2 id={headingId} className="notes-group-title">
          {group.title}
        </h2>
        <p className="notes-group-sub">
          <StatePill view={group.state} />
          <span className="session-id">{group.tool}</span>
          <span className="session-time">{relativeTime(group.entry.lastReceivedUtc, now)}</span>
          {!listed && (
            <span className="field-note">
              Not in the current session list, so its state is unknown
            </span>
          )}
        </p>
      </div>
      <button
        type="button"
        className="btn btn-secondary btn-small"
        disabled={!listed}
        aria-label={`Open session: ${group.title}`}
        onClick={() => onOpenSession(group.key)}
      >
        Open session
      </button>
    </header>
  );
}
