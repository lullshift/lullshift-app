import { useId, useState } from "react";
import { ellipsize, relativeTime } from "../format";
import { Dialog } from "../ui/Dialog";
import type { Thread } from "./api";

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

export const threadLabel = (t: Thread) => ellipsize(t.title.trim() || "Untitled conversation", 60);

/** Thread list with new, rename and delete (with a confirm). Titles are plain text. */
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
            <li key={t.id} className="thread-item">
              <button
                type="button"
                className="thread-select"
                aria-current={t.id === selectedId ? "true" : undefined}
                onClick={() => onSelect(t.id)}
              >
                <span className="thread-title">{threadLabel(t)}</span>
                <span className="thread-meta">{relativeTime(t.updatedAt, now)}</span>
              </button>
              <button
                type="button"
                className="btn-mini"
                aria-label={`Rename ${threadLabel(t)}`}
                onClick={() => startRename(t)}
              >
                Rename
              </button>
              <button
                type="button"
                className="btn-mini"
                aria-label={`Delete ${threadLabel(t)}`}
                onClick={() => setDeleting(t)}
              >
                Delete
              </button>
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
