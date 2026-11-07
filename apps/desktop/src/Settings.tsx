import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { isPermissionGranted, requestPermission } from "@tauri-apps/plugin-notification";
import { useEffect, useId, useRef, useState, type RefObject } from "react";
import { AppearanceSettings } from "./AppearanceSettings";
import { CompanionSettings } from "./companion/CompanionSettings";
import { ContextSettings } from "./context/ContextSettings";
import { NotificationSettings } from "./NotificationSettings";
import { Banner } from "./ui/Banner";
import { Icon } from "./ui/Icon";
import { Switch } from "./ui/Switch";
import { updateUiPrefs, useUiPrefs } from "./uiPrefsStore";

type Loadable<T> = { status: "loading" } | { status: "ready"; value: T } | { status: "error" };

/**
 * Companion's providers, models and lookups (decision 36), as a section that expands and
 * collapses. Its content reads the keychain, so it is mounted only while expanded. The expanded
 * state is saved with the interface preferences. `focusRequest` changes when the Companion view's
 * "Provider settings" link was used: the section opens and its button takes focus.
 */
export function CompanionSection({ focusRequest }: { focusRequest: number }) {
  const { companionSettingsOpen } = useUiPrefs();
  const open = companionSettingsOpen === true;
  const id = useId();
  const buttonRef = useRef<HTMLButtonElement>(null);
  const wantFocus = useRef(false);

  useEffect(() => {
    if (focusRequest === 0) return;
    wantFocus.current = true;
    void updateUiPrefs({ companionSettingsOpen: true });
  }, [focusRequest]);

  // Focus once the section is open, after its content mounted.
  useEffect(() => {
    if (!open || !wantFocus.current) return;
    wantFocus.current = false;
    buttonRef.current?.focus();
    buttonRef.current?.scrollIntoView?.({ block: "start" });
  }, [open]);

  return (
    <section className="settings-disclosure" aria-labelledby={`${id}-h`}>
      <h2 className="section-title" id={`${id}-h`}>
        <button
          ref={buttonRef}
          type="button"
          className="disclosure-button"
          aria-expanded={open}
          aria-controls={`${id}-body`}
          onClick={() => void updateUiPrefs({ companionSettingsOpen: !open })}
        >
          <span className="disclosure-chevron" aria-hidden="true" />
          Companion
        </button>
      </h2>
      <p className="row-desc settings-disclosure-desc">
        Providers, models, keys, budgets and lookups for Companion.
      </p>
      <div id={`${id}-body`} hidden={!open}>
        {open && <CompanionSettings />}
      </div>
    </section>
  );
}

/** Startup and notification preferences. Both are off until the user turns them on. */
export default function Settings({
  headingRef,
  companionFocus = 0,
}: {
  headingRef: RefObject<HTMLHeadingElement | null>;
  /** Changes when the Companion section should open and take focus. */
  companionFocus?: number;
}) {
  const [autostart, setAutostart] = useState<Loadable<boolean>>({ status: "loading" });
  const [notify, setNotify] = useState<Loadable<boolean>>({ status: "loading" });
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    isEnabled()
      .then((value) => setAutostart({ status: "ready", value }))
      .catch(() => setAutostart({ status: "error" }));
    isPermissionGranted()
      .then((value) => setNotify({ status: "ready", value }))
      .catch(() => setNotify({ status: "error" }));
  }, []);

  const toggleAutostart = async (on: boolean) => {
    setBusy(true);
    try {
      await (on ? enable() : disable());
      setAutostart({ status: "ready", value: await isEnabled() });
    } catch {
      setAutostart({ status: "error" });
    } finally {
      setBusy(false);
    }
  };

  const askNotify = async () => {
    setBusy(true);
    try {
      setNotify({ status: "ready", value: (await requestPermission()) === "granted" });
    } catch {
      setNotify({ status: "error" });
    } finally {
      setBusy(false);
    }
  };

  const notifyText =
    notify.status === "loading"
      ? "Checking…"
      : notify.status === "error"
        ? "Status unknown."
        : notify.value
          ? "Allowed. Choose below what Lullshift tells you about."
          : "Not allowed yet. Lullshift can't tell you when a session needs you.";

  return (
    <section aria-labelledby="settings-heading" className="view">
      <header className="view-header">
        <h1 id="settings-heading" ref={headingRef} tabIndex={-1}>
          Settings
        </h1>
        <p className="view-lede">Everything here stays on this computer.</p>
      </header>

      <div className="card rows">
        <Switch
          label="Start with your computer"
          description="Lullshift opens quietly in the tray when you sign in."
          checked={autostart.status === "ready" && autostart.value}
          disabled={busy || autostart.status !== "ready"}
          onChange={(on) => void toggleAutostart(on)}
        />
        {autostart.status === "error" && (
          <Banner tone="danger">The startup setting couldn't be read or changed.</Banner>
        )}
      </div>

      <AppearanceSettings />

      <h2 className="section-title">Notifications</h2>
      <div className="card rows">
        <div className="row">
          <div className="row-text">
            <span className="row-label">System notifications</span>
            <span className="row-desc">{notifyText}</span>
          </div>
          {notify.status === "ready" && !notify.value && (
            <button
              type="button"
              className="btn btn-secondary"
              disabled={busy}
              onClick={() => void askNotify()}
            >
              Allow
            </button>
          )}
          {notify.status === "ready" && notify.value && (
            <span className="pill tone-success">
              <span className="pill-dot" aria-hidden="true" />
              On
            </span>
          )}
        </div>
        <NotificationSettings />
      </div>

      <ContextSettings />

      <CompanionSection focusRequest={companionFocus} />

      <div className="card quiet-card">
        <Icon name="power" />
        <p>
          Closing the window keeps Lullshift in the tray, watching your agents. To stop it, choose{" "}
          <strong>Quit Lullshift</strong> from the tray menu.
        </p>
      </div>
    </section>
  );
}
