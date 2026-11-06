import { useId, useRef, useState } from "react";
import type { SessionView } from "../sessions";
import { Banner } from "../ui/Banner";
import { Dialog } from "../ui/Dialog";
import {
  companionPreviewContext,
  toCommandError,
  type ContextPreview,
  type ProviderProfile,
  type Scope,
} from "./api";
import { ContextPicker } from "./ContextPicker";
import {
  buildScope,
  canSend,
  charCountText,
  EMPTY_SCOPE,
  errorText,
  needsPreview,
  scopeKey,
  scopeLabel,
  sendsToText,
  CONTEXT_CHANGED_NOTE,
  historyText,
  type ConfirmedPreview,
} from "./logic";

/** What came of a send. `contextChanged`: the backend refused because the context differs from the preview. */
export interface SendResult {
  stored: boolean;
  contextChanged: boolean;
}

interface Props {
  profiles: readonly ProviderProfile[];
  profile: ProviderProfile | null;
  onPickProfile: (id: string) => void;
  sessions: readonly SessionView[];
  sending: boolean;
  /** How many earlier messages go along (same host only). */
  historyCount: number;
  /** `expectedSha256` is the hash of the preview the user confirmed (null for no context). */
  onSend: (text: string, scope: Scope, expectedSha256: string | null) => Promise<SendResult>;
  onCancel: () => void;
  onOpenSettings: () => void;
}

type PreviewState =
  | { phase: "closed" }
  | { phase: "loading" }
  | { phase: "error"; text: string }
  | { phase: "ready"; preview: ContextPreview; note: string | null }
  | { phase: "confirming"; preview: ContextPreview };

