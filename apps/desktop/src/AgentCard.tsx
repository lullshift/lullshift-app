import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import {
  adapterErrorText,
  adapterStatusText,
  CAPABILITY_SUMMARY,
  CODEX_TRUST_NOTE,
  hookEventText,
  type AdapterStatus,
  type AdapterView,
  type UninstallView,
} from "./adapters";
import { disconnectNotice, providerIcon, providerName } from "./format";
import { Banner } from "./ui/Banner";
import { Dialog } from "./ui/Dialog";
import { useRestoreFocusAfterDialog } from "./ui/focus";
import { Icon } from "./ui/Icon";
import { TaskContextSwitch } from "./context/TaskContextSwitch";

type Agent = "codex" | "cursor" | "opencode";
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

const SUB: Record<Agent, string> = {
  codex: "OpenAI's coding agent in your terminal",
  cursor: "The Cursor editor's agent",
  opencode: "The open-source coding agent (OpenCode 2.x only)",
};

const FILE: Record<Agent, string> = {
  codex: "your Codex hooks file",
  cursor: "your Cursor hooks file",
  opencode: "OpenCode's plugins folder",
};

/** What the pill says. An installed experimental plugin is "Unknown" until an event arrives. */
function pillFor(agent: Agent, view: AdapterView): { label: string; tone: string } {
  if (agent === "opencode" && view.status === "installed" && view.eventSeen !== true) {
    return { label: "Unknown", tone: "neutral" };
  }
  return PILL[view.status];
}

/**
 * Hook setup for Codex or Cursor. Same consent pattern as Claude Code: nothing is written until
 * the user confirms here and again in the system dialog (ADR-021).
 */
