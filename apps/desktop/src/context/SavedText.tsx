import { useCallback, useEffect, useId, useRef, useState } from "react";
import { Banner } from "../ui/Banner";

const DEBOUNCE_MS = 800;

interface Props {
  label: string;
  /** Small note beside the label, such as where the text came from. */
  labelNote?: string;
  /** The saved value from Rust. */
  value: string;
  maxLength: number;
  rows?: number;
  placeholder?: string;
  /** Shown if saving fails. */
  failText: string;
  onSave: (text: string) => Promise<void>;
}

/**
 * A text box that saves when it loses focus and shortly after typing stops, and once more when
 * it closes. A failed save keeps the typed text and says so; the next blur or "Try again" retries.
 */
export function SavedText({
  label,
  labelNote,
  value,
  maxLength,
  rows = 3,
  placeholder,
  failText,
  onSave,
}: Props) {
  const id = useId();
  const [draft, setDraft] = useState(value);
  const [failed, setFailed] = useState(false);
  const draftRef = useRef(value);
  const savedRef = useRef(value);
  const timer = useRef<number | undefined>(undefined);
  const onSaveRef = useRef(onSave);
  useEffect(() => {
    onSaveRef.current = onSave;
  });

  // Take a new saved value from Rust unless the user has unsaved typing.
  useEffect(() => {
    if (draftRef.current === savedRef.current) {
      draftRef.current = value;
      setDraft(value);
    }
    savedRef.current = value;
  }, [value]);

  const flush = useCallback(async () => {
    window.clearTimeout(timer.current);
    const text = draftRef.current;
    if (text === savedRef.current) return;
    try {
      await onSaveRef.current(text);
      savedRef.current = text;
      setFailed(false);
    } catch {
      setFailed(true);
    }
  }, []);

  // Save pending typing when the panel closes or switches session.
  useEffect(
    () => () => {
      void flush();
    },
    [flush],
  );

  return (
    <div className="saved-text">
      <label htmlFor={id} className="field-label">
        {label}
        {labelNote && <span className="field-note"> · {labelNote}</span>}
      </label>
      <textarea
        id={id}
        className="field textarea"
        rows={rows}
        maxLength={maxLength}
        placeholder={placeholder}
        value={draft}
        onChange={(e) => {
          draftRef.current = e.target.value;
          setDraft(e.target.value);
          window.clearTimeout(timer.current);
          timer.current = window.setTimeout(() => void flush(), DEBOUNCE_MS);
        }}
        onBlur={() => void flush()}
      />
      {failed && (
        <Banner tone="danger">
          {failText}{" "}
          <button type="button" className="link-btn" onClick={() => void flush()}>
            Try again
          </button>
        </Banner>
      )}
    </div>
  );
}
