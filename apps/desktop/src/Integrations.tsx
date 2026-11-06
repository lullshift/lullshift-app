import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState, type RefObject } from "react";
import {
  adapterErrorText,
  adapterStatusText,
  hookEventText,
  type AdapterStatus,
  type AdapterView,
  type UninstallView,
} from "./adapters";
import AgentCard from "./AgentCard";
import { TaskContextSwitch } from "./context/TaskContextSwitch";
import { disconnectNotice } from "./format";
import { Banner } from "./ui/Banner";
import { Dialog } from "./ui/Dialog";
import { useRestoreFocusAfterDialog } from "./ui/focus";
import { Icon } from "./ui/Icon";

type Load = { status: "loading" } | { status: "ready"; view: AdapterView } | { status: "error" };
type Message = { tone: "success" | "danger"; text: string };

const PILL: Record<AdapterStatus, { label: string; tone: string }> = {
  installed: { label: "Connected", tone: "success" },
  not_installed: { label: "Not connected", tone: "neutral" },
  needs_update: { label: "Needs repair", tone: "attention" },
  partial: { label: "Partly connected", tone: "attention" },
  settings_invalid: { label: "Can't connect", tone: "danger" },
  hooks_disabled: { label: "Hooks turned off", tone: "attention" },
};

/** Claude Code hook setup. Nothing is written until the user confirms the consent dialog. */
export default function Integrations({
  headingRef,
}: {
  headingRef: RefObject<HTMLHeadingElement | null>;
}) {
  const [load, setLoad] = useState<Load>({ status: "loading" });
  const [dialog, setDialog] = useState<"install" | "uninstall" | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<Message | null>(null);
  /** How many sessions stopped being tracked after a disconnect (S05-T10); null if none. */
  const [notice, setNotice] = useState<string | null>(null);

  const actionsRef = useRef<HTMLDivElement>(null);
  // Connect and Disconnect swap places after a change; keep focus in the card, not on the page.
  useRestoreFocusAfterDialog(
    dialog !== null,
    () => actionsRef.current?.querySelector("button") ?? headingRef.current,
  );

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
    setNotice(null);
    try {
      if (kind === "install") {
        const view = await invoke<AdapterView>("claude_code_install");
        setLoad({ status: "ready", view });
        setMessage({
          tone: "success",
          text: "Claude Code is connected. Its sessions appear under Sessions as they report.",
        });
      } else {
        const r = await invoke<UninstallView>("claude_code_uninstall");
        setLoad({ status: "ready", view: r.view });
        setMessage({
          tone: "success",
          text:
            r.outcome === "restored"
              ? "Disconnected. Your settings file was restored exactly as it was."
              : r.outcome === "removed"
                ? "Disconnected. Changes you made to the file since connecting were kept."
                : "There were no Lullshift hooks to remove.",
        });
        setNotice(disconnectNotice("claude-code", r.sessionsDisconnected ?? 0));
      }
    } catch (err: unknown) {
      // "cancelled": the user said no in the system dialog; nothing changed and nothing to say.
      if (String(err) !== "cancelled") {
        setMessage({ tone: "danger", text: adapterErrorText(String(err)) });
      }
      void refresh();
    } finally {
      setBusy(false);
      setDialog(null);
    }
  };

  const view = load.status === "ready" ? load.view : null;
  const canInstall = view && view.status !== "installed" && view.status !== "settings_invalid";
  const canRemove = view && ["installed", "needs_update", "partial"].includes(view.status);
  const pill = view ? PILL[view.status] : null;

  return (
    <section aria-labelledby="integrations-heading" className="view">
      <header className="view-header">
        <h1 id="integrations-heading" ref={headingRef} tabIndex={-1}>
          Integrations
        </h1>
        <p className="view-lede">Connect the agents you use so Lullshift can follow them.</p>
      </header>

      {message && <Banner tone={message.tone}>{message.text}</Banner>}
      {notice && <Banner tone="info">{notice}</Banner>}

      <article className="card integration" aria-labelledby="claude-code-title">
        <div className="integration-head">
          <span className="session-glyph" aria-hidden="true">
            <Icon name="terminal" />
          </span>
          <div className="integration-titles">
            <h2 id="claude-code-title" className="card-title">
              Claude Code
            </h2>
            <p className="card-sub">Anthropic's coding agent in your terminal</p>
          </div>
          {pill && (
            <span className={`pill tone-${pill.tone}`}>
              <span className="pill-dot" aria-hidden="true" />
              {pill.label}
            </span>
          )}
        </div>

        {load.status === "loading" && <p role="status">Checking…</p>}
        {load.status === "error" && (
          <Banner tone="danger">
            Couldn't check the Claude Code setup. Its status is unknown.
          </Banner>
        )}
        {view && <p className="card-text">{adapterStatusText(view.status)}</p>}

        <ul className="facts">
          <li>
            <Icon name="lock" size={18} />
            <span>
              Only the session ID, event type and time reach Lullshift. Never your code or files.
              Your first prompt and Claude's questions are kept only if you turn on Task context
              below.
            </span>
          </li>
          <li>
            <Icon name="bell" size={18} />
            <span>You get one gentle notification when a session needs you.</span>
          </li>
        </ul>

        <TaskContextSwitch agent="claude-code" />

        {(canInstall || canRemove) && (
          <div className="actions" ref={actionsRef}>
            {canInstall && (
              <button
                type="button"
                className="btn btn-primary"
                disabled={busy}
                onClick={() => setDialog("install")}
              >
                {view.status === "not_installed" ? "Connect Claude Code" : "Repair connection"}
              </button>
            )}
            {canRemove && (
              <button
                type="button"
                className="btn btn-secondary"
                disabled={busy}
                onClick={() => setDialog("uninstall")}
              >
                Disconnect
              </button>
            )}
          </div>
        )}
      </article>

      <AgentCard agent="codex" />
      <AgentCard agent="cursor" />
      <AgentCard agent="opencode" />

      <Dialog
        open={dialog === "install"}
        title="Connect Claude Code?"
        busy={busy}
        onClose={() => setDialog(null)}
        actions={
          <>
            <button
              type="button"
              className="btn btn-quiet"
              disabled={busy}
              onClick={() => setDialog(null)}
            >
              Cancel
            </button>
            <button
              type="button"
              className="btn btn-primary"
              disabled={busy}
              onClick={() => void act("install")}
            >
              {busy ? "Waiting for you…" : "Continue"}
            </button>
          </>
        }
        description={
          <>
            <p>Lullshift will add a small hook to this file:</p>
            <p className="path">
              <code>{view?.settingsPath ?? "your Claude Code settings file"}</code>
            </p>
            <p>
              Then Claude Code tells Lullshift when {hookEventText(view?.events ?? []).join(", ")}.
            </p>
          </>
        }
      >
        <ul className="checklist">
          <li>
            <Icon name="check" size={18} />
            <span>
              Lullshift receives only the session ID, the event type and the time. Never your
              prompts, code, file paths or Claude's replies.
            </span>
          </li>
          <li>
            <Icon name="check" size={18} />
            <span>
              A backup is saved first. Your other settings stay, though the file's spacing and
              formatting may change.
            </span>
          </li>
          <li>
            <Icon name="check" size={18} />
            <span>
              Disconnect at any time. If nothing else changed the file, it's restored exactly.
            </span>
          </li>
          <li>
            <Icon name="info" size={18} />
            <span>Folders Claude Code doesn't trust yet send no events.</span>
          </li>
          <li>
            <Icon name="info" size={18} />
            <span>
              If you set <code>CLAUDE_CONFIG_DIR</code>, Lullshift uses the value it was started
              with. Check the path above is the one your Claude Code uses.
            </span>
          </li>
          <li>
            <Icon name="info" size={18} />
            <span>
              Disconnect before uninstalling or moving Lullshift; otherwise Claude Code keeps trying
              to run it.
            </span>
          </li>
        </ul>
        <p className="dialog-note">
          Next, your system asks you to confirm once more. Lullshift changes the file only after you
          say yes there.
        </p>
      </Dialog>

      <Dialog
        open={dialog === "uninstall"}
        title="Disconnect Claude Code?"
        busy={busy}
        onClose={() => setDialog(null)}
        actions={
          <>
            <button
              type="button"
              className="btn btn-quiet"
              disabled={busy}
              onClick={() => setDialog(null)}
            >
              Cancel
            </button>
            <button
              type="button"
              className="btn btn-danger"
              disabled={busy}
              onClick={() => void act("uninstall")}
            >
              {busy ? "Disconnecting…" : "Remove hooks"}
            </button>
          </>
        }
        description={
          <p>
            Lullshift removes its hooks and stops receiving Claude Code events. Your own hooks and
            settings stay as they are.
          </p>
        }
      />
    </section>
  );
}
