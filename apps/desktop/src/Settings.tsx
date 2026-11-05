import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { isPermissionGranted, requestPermission } from "@tauri-apps/plugin-notification";
import { useEffect, useState } from "react";

type Loadable<T> = { status: "loading" } | { status: "ready"; value: T } | { status: "error" };

/** Startup and notification preferences. Both are off until the user turns them on. */
export default function Settings() {
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

  return (
    <section aria-labelledby="settings-heading" className="settings">
      <h2 id="settings-heading">Settings</h2>
      <p className="note">
        Closing the window keeps Lullshift running in the tray. Use “Quit Lullshift” in the tray
        menu to stop it.
      </p>
      <label className="setting">
        <input
          type="checkbox"
          checked={autostart.status === "ready" && autostart.value}
          disabled={busy || autostart.status !== "ready"}
          onChange={(e) => void toggleAutostart(e.currentTarget.checked)}
        />
        <span>Start Lullshift when I sign in to my computer (starts in the tray)</span>
      </label>
      {autostart.status === "error" && (
        <p role="alert" className="note">
          The startup setting couldn't be read or changed.
        </p>
      )}
      <div className="setting">
        <span>
          Notifications:{" "}
          {notify.status === "loading"
            ? "checking…"
            : notify.status === "error"
              ? "status unknown"
              : notify.value
                ? "allowed"
                : "not allowed"}
        </span>
        {notify.status === "ready" && !notify.value && (
          <button type="button" disabled={busy} onClick={() => void askNotify()}>
            Allow notifications
          </button>
        )}
      </div>
    </section>
  );
}