export default function AgentCard({ agent }: { agent: Agent }) {
  const name = providerName(agent);
  const [load, setLoad] = useState<Load>({ status: "loading" });
  const [dialog, setDialog] = useState<"install" | "uninstall" | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<Message | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const actionsRef = useRef<HTMLDivElement>(null);
  const titleRef = useRef<HTMLHeadingElement>(null);
  // Connect and Disconnect swap places after a change; keep focus in the card, not on the page.
  useRestoreFocusAfterDialog(
    dialog !== null,
    () => actionsRef.current?.querySelector("button") ?? titleRef.current,
  );

  const refresh = () =>
    invoke<AdapterView>(`${agent}_status`)
      .then((view) => setLoad({ status: "ready", view }))
      .catch(() => setLoad({ status: "error" }));

  useEffect(() => {
    void refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // An installed experimental plugin is unknown until its first event arrives; look again now
  // and then so the card turns to Connected by itself.
  const waitingForEvent =
    agent === "opencode" &&
    load.status === "ready" &&
    load.view.status === "installed" &&
    load.view.eventSeen !== true;
  useEffect(() => {
    if (!waitingForEvent) return;
    const timer = window.setInterval(() => void refresh(), 5000);
    return () => window.clearInterval(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [waitingForEvent]);

  const act = async (kind: "install" | "uninstall") => {
    setBusy(true);
    setMessage(null);
    setNotice(null);
    try {
      if (kind === "install") {
        const view = await invoke<AdapterView>(`${agent}_install`);
        setLoad({ status: "ready", view });
        setMessage({
          tone: "success",
          text:
            agent === "codex"
              ? "Codex is connected. Open /hooks in Codex and trust the new hooks; until then it sends no events."
              : agent === "opencode"
                ? "The OpenCode plugin is in place. Restart OpenCode (2.x); this turns to Connected when its first event arrives."
                : "Cursor is connected. Its sessions appear under Sessions as they report.",
        });
      } else {
        const r = await invoke<UninstallView>(`${agent}_uninstall`);
        setLoad({ status: "ready", view: r.view });
        setMessage({
          tone: "success",
          text:
            agent === "opencode"
              ? r.outcome === "nothing_to_remove"
                ? "There was no Lullshift plugin file to remove."
                : "Disconnected. The Lullshift plugin file was removed. Restart OpenCode to stop it from running."
              : r.outcome === "restored"
                ? "Disconnected. Your hooks file was restored exactly as it was."
                : r.outcome === "removed"
                  ? "Disconnected. Changes you made to the file since connecting were kept."
                  : "There were no Lullshift hooks to remove.",
        });
        setNotice(disconnectNotice(agent, r.sessionsDisconnected ?? 0));
      }
    } catch (err: unknown) {
      if (String(err) !== "cancelled") {
        setMessage({ tone: "danger", text: adapterErrorText(String(err), agent) });
      }
      void refresh();
    } finally {
      setBusy(false);
      setDialog(null);
    }
  };

  const view = load.status === "ready" ? load.view : null;
  const canInstall = view && view.status !== "installed" && view.status !== "settings_invalid";
  const canRemove =
    view && ["installed", "needs_update", "partial", "hooks_disabled"].includes(view.status);
  const pill = view ? pillFor(agent, view) : null;
  const titleId = `${agent}-title`;

  return (
    <>
      {message && <Banner tone={message.tone}>{message.text}</Banner>}
      {notice && <Banner tone="info">{notice}</Banner>}

      <article className="card integration" aria-labelledby={titleId}>
        <div className="integration-head">
          <span className="session-glyph" aria-hidden="true">
            <Icon name={providerIcon(agent)} />
          </span>
          <div className="integration-titles">
            <h2 id={titleId} ref={titleRef} tabIndex={-1} className="card-title">
              {name}
              {agent === "opencode" && (
                <>
                  {" "}
                  <span className="pill tone-attention">Experimental</span>
                </>
              )}
            </h2>
            <p className="card-sub">{SUB[agent]}</p>
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
          <Banner tone="danger">Couldn't check the {name} setup. Its status is unknown.</Banner>
        )}
        {view && (
          <p className="card-text">{adapterStatusText(view.status, agent, view.eventSeen)}</p>
        )}

        <ul className="facts">
          <li>
            <Icon name="lock" size={18} />
            <span>
              Only the session ID, event type and time reach Lullshift. Never your code, replies or
              email. Your first prompt is kept only if you turn on Task context below.
            </span>
          </li>
          {CAPABILITY_SUMMARY[agent].map((line) => (
            <li key={line}>
              <Icon name="info" size={18} />
              <span>{line}</span>
            </li>
          ))}
          {agent === "codex" && (
            <li>
              <Icon name="info" size={18} />
              <span>{CODEX_TRUST_NOTE}</span>
            </li>
          )}
        </ul>

        <TaskContextSwitch agent={agent} />

        {(canInstall || canRemove) && (
          <div className="actions" ref={actionsRef}>
            {canInstall && (
              <button
                type="button"
                className="btn btn-primary"
                disabled={busy}
                onClick={() => setDialog("install")}
              >
                {view.status === "not_installed" || view.status === "hooks_disabled"
                  ? `Connect ${name}`
                  : "Repair connection"}
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

      <Dialog
        open={dialog === "install"}
        title={`Connect ${name}?`}
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
            <p>
              {agent === "opencode"
                ? "Lullshift will create this plugin file, and change nothing else:"
                : "Lullshift will add small hooks to this file:"}
            </p>
            <p className="path">
              <code>{view?.settingsPath ?? FILE[agent]}</code>
            </p>
            <p>
              Then {name} tells Lullshift when {hookEventText(view?.events ?? []).join(", ")}.
            </p>
          </>
        }
      >
        <ul className="checklist">
          <li>
            <Icon name="check" size={18} />
            <span>
              Lullshift receives only the session ID, the event type and the time. Never your
              prompts, code, file paths or {name}'s replies.
            </span>
          </li>
          {agent === "opencode" ? (
            <>
              <li>
                <Icon name="check" size={18} />
                <span>
                  One new file that Lullshift owns. A file with that name that Lullshift didn't
                  write is never replaced, and none of your OpenCode settings are touched.
                </span>
              </li>
              <li>
                <Icon name="check" size={18} />
                <span>Disconnect at any time. It deletes the file.</span>
              </li>
              <li>
                <Icon name="info" size={18} />
                <span>
                  Experimental: this works only with OpenCode 2.x, which Lullshift can't check. On
                  OpenCode 1.x it does nothing, and OpenCode may show a plugin error. OpenCode loads
                  the plugin the next time it starts, and the status stays unknown until its first
                  event arrives.
                </span>
              </li>
            </>
          ) : (
            <>
              <li>
                <Icon name="check" size={18} />
                <span>
                  A backup is saved first. Your own hooks stay, though the file's spacing and
                  formatting may change.
                </span>
              </li>
              <li>
                <Icon name="check" size={18} />
                <span>
                  Disconnect at any time. If nothing else changed the file, it's restored exactly.
                </span>
              </li>
            </>
          )}
          {agent === "codex" && (
            <li>
              <Icon name="info" size={18} />
              <span>{CODEX_TRUST_NOTE}</span>
            </li>
          )}
          {agent === "cursor" && (
            <li>
              <Icon name="info" size={18} />
              <span>
                Cursor doesn't tell Lullshift when it is waiting for you, so you won't get a "needs
                you" notification for Cursor.
              </span>
            </li>
          )}
          <li>
            <Icon name="info" size={18} />
            <span>
              Disconnect before uninstalling or moving Lullshift; otherwise {name} keeps trying to
              run it.
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
        title={`Disconnect ${name}?`}
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
              {busy ? "Disconnecting…" : agent === "opencode" ? "Remove plugin" : "Remove hooks"}
            </button>
          </>
        }
        description={
          <p>
            {agent === "opencode"
              ? "Lullshift deletes its plugin file and stops receiving OpenCode events. Your OpenCode settings stay as they are."
              : `Lullshift removes its hooks and stops receiving ${name} events. Your own hooks and settings stay as they are.`}
          </p>
        }
      />
    </>
  );
}
