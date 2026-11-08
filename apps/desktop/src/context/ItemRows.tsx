import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { Dialog } from "../ui/Dialog";
import { Icon } from "../ui/Icon";
import type { ItemKind, SessionItem } from "./api";
import { categoryLabel } from "./panelLogic";

interface Props {
  kind: ItemKind;
  items: SessionItem[];
  /** Next Move items carry a category. */
  categories?: boolean;
  /** Next Move items can be copied to the clipboard by the user. */
  copy?: boolean;
  /** Tells the list what happened to a Copy click ("" clears it). */
  onStatus?: (message: string) => void;
  /** All callbacks resolve true when the command worked. */
  onUpdate: (id: number, change: { text?: string; done?: boolean }) => Promise<boolean>;
  onDelete: (id: number) => Promise<boolean>;
  onMove: (id: number, position: number) => Promise<boolean>;
}

const shorten = (text: string) => (text.length > 80 ? `${text.slice(0, 80)}…` : text);

/**
 * The rows of an item list, shared by the session panel and the Notes view. The text edits in
 * place (focus or click it; Enter or leaving saves, Esc cancels, empty text cancels), the
 * checkbox only toggles done, Alt+Up and Alt+Down reorder, and Delete asks first.
 */
export function ItemRows({
  kind,
  items,
  categories,
  copy,
  onStatus,
  onUpdate,
  onDelete,
  onMove,
}: Props) {
  const [editing, setEditing] = useState<number | null>(null);
  const [editText, setEditText] = useState("");
  const [confirming, setConfirming] = useState<SessionItem | null>(null);
  const [deleting, setDeleting] = useState(false);
  const listRef = useRef<HTMLUListElement>(null);
  // Set when Enter or Esc ends an edit, so the blur that follows doesn't save a second time.
  const finishing = useRef(false);
  // After Enter or Esc, focus returns to the row (not the text, which would reopen the editor).
  const refocusRow = useRef<number | null>(null);
  const refocus = useRef<{ id: number; dir: "up" | "down" } | null>(null);

  useEffect(() => {
    const id = refocusRow.current;
    if (id === null || editing !== null) return;
    refocusRow.current = null;
    listRef.current?.querySelector<HTMLElement>(`[data-item-id="${id}"]`)?.focus();
  }, [editing]);

  // After a move, keep focus on the same item's move button (or its twin if that one is now off).
  useEffect(() => {
    const want = refocus.current;
    if (!want) return;
    refocus.current = null;
    const row = listRef.current?.querySelector(`[data-item-id="${want.id}"]`);
    const same = row?.querySelector<HTMLButtonElement>(`[data-move="${want.dir}"]`);
    const other = row?.querySelector<HTMLButtonElement>(
      `[data-move="${want.dir === "up" ? "down" : "up"}"]`,
    );
    (same && !same.disabled ? same : other)?.focus();
  }, [items]);

  const startEdit = (item: SessionItem) => {
    if (editing === item.id) return;
    finishing.current = false;
    setEditText(item.text);
    setEditing(item.id);
  };

  /** Empty text, or text that did not change, is a cancel; it never deletes the item. */
  const saveEdit = async (item: SessionItem, how: "enter" | "blur") => {
    if (finishing.current) return;
    finishing.current = true;
    if (how === "enter") refocusRow.current = item.id;
    const t = editText.trim();
    if (!t || t === item.text) return setEditing(null);
    if (await onUpdate(item.id, { text: t })) setEditing(null);
    else finishing.current = false; // stay in the editor; the failure is shown by the owner
  };

  const cancelEdit = (item: SessionItem) => {
    finishing.current = true;
    refocusRow.current = item.id;
    setEditing(null);
  };

  const move = (item: SessionItem, i: number, dir: "up" | "down") => {
    const target = items[dir === "up" ? i - 1 : i + 1];
    if (!target) return;
    // Set before the move so the effect after the reload can refocus.
    refocus.current = { id: item.id, dir };
    void onMove(item.id, target.position);
  };

  const rowKey = (e: KeyboardEvent<HTMLLIElement>, item: SessionItem, i: number) => {
    if (!e.altKey || (e.key !== "ArrowUp" && e.key !== "ArrowDown")) return;
    if (
      (e.target as HTMLElement).tagName === "INPUT" &&
      (e.target as HTMLInputElement).type === "text"
    )
      return;
    e.preventDefault();
    move(item, i, e.key === "ArrowUp" ? "up" : "down");
  };

  const copyItem = async (item: SessionItem) => {
    try {
      await navigator.clipboard.writeText(item.text);
      onStatus?.("Copied");
    } catch {
      onStatus?.("Couldn't copy. Select the text and copy it yourself.");
    }
    window.setTimeout(() => onStatus?.(""), 2500);
  };

  return (
    <>
      <ul className="items" ref={listRef}>
        {items.map((item, i) => (
          <li
            key={item.id}
            className="item"
            data-item-id={item.id}
            tabIndex={-1}
            onKeyDown={(e) => rowKey(e, item, i)}
          >
            <input
              type="checkbox"
              className="item-check"
              checked={item.done}
              aria-label={`Done: ${item.text}`}
              onChange={(e) => void onUpdate(item.id, { done: e.target.checked })}
            />
            <div className="item-main">
              {categories && item.category && (
                <span className="item-cat">{categoryLabel(item.category)}</span>
              )}
              {editing === item.id ? (
                <input
                  type="text"
                  className="field item-edit"
                  aria-label={`Edit ${kind === "next_move" ? "item" : "text"}`}
                  maxLength={2000}
                  value={editText}
                  autoFocus
                  onFocus={(e) => e.currentTarget.select()}
                  onChange={(e) => setEditText(e.target.value)}
                  onBlur={() => void saveEdit(item, "blur")}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      e.preventDefault();
                      void saveEdit(item, "enter");
                    } else if (e.key === "Escape") {
                      // Cancels the edit only; the panel stays open.
                      e.preventDefault();
                      e.stopPropagation();
                      cancelEdit(item);
                    }
                  }}
                />
              ) : (
                <button
                  type="button"
                  className={`item-text${item.done ? " is-done" : ""}`}
                  title="Click to edit"
                  onFocus={() => startEdit(item)}
                  onClick={() => startEdit(item)}
                >
                  {item.text}
                </button>
              )}
            </div>
            <div className="item-actions">
              <button
                type="button"
                className="btn-mini btn-icon"
                data-move="up"
                aria-label={`Move up: ${item.text}`}
                aria-keyshortcuts="Alt+ArrowUp"
                title="Move up (Alt+Up)"
                disabled={i === 0}
                onClick={() => move(item, i, "up")}
              >
                <Icon name="up" size={16} />
              </button>
              <button
                type="button"
                className="btn-mini btn-icon"
                data-move="down"
                aria-label={`Move down: ${item.text}`}
                aria-keyshortcuts="Alt+ArrowDown"
                title="Move down (Alt+Down)"
                disabled={i === items.length - 1}
                onClick={() => move(item, i, "down")}
              >
                <Icon name="down" size={16} />
              </button>
              {copy && (
                <button
                  type="button"
                  className="btn-mini"
                  aria-label={`Copy: ${item.text}`}
                  onClick={() => void copyItem(item)}
                >
                  Copy
                </button>
              )}
              <button
                type="button"
                className="btn-mini btn-icon"
                aria-label={`Delete ${item.text}`}
                title="Delete"
                onClick={() => setConfirming(item)}
              >
                <Icon name="trash" size={16} />
              </button>
            </div>
          </li>
        ))}
      </ul>
      <Dialog
        open={confirming !== null}
        title="Delete this item?"
        busy={deleting}
        onClose={() => setConfirming(null)}
        description={
          confirming && (
            <p>
              “{shorten(confirming.text)}” will be removed from this session. It can't be undone.
            </p>
          )
        }
        actions={
          <>
            <button
              type="button"
              className="btn btn-quiet"
              disabled={deleting}
              onClick={() => setConfirming(null)}
            >
              Cancel
            </button>
            <button
              type="button"
              className="btn btn-danger"
              disabled={deleting}
              onClick={async () => {
                if (!confirming) return;
                setDeleting(true);
                try {
                  await onDelete(confirming.id);
                } finally {
                  setDeleting(false);
                  setConfirming(null);
                }
              }}
            >
              {deleting ? "Deleting…" : "Delete"}
            </button>
          </>
        }
      />
    </>
  );
}