export function Composer({
  profiles,
  profile,
  onPickProfile,
  sessions,
  sending,
  historyCount,
  onSend,
  onCancel,
  onOpenSettings,
}: Props) {
  const textId = useId();
  const profileId = useId();
  const [text, setText] = useState("");
  const [draft, setDraft] = useState(EMPTY_SCOPE);
  const [preview, setPreview] = useState<PreviewState>({ phase: "closed" });
  const ticket = useRef(0);
  const scope = buildScope(draft);
  const host = profile?.host ?? null;
  const hasKey = !!profile?.hasKey;
  const ready = !!profile && hasKey && !!scope && text.trim().length > 0 && !sending;

  const transmit = async (confirmed: ConfirmedPreview | null) => {
    if (!scope || !profile) return;
    if (!canSend({ text, scope, confirmed, hasProfile: hasKey, sending })) return;
    const toSend = text;
    const draftBefore = draft;
    // The scope never carries over: back to "No context" as soon as this send starts.
    setDraft(EMPTY_SCOPE);
    const result = await onSend(toSend, scope, confirmed?.sha256 ?? null);
    if (result.contextChanged) {
      // Nothing was stored or sent. Put the choice back and show what would be sent now.
      setDraft(draftBefore);
      void openPreview(CONTEXT_CHANGED_NOTE, scope);
      return;
    }
    if (result.stored) setText((cur) => (cur === toSend ? "" : cur));
  };

  const openPreview = async (note: string | null = null, forScope: Scope | null = scope) => {
    if (!forScope) return;
    const mine = ++ticket.current;
    setPreview({ phase: "loading" });
    try {
      const p = await companionPreviewContext(forScope);
      if (mine === ticket.current) setPreview({ phase: "ready", preview: p, note });
    } catch (err) {
      if (mine === ticket.current) {
        setPreview({ phase: "error", text: errorText(toCommandError(err), host) });
      }
    }
  };

  const submit = () => {
    if (!ready) return;
    if (needsPreview(scope, null)) void openPreview();
    else void transmit(null);
  };

  const closePreview = () => {
    ticket.current++;
    setPreview({ phase: "closed" });
  };

  const confirmPreview = async () => {
    if (preview.phase !== "ready" || !scope) return;
    const seen = preview.preview;
    const mine = ++ticket.current;
    setPreview({ phase: "confirming", preview: seen });
    try {
      // Build it again right before sending: if the notes changed since the user read them,
      // show the new text and ask again instead of sending something they didn't see.
      const again = await companionPreviewContext(scope);
      if (mine !== ticket.current) return;
      if (
        again.text !== seen.text ||
        again.sha256 !== seen.sha256 ||
        scopeKey(again.scope) !== scopeKey(scope)
      ) {
        setPreview({
          phase: "ready",
          preview: again,
          note: CONTEXT_CHANGED_NOTE,
        });
        return;
      }
      setPreview({ phase: "closed" });
      await transmit({ scope, text: seen.text, sha256: seen.sha256 });
    } catch (err) {
      if (mine === ticket.current) {
        setPreview({ phase: "error", text: errorText(toCommandError(err), host) });
      }
    }
  };

  const shown =
    preview.phase === "ready" || preview.phase === "confirming" ? preview.preview : null;

  return (
    <div className="card composer">
      <div className="composer-provider" aria-label="Provider for this message">
        {profile ? (
          <>
            <span className="composer-chip">{profile.label}</span>
            <span className="composer-chip">{profile.model || "No model set"}</span>
            <span className="composer-chip composer-host">{sendsToText(host)}</span>
            {profiles.length > 1 && (
              <>
                <label className="visually-hidden" htmlFor={profileId}>
                  Provider
                </label>
                <select
                  id={profileId}
                  className="field composer-select"
                  value={profile.id}
                  disabled={sending}
                  onChange={(e) => onPickProfile(e.target.value)}
                >
                  {profiles.map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.label}
                    </option>
                  ))}
                </select>
              </>
            )}
          </>
        ) : (
          <span className="composer-chip">{sendsToText(null)}</span>
        )}
      </div>

      {!profile && (
        <Banner tone="info">
          Set up a provider to start.{" "}
          <button type="button" className="btn-link" onClick={onOpenSettings}>
            Open Companion settings
          </button>
        </Banner>
      )}
      {profile && !hasKey && (
        <Banner tone="attention">
          {profile.keychainOk
            ? "No key is set for this provider yet."
            : "The system keychain isn't available, so no key can be read."}{" "}
          <button type="button" className="btn-link" onClick={onOpenSettings}>
            Open Companion settings
          </button>
        </Banner>
      )}

      <label className="field-label" htmlFor={textId}>
        Message
      </label>
      <textarea
        id={textId}
        className="field textarea composer-text"
        rows={4}
        maxLength={20000}
        value={text}
        readOnly={sending}
        placeholder="Ask something. Press Ctrl+Enter to send."
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
            e.preventDefault();
            submit();
          }
        }}
      />

      <ContextPicker draft={draft} onChange={setDraft} sessions={sessions} disabled={sending} />
      <p className="field-note composer-history">{historyText(historyCount, host)}</p>

      <div className="composer-actions">
        <p className="field-note composer-summary">
          {scope?.kind === "none" || !scope
            ? "This message sends only what you typed."
            : `This message will also send: ${scopeLabel(scope.kind).toLowerCase()}. You will see it first.`}
        </p>
        {sending && (
          <button type="button" className="btn btn-secondary" onClick={onCancel}>
            Cancel
          </button>
        )}
        <button type="button" className="btn btn-primary" disabled={!ready} onClick={submit}>
          {sending ? "Sending…" : scope && scope.kind !== "none" ? "Review and send" : "Send"}
        </button>
      </div>

      <Dialog
        open={preview.phase !== "closed"}
        title="Check what will be sent"
        busy={preview.phase === "confirming"}
        onClose={closePreview}
        actions={
          <>
            <button
              type="button"
              className="btn btn-quiet"
              autoFocus
              disabled={preview.phase === "confirming"}
              onClick={closePreview}
            >
              Cancel
            </button>
            {(preview.phase === "ready" || preview.phase === "confirming") && (
              <button
                type="button"
                className="btn btn-primary"
                disabled={preview.phase === "confirming"}
                onClick={() => void confirmPreview()}
              >
                Send with this context
              </button>
            )}
          </>
        }
        description={
          shown ? (
            <>
              <p>
                <strong>{sendsToText(host)}</strong>
                {profile ? ` (${profile.label})` : ""}. Scope: {scopeLabel(shown.scope.kind)}.{" "}
                {charCountText(shown.chars)}.
              </p>
              <p>{historyText(historyCount, host)}</p>
              <p>
                This exact text goes to the provider along with your message. Nothing else from your
                sessions is included.
              </p>
            </>
          ) : undefined
        }
      >
        {preview.phase === "loading" && <p>Preparing the preview…</p>}
        {preview.phase === "error" && <Banner tone="danger">{preview.text}</Banner>}
        {preview.phase === "ready" && preview.note && (
          <Banner tone="attention">{preview.note}</Banner>
        )}
        {shown && (
          <pre className="preview-text" tabIndex={0} aria-label="Exact text that will be sent">
            {shown.text}
          </pre>
        )}
      </Dialog>
    </div>
  );
}
