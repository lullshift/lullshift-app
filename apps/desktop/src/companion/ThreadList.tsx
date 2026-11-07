import { useId, useState } from "react";
import { ellipsize, relativeTime } from "../format";
import { Dialog } from "../ui/Dialog";
import { MenuButton } from "../ui/MenuButton";
import type { Thread } from "./api";
import { threadMark, useInflight, type ThreadMark } from "./inflight";

const MARK_TEXT: Record<Exclude<ThreadMark, null>, string> = {
  writing: "Writing…",
  approval: "Needs your approval",
  new: "New reply",
  failed: "Reply failed",
};

/** A status in words; the dot is only an extra cue and is hidden from screen readers. */
function ThreadMarkView({ mark }: { mark: ThreadMark }) {
  if (!mark) return null;
  return (
    <span className={`thread-mark thread-mark-${mark}`}>
      <span className="thread-dot" aria-hidden="true" />
      {MARK_TEXT[mark]}
      <span className="visually-hidden">. </span>
    </span>
  );
}

interface Props {
  threads: readonly Thread[];
  selectedId: number | null;
  now: number;
  onSelect: (id: number) => void;
  onCreate: () => void;
  onRename: (id: number, title: string) => void;
  onDelete: (id: number) => void;
}

const TITLE_MAX = 120;

export const fullTitle = (t: Thread) => t.title.trim() || "Untitled conversation";
export const threadLabel = (t: Thread) => ellipsize(fullTitle(t), 60);

/**
 * Thread list with new, and rename and delete (with a confirm) in each row's ⋮ menu. Titles are
 * plain text, cut with an ellipsis; the full title shows on hover and is the button's name.
 */
export function ThreadList({
  threads,
  selectedId,
  now,
  onSelect,
  onCreate,
  onRename,
  onDelete,
}: Props) {
  const inputId = useId();
  const inflight = useInflight();
  const [renaming, setRenaming] = useState<Thread | null>(null);
  const [draft, setDraft] = useState("");
  const [deleting, setDeleting] = useState<Thread | null>(null);

  const startRename = (t: Thread) => {
    setDraft(t.title);
    setRenaming(t);
  };

  const submitRename = () => {
    const title = draft.trim();
    if (renaming && title) onRename(renaming.id, title.slice(0, TITLE_MAX));
    setRenaming(null);
  };

  return (
    <nav className="card thread-list" aria-label="Conversations">
      <button type="button" className="btn btn-primary" onClick={onCreate}>
        New conversation
      </button>
      {threads.length === 0 ? (
        <p className="card-sub">No conversations yet.</p>
      ) : (
        <ul className="thread-items">
          {threads.map((t) => (
            <li
              key={t.id}
              className={t.id === selectedId ? "thread-item thread-item-selected" : "thread-item"}
            >
              <button
                type="button"
                className="thread-select"
                aria-current={t.id === selectedId ? "true" : undefined}
                title={fullTitle(t)}
                onClick={() => onSelect(t.id)}
              >
                <span className="thread-title">{fullTitle(t)}</span>
                <span className="thread-meta">
                  <ThreadMarkView mark={threadMark(inflight.get(t.id))} />
                  {relativeTime(t.updatedAt, now)}
                </span>
              </button>
              <MenuButton
                label={`More actions for ${threadLabel(t)}`}
                className="thread-menu"
                items={[
                  { label: "Rename", onSelect: () => startRename(t) },
                  { label: "Delete", onSelect: () => setDeleting(t), danger: true },
                ]}
              />
            </li>
          ))}
        </ul>
      )}

      <Dialog
        open={renaming !== null}
        title="Rename conversation"
        onClose={() => setRenaming(null)}
        actions={
          <>
            <button type="button" className="btn btn-quiet" onClick={() => setRenaming(null)}>
              Cancel
            </button>
            <button
              type="button"
              className="btn btn-primary"
              disabled={draft.trim().length === 0}
              onClick={submitRename}
            >
              Save name
            </button>
          </>
        }
      >
        <label className="field-label" htmlFor={inputId}>
          Name
        </label>
        <input
          id={inputId}
          className="field"
          type="text"
          autoFocus
          maxLength={TITLE_MAX}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              if (draft.trim()) submitRename();
            }
          }}
        />
      </Dialog>

      <Dialog
        open={deleting !== null}
        title="Delete this conversation?"
        onClose={() => setDeleting(null)}
        actions={
          <>
            <button
              type="button"
              className="btn btn-quiet"
              autoFocus
              onClick={() => setDeleting(null)}
            >
              Keep it
            </button>
            <button
              type="button"
              className="btn btn-danger"
              onClick={() => {
                if (deleting) onDelete(deleting.id);
                setDeleting(null);
              }}
            >
              Delete
            </button>
          </>
        }
        description={
          <p>
            The messages, and the context saved with them, are removed from this computer. This
            can't be undone. Text already sent to a provider stays with that provider.
          </p>
        }
      />
    </nav>
  );
}
