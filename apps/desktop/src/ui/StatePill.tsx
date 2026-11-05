import type { StateView } from "../sessions";

/** Session state as a dot plus a text label; the colour is never the only signal. */
export function StatePill({ view }: { view: StateView }) {
  return (
    <span className={`pill tone-${view.tone}`}>
      <span className="pill-dot" aria-hidden="true" />
      {view.label}
    </span>
  );
}
