import { useId, useState, type FormEvent } from "react";
import type { ItemKind, NextMoveCategory, SessionItem } from "./api";
import { ItemRows } from "./ItemRows";
import { CATEGORIES } from "./panelLogic";

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
  const [status, setStatus] = useState("");

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    const t = text.trim();
    if (!t) return;
    if (await onAdd(categories ? category : null, t)) setText("");
  };

  return (
    <section className="detail-section" aria-labelledby={headingId}>
      <h3 id={headingId} className="detail-heading">
        {title}
      </h3>
      {items.length === 0 ? (
        <p className="detail-none">{emptyText}</p>
      ) : (
        <ItemRows
          kind={kind}
          items={items}
          categories={categories}
          copy={copy}
          onStatus={setStatus}
          onUpdate={onUpdate}
          onDelete={onDelete}
          onMove={onMove}
        />
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
