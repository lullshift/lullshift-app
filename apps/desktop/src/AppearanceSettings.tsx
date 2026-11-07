import type { ContentWidth, Theme } from "./context/api";
import { themeOf } from "./theme";
import { updateUiPrefs, useUiPrefs } from "./uiPrefsStore";

const OPTIONS: { value: ContentWidth; label: string; desc: string }[] = [
  { value: "full", label: "Full width", desc: "Use the whole window." },
  { value: "comfortable", label: "Comfortable", desc: "Keep content to a narrower column." },
];

const THEME_OPTIONS: { value: Theme; label: string; desc: string }[] = [
  { value: "system", label: "System", desc: "Follow the light or dark setting of your computer." },
  { value: "light", label: "Light", desc: "Always light." },
  { value: "dark", label: "Dark", desc: "Always dark." },
];

/** Theme and content width, saved with the other interface preferences. */
export function AppearanceSettings() {
  const { contentWidth, theme } = useUiPrefs();
  const currentTheme = themeOf(theme);
  return (
    <>
      <h2 className="section-title">Appearance</h2>
      <div className="card rows">
        <fieldset className="row row-choice">
          <legend className="row-text">
            <span className="row-label">Theme</span>
            <span className="row-desc">Applies to the whole app and to activities you start.</span>
          </legend>
          <div className="choice-group">
            {THEME_OPTIONS.map((o) => (
              <label key={o.value} className="choice">
                <input
                  type="radio"
                  name="theme"
                  value={o.value}
                  checked={currentTheme === o.value}
                  onChange={() => void updateUiPrefs({ theme: o.value })}
                />
                <span className="choice-text">
                  <span className="choice-label">{o.label}</span>
                  <span className="choice-desc">{o.desc}</span>
                </span>
              </label>
            ))}
          </div>
        </fieldset>
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
