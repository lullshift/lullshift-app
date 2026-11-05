import { useEffect, useId, useRef, type ReactNode } from "react";

interface Props {
  open: boolean;
  title: string;
  onClose: () => void;
  children: ReactNode;
  /** Buttons for the dialog footer; the first focusable element gets focus on open. */
  actions: ReactNode;
  /** Prevent closing (Esc or backdrop) while work is in progress. */
  busy?: boolean;
}

/**
 * Modal dialog on the native <dialog> element: the browser traps focus, Esc cancels, and focus
 * returns to the opener when it closes.
 */
export function Dialog({ open, title, onClose, children, actions, busy }: Props) {
  const ref = useRef<HTMLDialogElement>(null);
  const titleId = useId();

  useEffect(() => {
    const d = ref.current;
    if (!d) return;
    if (open && !d.open) d.showModal();
    if (!open && d.open) d.close();
  }, [open]);

  return (
    <dialog
      ref={ref}
      className="dialog"
      aria-labelledby={titleId}
      onCancel={(e) => {
        e.preventDefault();
        if (!busy) onClose();
      }}
      onClick={(e) => {
        // A click on the backdrop lands on the <dialog> element itself.
        if (e.target === e.currentTarget && !busy) onClose();
      }}
    >
      <div className="dialog-body">
        <h2 id={titleId} className="dialog-title">
          {title}
        </h2>
        {children}
      </div>
      <div className="dialog-actions">{actions}</div>
    </dialog>
  );
}
