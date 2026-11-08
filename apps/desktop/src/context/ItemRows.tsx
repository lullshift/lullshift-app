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
  /** Where focus goes when the last row is gone (for example the Add input). */
  focusFallback?: () => void;
  /** All callbacks resolve true when the command worked. */
  onUpdate: (id: number, change: { text?: string; done?: boolean }) => Promise<boolean>;
  onDelete: (id: number) => Promise<boolean>;
  onMove: (id: number, position: number) => Promise<boolean>;
}

const shorten = (text: string) => (text.length > 80 ? `${text.slice(0, 80)}…` : text);

/** Where focus goes when a row disappears: the next row, else the previous, else the fallback. */
function neighbour(items: readonly SessionItem[], id: number): number | null {
  const i = items.findIndex((x) => x.id === id);
  return items[i + 1]?.id ?? items[i - 1]?.id ?? null;
}

/**
 * The rows of an item list, shared by the session panel and the Notes view. The text is a
 * button: click, Enter, Space or F2 opens the editor in place (Tab only moves focus). In the
 * editor Enter or leaving saves, Esc cancels, and empty text counts as cancel. The checkbox only
 * toggles done, Alt+Up and Alt+Down reorder, and Delete asks first.
 */
export function ItemRows({
  items,
  categories,
  copy,
  onStatus,
  focusFallback,
  onUpdate,
  onDelete,
  onMove,
}: Props) {
  const [editing, setEditing] = useState<number | null>(null);
  const [editText, setEditText] = useState("");
  const [saving, setSaving] = useState(false);
  // Edits that could not be saved, kept per item so moving to another row loses nothing.
  const [unsaved, setUnsaved] = useState<Record<number, string>>({});
  const [confirming, setConfirming] = useState<SessionItem | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [deleteFailed, setDeleteFailed] = useState(false);
  const listRef = useRef<HTMLUListElement>(null);
  // Set when Enter or Esc ends an edit, so the blur that follows doesn't save a second time.
  const finishing = useRef(false);
  // After Enter or Esc, focus returns to the row (not the text, which would reopen the editor).
  const refocusRow = useRef<number | null>(null);
  const refocus = useRef<{ id: number; dir: "up" | "down" } | null>(null);
  // A row that is about to disappear (deleted, or ticked under a filter) and where focus goes.
  const vanish = useRef<{ from: number; to: number | null } | null>(null);

  const itemsRef = useRef(items);
  useEffect(() => {
    itemsRef.current = items;
  });
  const idKey = items.map((i) => i.id).join(",");

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
  }, [idKey]);

  // When the row that had focus is gone, focus the next one (after the dialog has closed).
  useEffect(() => {
    const v = vanish.current;
    if (!v || confirming !== null) return;
    if (items.some((i) => i.id === v.from)) return;
    vanish.current = null;
    const t = window.setTimeout(() => {
      const row =
        v.to === null
          ? null
          : listRef.current?.querySelector<HTMLElement>(`[data-item-id="${v.to}"]`);
      if (row) row.focus();
      else focusFallback?.();
    }, 0);
    return () => window.clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- keyed on the visible ids
  }, [idKey, confirming]);

  const startEdit = (item: SessionItem) => {
    if (editing === item.id) return;
    finishing.current = false;
    setEditText(unsaved[item.id] ?? item.text);
    setEditing(item.id);
  };

  const clearUnsaved = (id: number) =>
    setUnsaved((u) => {
      if (!(id in u)) return u;
      const rest = { ...u };
      delete rest[id];
      return rest;
    });

  /** Empty text, or text that did not change, is a cancel; it never deletes the item. */
  const saveEdit = async (item: SessionItem, how: "enter" | "blur") => {
    if (finishing.current || saving) return;
    finishing.current = true;
    if (how === "enter") refocusRow.current = item.id;
    const t = editText.trim();
    if (!t || t === item.text) {
      clearUnsaved(item.id);
      return setEditing(null);
    }
    setSaving(true);
    const ok = await onUpdate(item.id, { text: t }).finally(() => setSaving(false));
    if (ok) clearUnsaved(item.id);
    else setUnsaved((u) => ({ ...u, [item.id]: t }));
    setEditing(null);
  };

  const cancelEdit = (item: SessionItem) => {
    finishing.current = true;
    refocusRow.current = item.id;
    clearUnsaved(item.id);
    setEditing(null);
  };

  const move = (item: SessionItem, i: number, dir: "up" | "down") => {
    const target = items[dir === "up" ? i - 1 : i + 1];
    if (!target) return;
    // Set before the move so the effect after the reload can refocus.
    refocus.current = { id: item.id, dir };
    void onMove(item.id, target.position).then((ok) => {
      if (!ok) refocus.current = null;
    });
  };

  const rowKey = (e: KeyboardEvent<HTMLLIElement>, item: SessionItem, i: number) => {
    if (!e.altKey || (e.key !== "ArrowUp" && e.key !== "ArrowDown")) return;
    const t = e.target as HTMLElement;
    if (t.tagName === "INPUT" && (t as HTMLInputElement).type === "text") return;
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

  const closeDialog = () => {
    setConfirming(null);
    setDeleteFailed(false);
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
              onChange={(e) => {
                // If a filter will hide the row once it is ticked, focus moves on with it.
                vanish.current = { from: item.id, to: neighbour(items, item.id) };
                void onUpdate(item.id, { done: e.target.checked }).then(() =>
                  window.setTimeout(() => {
                    // Still listed after the reload: nothing disappeared, nothing to move.
                    if (itemsRef.current.some((x) => x.id === item.id)) vanish.current = null;
                  }, 0),
                );
              }}
            />
            <div className="item-main">
              {categories && item.category && (
                <span className="item-cat">{categoryLabel(item.category)}</span>
              )}
              {editing === item.id ? (
                <input
                  type="text"
                  className="field item-edit"
                  aria-label={`Edit: ${item.text}`}
                  maxLength={2000}
                  value={editText}
                  readOnly={saving}
                  ref={(el) => {
                    // Caret at the end, not select-all.
                    if (el && !el.dataset.opened) {
                      el.dataset.opened = "1";
                      el.focus();
                      el.setSelectionRange(el.value.length, el.value.length);
                    }
                  }}
                  onChange={(e) => setEditText(e.target.value)}
                  onBlur={() => void saveEdit(item, "blur")}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      e.preventDefault();
                      // Enter that confirms an IME composition is not a save.
                      if (e.nativeEvent.isComposing) return;
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
                <>
                  <button
                    type="button"
                    className={`item-text${item.done ? " is-done" : ""}`}
                    aria-label={`Edit: ${item.text}`}
                    title="Click, or press Enter or F2, to edit"
                    onClick={() => startEdit(item)}
                    onKeyDown={(e) => {
                      if (e.key === "F2") {
                        e.preventDefault();
                        startEdit(item);
                      }
                    }}
                  >
                    {unsaved[item.id] ?? item.text}
                  </button>
                  {item.id in unsaved && (
                    <span className="field-note item-unsaved">
                      Not saved yet. Open it to try again, or press Esc there to drop it.
                    </span>
                  )}
                </>
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
        onClose={closeDialog}
        description={
          confirming && (
            <>
              <p>
                “{shorten(confirming.text)}” will be removed from this session. It can't be undone.
              </p>
              {deleteFailed && (
                <p role="alert" className="dialog-error">
                  That wasn't deleted. Try again.
                </p>
              )}
            </>
          )
        }
        actions={
          <>
            <button
              type="button"
              className="btn btn-quiet"
              disabled={deleting}
              onClick={closeDialog}
            >
              Cancel
            </button>
            <button
              type="button"
              className="btn btn-danger"
              disabled={deleting}
              onClick={async () => {
                if (!confirming) return;
                // Recorded before the dialog closes: the row that gets focus next.
                vanish.current = { from: confirming.id, to: neighbour(items, confirming.id) };
                setDeleting(true);
                setDeleteFailed(false);
                const ok = await onDelete(confirming.id).finally(() => setDeleting(false));
                if (ok) closeDialog();
                else {
                  vanish.current = null;
                  setDeleteFailed(true);
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
