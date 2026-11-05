import { useEffect, useId, useRef, useState, type FormEvent } from "react";
import type { ItemKind, NextMoveCategory, SessionItem } from "./api";
import { CATEGORIES, categoryLabel } from "./panelLogic";

interface Props {
  title: string;
  kind: ItemKind;
  items: SessionItem[];
  /** Next Move items carry a category. */
  categories?: boolean;
  /** Next Move items can be copied to the clipboard by the user. */
  copy?: boolean;
  emptyText: string;
  placeholder: string;
  /** All callbacks resolve true when the command worked. */
  onAdd: (category: NextMoveCategory | null, text: string) => Promise<boolean>;
  onUpdate: (id: number, change: { text?: string; done?: boolean }) => Promise<boolean>;
  onDelete: (id: number) => Promise<boolean>;
  onMove: (id: number, position: number) => Promise<boolean>;
}

/** An editable, reorderable list of session items, all keyboard accessible. */
export function ItemList({
  title,
  kind,
  items,
  categories,
  copy,
  emptyText,
  placeholder,
  onAdd,
  onUpdate,
  onDelete,
  onMove,
}: Props) {
  const headingId = useId();
  const [text, setText] = useState("");
  const [category, setCategory] = useState<NextMoveCategory>("question");
  const [editing, setEditing] = useState<number | null>(null);
  const [editText, setEditText] = useState("");
  const [status, setStatus] = useState("");
  const refocus = useRef<{ id: number; dir: "up" | "down" } | null>(null);
  const listRef = useRef<HTMLUListElement>(null);

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

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    const t = text.trim();
    if (!t) return;
    if (await onAdd(categories ? category : null, t)) setText("");
  };

  const copyItem = async (item: SessionItem) => {
    try {
      await navigator.clipboard.writeText(item.text);
      setStatus("Copied");
    } catch {
      setStatus("Couldn't copy. Select the text and copy it yourself.");
    }
    window.setTimeout(() => setStatus(""), 2500);
  };

  const saveEdit = async (item: SessionItem) => {
    const t = editText.trim();
    if (!t || t === item.text) return setEditing(null);
    if (await onUpdate(item.id, { text: t })) setEditing(null);
  };

  return (
    <section className="detail-section" aria-labelledby={headingId}>
      <h3 id={headingId} className="detail-heading">
        {title}
      </h3>
      {items.length === 0 ? (
        <p className="detail-none">{emptyText}</p>
      ) : (
        <ul className="items" ref={listRef}>
          {items.map((item, i) => (
            <li key={item.id} className="item" data-item-id={item.id}>
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
                    onChange={(e) => setEditText(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") {
                        e.preventDefault();
                        void saveEdit(item);
                      } else if (e.key === "Escape") {
                        // Cancels the edit only; the panel stays open.
                        e.preventDefault();
                        e.stopPropagation();
                        setEditing(null);
                      }
                    }}
                  />
                ) : (
                  <span className={`item-text${item.done ? " is-done" : ""}`}>{item.text}</span>
                )}
              </div>
              <div className="item-actions">
                {editing === item.id ? (
                  <>
                    <button type="button" className="btn-mini" onClick={() => void saveEdit(item)}>
                      Save
                    </button>
                    <button type="button" className="btn-mini" onClick={() => setEditing(null)}>
                      Cancel
                    </button>
                  </>
                ) : (
                  <>
                    <button
                      type="button"
                      className="btn-mini"
                      aria-label={`Edit: ${item.text}`}
                      onClick={() => {
                        setEditing(item.id);
                        setEditText(item.text);
                      }}
                    >
                      Edit
                    </button>
                    <button
                      type="button"
                      className="btn-mini"
                      data-move="up"
                      aria-label={`Move up: ${item.text}`}
                      disabled={i === 0}
                      onClick={() => {
                        // Set before the move so the effect after the reload can refocus.
                        refocus.current = { id: item.id, dir: "up" };
                        void onMove(item.id, items[i - 1]!.position);
                      }}
                    >
                      <span aria-hidden="true">↑</span>
                    </button>
                    <button
                      type="button"
                      className="btn-mini"
                      data-move="down"
                      aria-label={`Move down: ${item.text}`}
                      disabled={i === items.length - 1}
                      onClick={() => {
                        // Set before the move so the effect after the reload can refocus.
                        refocus.current = { id: item.id, dir: "down" };
                        void onMove(item.id, items[i + 1]!.position);
                      }}
                    >
                      <span aria-hidden="true">↓</span>
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
                      className="btn-mini"
                      aria-label={`Delete: ${item.text}`}
                      onClick={() => void onDelete(item.id)}
                    >
                      Delete
                    </button>
                  </>
                )}
              </div>
            </li>
          ))}
        </ul>
      )}
      {copy && (
        <>
          <p className="detail-note">
            Copy puts the text on your clipboard. Paste it into Claude Code yourself; nothing is
            sent to the agent.
          </p>
          <p className="copy-status" role="status">
            {status}
          </p>
        </>
      )}
      <form className="item-add" onSubmit={(e) => void submit(e)}>
        {categories && (
          <select
            className="field"
            aria-label="Category"
            value={category}
            onChange={(e) => setCategory(e.target.value as NextMoveCategory)}
          >
            {CATEGORIES.map((c) => (
              <option key={c.id} value={c.id}>
                {c.label}
              </option>
            ))}
          </select>
        )}
        <input
          type="text"
          className="field item-add-text"
          aria-label={`Add to ${title}`}
          placeholder={placeholder}
          maxLength={2000}
          value={text}
          onChange={(e) => setText(e.target.value)}
        />
        <button type="submit" className="btn btn-secondary btn-small" disabled={!text.trim()}>
          Add
        </button>
      </form>
    </section>
  );
}
