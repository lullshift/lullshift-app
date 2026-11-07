import { useEffect, useId, useRef, useState, type KeyboardEvent } from "react";

export interface MenuItem {
  label: string;
  onSelect: () => void;
  danger?: boolean;
}

interface Props {
  /** Accessible name of the button, e.g. "More actions for Plans". */
  label: string;
  items: readonly MenuItem[];
  className?: string;
}

/**
 * A menu button (WAI-ARIA menu button pattern) showing a vertical ⋮. Enter, Space or ArrowDown
 * opens it on the first item, ArrowUp on the last; arrows move with wrap-around, Home and End
 * jump, Escape and Tab close it and Escape returns focus to the button. A click outside closes
 * it. Choosing an item closes the menu, returns focus to the button, then runs the item.
 */
export function MenuButton({ label, items, className }: Props) {
  const id = useId();
  const [open, setOpen] = useState(false);
  const [focusAt, setFocusAt] = useState(0);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLUListElement>(null);
  const itemRefs = useRef<(HTMLButtonElement | null)[]>([]);

  useEffect(() => {
    if (open) itemRefs.current[focusAt]?.focus();
  }, [open, focusAt]);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      const t = e.target as Node;
      if (!menuRef.current?.contains(t) && !buttonRef.current?.contains(t)) setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [open]);

  const openAt = (i: number) => {
    setFocusAt(i);
    setOpen(true);
  };

  const close = (refocus: boolean) => {
    setOpen(false);
    if (refocus) buttonRef.current?.focus();
  };

  const onButtonKey = (e: KeyboardEvent<HTMLButtonElement>) => {
    if (e.key === "ArrowDown" || e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      openAt(0);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      openAt(items.length - 1);
    }
  };

  const onMenuKey = (e: KeyboardEvent<HTMLUListElement>) => {
    const n = items.length;
    const moves: Record<string, () => void> = {
      ArrowDown: () => setFocusAt((i) => (i + 1) % n),
      ArrowUp: () => setFocusAt((i) => (i - 1 + n) % n),
      Home: () => setFocusAt(0),
      End: () => setFocusAt(n - 1),
      Escape: () => close(true),
    };
    if (e.key === "Tab") {
      close(false);
      return;
    }
    const run = moves[e.key];
    if (run) {
      e.preventDefault();
      e.stopPropagation();
      run();
    }
  };

  return (
    <span className={`menu-wrap ${className ?? ""}`}>
      <button
        ref={buttonRef}
        type="button"
        className="menu-button"
        aria-label={label}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? `${id}-menu` : undefined}
        onClick={() => (open ? close(false) : openAt(0))}
        onKeyDown={onButtonKey}
      >
        <span aria-hidden="true">⋮</span>
      </button>
      {open && (
        <ul
          ref={menuRef}
          id={`${id}-menu`}
          className="menu"
          role="menu"
          aria-label={label}
          onKeyDown={onMenuKey}
        >
          {items.map((item, i) => (
            <li key={item.label} role="none">
              <button
                ref={(el) => {
                  itemRefs.current[i] = el;
                }}
                type="button"
                role="menuitem"
                tabIndex={i === focusAt ? 0 : -1}
                className={item.danger ? "menu-item menu-item-danger" : "menu-item"}
                onClick={() => {
                  close(true);
                  item.onSelect();
                }}
              >
                {item.label}
              </button>
            </li>
          ))}
        </ul>
      )}
    </span>
  );
}
