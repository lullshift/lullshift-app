import { PRIVATE_HOST_LABEL, PRIVATE_HOST_NOTE } from "./logic";

/** The explicit "on my own computer or network" confirmation (decision 17). Never pre-ticked. */
export function PrivateHostBox({
  id,
  checked,
  disabled,
  onChange,
}: {
  id: string;
  checked: boolean;
  disabled: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <div className="mk-private">
      <label className="mk-check" htmlFor={id}>
        <input
          id={id}
          type="checkbox"
          checked={checked}
          disabled={disabled}
          aria-describedby={`${id}-note`}
          onChange={(e) => onChange(e.target.checked)}
        />
        <span>{PRIVATE_HOST_LABEL}</span>
      </label>
      <p id={`${id}-note`} className="field-note">
        {PRIVATE_HOST_NOTE}
      </p>
    </div>
  );
}
