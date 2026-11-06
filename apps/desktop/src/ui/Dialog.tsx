import { useEffect, useId, useRef, type ReactNode } from "react";

interface Props {
  open: boolean;
  title: string;
  onClose: () => void;
  /**
   * The explanation of what the dialog asks (what will change, what is kept). Rendered first and
   * linked with aria-describedby so a screen reader reads it after the title. Required for every
   * consent or confirmation dialog.
   */
  description?: ReactNode;
  children?: ReactNode;
  /** Buttons for the dialog footer; the first focusable element gets focus on open. */
  actions: ReactNode;
  /** Prevent closing (Esc or backdrop) while work is in progress. */
  busy?: boolean;
}

/**
 * Modal dialog on the native <dialog> element: the browser traps focus, Esc cancels, and focus
 * returns to the opener when it closes.
 */
export function Dialog({ open, title, onClose, description, children, actions, busy }: Props) {
  const ref = useRef<HTMLDialogElement>(null);
  const titleId = useId();
  const descId = useId();

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
      aria-describedby={description ? descId : undefined}
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
        {description && (
          <div id={descId} className="dialog-description">
            {description}
          </div>
        )}
        {children}
      </div>
      <div className="dialog-actions">{actions}</div>
    </dialog>
  );
}
