import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import {
  adapterErrorText,
  adapterStatusText,
  type AdapterView,
  type UninstallView,
} from "./adapters";

type Load = { status: "loading" } | { status: "ready"; view: AdapterView } | { status: "error" };

/** Claude Code hook setup. Nothing is written until the user confirms the consent panel. */
export default function Integrations() {
  const [load, setLoad] = useState<Load>({ status: "loading" });
  const [confirming, setConfirming] = useState<"install" | "uninstall" | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  const refresh = () =>
    invoke<AdapterView>("claude_code_status")
      .then((view) => setLoad({ status: "ready", view }))
      .catch(() => setLoad({ status: "error" }));

  useEffect(() => {
    void refresh();
  }, []);

  const act = async (kind: "install" | "uninstall") => {
    setBusy(true);
    setMessage(null);
    try {
      if (kind === "install") {
        const view = await invoke<AdapterView>("claude_code_install");
        setLoad({ status: "ready", view });
        setMessage("Claude Code hooks added. Sessions appear here as Claude Code reports them.");
      } else {
        const r = await invoke<UninstallView>("claude_code_uninstall");
        setLoad({ status: "ready", view: r.view });
        setMessage(
          r.outcome === "restored"
            ? "Hooks removed. Your settings file was restored exactly as it was."
            : r.outcome === "removed"
              ? "Hooks removed. Changes you made to the file since then were kept."
              : "There were no Lullshift hooks to remove.",
        );
      }
    } catch (err: unknown) {
      setMessage(adapterErrorText(String(err)));
      void refresh();
    } finally {
      setBusy(false);
      setConfirming(null);
    }
  };

  const view = load.status === "ready" ? load.view : null;
  const connected = view?.status === "installed";
  const canInstall = view && view.status !== "installed" && view.status !== "settings_invalid";
  const canRemove = view && ["installed", "needs_update", "partial"].includes(view.status);

  return (
    <section aria-labelledby="integrations-heading" className="integrations">
      <h2 id="integrations-heading">Agent integrations</h2>
      <div className="card">
        <h3>Claude Code</h3>
        {load.status === "loading" && <p role="status">Checking…</p>}
        {load.status === "error" && (
          <p role="alert">Couldn't check the Claude Code setup. Its status is unknown.</p>
        )}
        {view && <p>{adapterStatusText(view.status)}</p>}
        {message && <p role="status">{message}</p>}

        {confirming === null && (
          <div className="actions">
            {canInstall && (
              <button type="button" disabled={busy} onClick={() => setConfirming("install")}>
                {view.status === "not_installed" ? "Connect Claude Code…" : "Repair hooks…"}
              </button>
            )}
            {canRemove && (
              <button type="button" disabled={busy} onClick={() => setConfirming("uninstall")}>
                Disconnect…
              </button>
            )}
          </div>
        )}

        {confirming === "install" && view && (
          <div className="consent" role="group" aria-labelledby="consent-heading">
            <h4 id="consent-heading">Add Lullshift hooks to Claude Code?</h4>
            <p>
              Lullshift will edit <code>{view.settingsPath ?? "your Claude Code settings"}</code> so
              Claude Code runs Lullshift on these events: {view.events.join(", ")}.
            </p>
            <ul>
              <li>
                Lullshift receives only the session ID, the event type and the time. It never
                receives your prompts, code, file paths or Claude's replies.
              </li>
              <li>
                A backup of the file is saved first. Your other settings stay, though the file's
                spacing and formatting may change.
              </li>
              <li>
                Disconnect at any time. If nothing else changed the file, it's restored exactly.
              </li>
              <li>Folders Claude Code doesn't trust yet send no events.</li>
              <li>
                If you set <code>CLAUDE_CONFIG_DIR</code>, Lullshift uses the value it was started
                with. Check the path above matches the Claude Code you use.
              </li>
              <li>
                Disconnect before uninstalling or moving Lullshift; otherwise Claude Code keeps
                trying to run it.
              </li>
            </ul>
            <div className="actions">
              <button type="button" disabled={busy} onClick={() => void act("install")}>
                Add hooks
              </button>
              <button type="button" disabled={busy} onClick={() => setConfirming(null)}>
                Cancel
              </button>
            </div>
          </div>
        )}

        {confirming === "uninstall" && (
          <div className="consent" role="group" aria-labelledby="remove-heading">
            <h4 id="remove-heading">Remove Lullshift's hooks from Claude Code?</h4>
            <p>
              Lullshift stops receiving Claude Code events. Your own hooks and settings stay as they
              are.
            </p>
            <div className="actions">
              <button type="button" disabled={busy} onClick={() => void act("uninstall")}>
                Remove hooks
              </button>
              <button type="button" disabled={busy} onClick={() => setConfirming(null)}>
                Cancel
              </button>
            </div>
          </div>
        )}
        {connected && confirming === null && (
          <p className="note">
            Claude Code sessions started before connecting begin reporting once Claude Code reloads
            its settings.
          </p>
        )}
      </div>
    </section>
  );
}
