import { useId, type ReactNode } from "react";

interface Props {
  checked: boolean;
  disabled?: boolean;
  onChange: (on: boolean) => void;
  label: ReactNode;
  description?: ReactNode;
}

/** An on/off setting row: label and description on the left, switch on the right. */
export function Switch({ checked, disabled, onChange, label, description }: Props) {
  const labelId = useId();
  const descId = useId();
  return (
    <div className="row">
      <div className="row-text">
        <span id={labelId} className="row-label">
          {label}
        </span>
        {description && (
          <span id={descId} className="row-desc">
            {description}
          </span>
        )}
      </div>
      <button
        type="button"
        role="switch"
        className="switch"
        aria-checked={checked}
        aria-labelledby={labelId}
        aria-describedby={description ? descId : undefined}
        disabled={disabled}
        onClick={() => onChange(!checked)}
      >
        <span className="switch-thumb" aria-hidden="true" />
      </button>
    </div>
  );
}
