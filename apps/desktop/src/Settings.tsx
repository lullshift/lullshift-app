import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { isPermissionGranted, requestPermission } from "@tauri-apps/plugin-notification";
import { useEffect, useState, type RefObject } from "react";
import { Banner } from "./ui/Banner";
import { Icon } from "./ui/Icon";
import { Switch } from "./ui/Switch";

type Loadable<T> = { status: "loading" } | { status: "ready"; value: T } | { status: "error" };

/** Startup and notification preferences. Both are off until the user turns them on. */
export default function Settings({
  headingRef,
}: {
  headingRef: RefObject<HTMLHeadingElement | null>;
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
          ? "Allowed. You'll hear from Lullshift only when a session needs you."
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

        <div className="row">
          <div className="row-text">
            <span className="row-label">Notifications</span>
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
      </div>

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
