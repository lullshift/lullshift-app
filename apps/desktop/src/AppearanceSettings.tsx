import type { ContentWidth } from "./context/api";
import { updateUiPrefs, useUiPrefs } from "./uiPrefsStore";

const OPTIONS: { value: ContentWidth; label: string; desc: string }[] = [
  { value: "full", label: "Full width", desc: "Use the whole window." },
  { value: "comfortable", label: "Comfortable", desc: "Keep content to a narrower column." },
];

/** Content width, saved with the other interface preferences. */
export function AppearanceSettings() {
  const { contentWidth } = useUiPrefs();
  return (
    <>
      <h2 className="section-title">Appearance</h2>
      <div className="card rows">
        <fieldset className="row row-choice">
          <legend className="row-text">
            <span className="row-label">Content width</span>
            <span className="row-desc">How wide Sessions, Activities and Settings may grow.</span>
          </legend>
          <div className="choice-group">
            {OPTIONS.map((o) => (
              <label key={o.value} className="choice">
                <input
                  type="radio"
                  name="content-width"
                  value={o.value}
                  checked={contentWidth === o.value}
                  onChange={() => void updateUiPrefs({ contentWidth: o.value })}
                />
                <span className="choice-text">
                  <span className="choice-label">{o.label}</span>
                  <span className="choice-desc">{o.desc}</span>
                </span>
              </label>
            ))}
          </div>
        </fieldset>
      </div>
    </>
  );
}
