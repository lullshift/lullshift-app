import { useEffect, useId, useRef, useState } from "react";
import { useSessionDetail } from "../context/useSessionDetail";
import { ellipsize, providerName, sessionName } from "../format";
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
  rememberedText,
  restoreScope,
  type ConfirmedPreview,
  type ScopeDraft,
} from "./logic";
import {
  COMPOSER_HINT,
  composerKeyAction,
  defaultChoice,
  modelName,
  modelsOf,
  sameChoice,
  type ModelChoice,
} from "./modelChoice";

/** What came of a send. `contextChanged`: the backend refused because the context differs from the preview. */
export interface SendResult {
  stored: boolean;
  contextChanged: boolean;
}

interface Props {
  profiles: readonly ProviderProfile[];
  /** The provider and model the next message goes to (the pick, else the default). */
  choice: ModelChoice | null;
  /** The user picked a provider and model for the next message only. */
  onPickChoice: (c: ModelChoice) => void;
  sessions: readonly SessionView[];
  sending: boolean;
  /** How many earlier messages go along (same host only). */
  historyCount: number;
  /** The context choice of the last message sent; the composer starts with it if it still works. */
  rememberedScope: Scope | null;
  /** `expectedSha256` is the hash of the preview the user confirmed (null for no context). */
  onSend: (text: string, scope: Scope, expectedSha256: string | null) => Promise<SendResult>;
  onCancel: () => void;
  onOpenSettings: () => void;
  /** Puts this text back in the message box (a new nonce each time), e.g. after a refused send. */
  restore?: { text: string; nonce: number } | null;
}

type PreviewState =
  | { phase: "closed" }
  | { phase: "loading" }
  | { phase: "error"; text: string }
  | { phase: "ready"; preview: ContextPreview; note: string | null }
  | { phase: "confirming"; preview: ContextPreview };

/** "Model · Provider" for a pair. */
export function choiceText(profiles: readonly ProviderProfile[], c: ModelChoice | null): string {
  if (!c) return "No model set up";
  const p = profiles.find((x) => x.id === c.profileId);
  const name = modelName(profiles, c.profileId, c.model);
  return p ? `${name} · ${p.label}` : name;
}

/** The modal that picks a provider and model for the next message, grouped by provider. */
export function ModelChoiceDialog({
  open,
  profiles,
  current,
  onPick,
  onClose,
  onOpenSettings,
}: {
  open: boolean;
  profiles: readonly ProviderProfile[];
  current: ModelChoice | null;
  onPick: (c: ModelChoice) => void;
  onClose: () => void;
  onOpenSettings: () => void;
}) {
  const name = useId();
  const fallback = defaultChoice(profiles);
  return (
    <Dialog
      open={open}
      title="Model for this message"
      onClose={onClose}
      actions={
        <>
          <button type="button" className="btn btn-quiet" onClick={onOpenSettings}>
            Manage models
          </button>
          <button type="button" className="btn btn-primary" onClick={onClose}>
            Done
          </button>
        </>
      }
      description={
        <p>
          Your pick is used for the next message only. After it is sent, Companion goes back to the
          default: {choiceText(profiles, fallback)}.
        </p>
      }
    >
      <div className="choice-groups">
        {profiles.map((p) => (
          <fieldset key={p.id} className="choice-group-provider">
            <legend>
              {p.label} <span className="field-note">({sendsToText(p.host)})</span>
            </legend>
            {modelsOf(p).map((m) => {
              const c = { profileId: p.id, model: m.model };
              return (
                <label key={m.model} className="pick">
                  <input
                    type="radio"
                    name={name}
                    checked={sameChoice(c, current)}
                    onChange={() => onPick(c)}
                  />
                  <span>
                    {m.label ?? m.model}
                    {m.label && <code className="model-row-id">{m.model}</code>}
                    {m.isDefault && p.isDefault && (
                      <span className="pill tone-neutral"> Default</span>
                    )}
                  </span>
                </label>
              );
            })}
          </fieldset>
        ))}
      </div>
    </Dialog>
  );
}

const ITEM_KIND: Record<string, string> = {
  decision: "Decision",
  checklist: "Checklist",
  next_move: "Next move",
};

interface Badge {
  key: string;
  text: string;
  full: string;
  remove: () => void;
}

function BadgeItem({ b, disabled, onOpen }: { b: Badge; disabled: boolean; onOpen: () => void }) {
  return (
    <li className="option-badge">
      <button
        type="button"
        className="option-badge-text"
        title={b.full}
        aria-label={`${b.full}. Change options`}
        disabled={disabled}
        onClick={onOpen}
      >
        {b.text}
      </button>
      <button
        type="button"
        className="option-badge-remove"
        aria-label={`Remove ${b.full}`}
        disabled={disabled}
        onClick={b.remove}
      >
        <span aria-hidden="true">×</span>
      </button>
    </li>
  );
}

/** The ticked notes and items of a selected-notes draft. Mounted only when a session is chosen. */
function NoteBadgeItems({
  draft,
  setDraft,
  disabled,
  onOpen,
}: {
  draft: ScopeDraft;
  setDraft: (d: ScopeDraft) => void;
  disabled: boolean;
  onOpen: () => void;
}) {
  const { load } = useSessionDetail({ provider: draft.provider, sessionId: draft.sessionId }, 0);
  const detail = load.status === "ready" ? load.detail : load.status === "error" ? load.last : null;
  const out: Badge[] = [];
  if (draft.notes) {
    out.push({
      key: "notes",
      text: "Your notes",
      full: "Your notes",
      remove: () => setDraft({ ...draft, notes: false }),
    });
  }
  for (const id of draft.itemIds) {
    const item = detail?.items.find((i) => i.id === id);
    const full = item
      ? `${ITEM_KIND[item.kind] ?? "Item"}: ${item.text.replace(/\s+/g, " ")}`
      : "A ticked item";
    out.push({
      key: `item-${id}`,
      text: ellipsize(full, 40),
      full,
      remove: () => setDraft({ ...draft, itemIds: draft.itemIds.filter((x) => x !== id) }),
    });
  }
  return (
    <>
      {out.map((b) => (
        <BadgeItem key={b.key} b={b} disabled={disabled} onOpen={onOpen} />
      ))}
    </>
  );
}

/** The chosen options as badges: click one to change the options, × removes it. */
export function OptionBadges({
  draft,
  setDraft,
  sessions,
  disabled,
  onOpen,
}: {
  draft: ScopeDraft;
  setDraft: (d: ScopeDraft) => void;
  sessions: readonly SessionView[];
  disabled: boolean;
  onOpen: () => void;
}) {
  if (draft.kind === "none") return null;
  const notesOn = draft.kind === "selectedNotes" && !!draft.provider && !!draft.sessionId;
  const s = sessions.find(
    (x) => x.provider === draft.provider && x.providerSessionId === draft.sessionId,
  );
  const where = s
    ? `${ellipsize(sessionName(s).primary, 40)} (${providerName(s.provider)})`
    : draft.sessionId
      ? "a session"
      : "no session chosen";
  const full = `${scopeLabel(draft.kind)}: ${where}`;
  return (
    <ul className="option-badges" aria-label="Options for this message">
      <BadgeItem
        b={{ key: "scope", text: full, full, remove: () => setDraft(EMPTY_SCOPE) }}
        disabled={disabled}
        onOpen={onOpen}
      />
      {notesOn && (
        <NoteBadgeItems
          key={`${draft.provider}:${draft.sessionId}`}
          draft={draft}
          setDraft={setDraft}
          disabled={disabled}
          onOpen={onOpen}
        />
      )}
    </ul>
  );
}

export function Composer({
  profiles,
  choice,
  onPickChoice,
  sessions,
  sending,
  historyCount,
  rememberedScope,
  onSend,
  onCancel,
  onOpenSettings,
  restore = null,
}: Props) {
  const textId = useId();
  const hintId = useId();
  const [text, setText] = useState("");
  const [restoredNonce, setRestoredNonce] = useState<number | null>(null);
  // Adjusting state while rendering (React's pattern for state derived from a prop change).
  if (restore && restore.nonce !== restoredNonce) {
    setRestoredNonce(restore.nonce);
    setText(restore.text);
  }
  const [draft, setDraftState] = useState<ScopeDraft>(EMPTY_SCOPE);
  const [restoreNote, setRestoreNote] = useState<string | null>(null);
  const [preview, setPreview] = useState<PreviewState>({ phase: "closed" });
  const [optionsOpen, setOptionsOpen] = useState(false);
  const [choosing, setChoosing] = useState(false);
  const ticket = useRef(0);
  const touched = useRef(false);
  const setDraft = (d: ScopeDraft) => {
    touched.current = true;
    setRestoreNote(null);
    setDraftState(d);
  };

  // Start with the last context choice, but only if it still works: it is previewed in the
  // background (nothing is sent), and if the session or items are gone the composer falls back to
  // No context and says so. The preview is still shown and confirmed before every send.
  useEffect(() => {
    if (!rememberedScope || rememberedScope.kind === "none") return;
    let live = true;
    companionPreviewContext(rememberedScope)
      .then(() => true)
      .catch(() => false)
      .then((available) => {
        if (!live || touched.current) return;
        const r = restoreScope(rememberedScope, available);
        setDraftState(r.draft);
        setRestoreNote(r.note ?? (available ? rememberedText(rememberedScope.kind) : null));
      });
    return () => {
      live = false;
    };
    // Only when this composer first appears: later changes belong to the user.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  const scope = buildScope(draft);
  const profile = profiles.find((p) => p.id === choice?.profileId) ?? null;
  const host = profile?.host ?? null;
  const hasKey = !!profile?.hasKey;
  const ready = !!profile && !!choice && hasKey && !!scope && text.trim().length > 0 && !sending;

  const transmit = async (confirmed: ConfirmedPreview | null) => {
    if (!scope || !profile) return;
    if (!canSend({ text, scope, confirmed, hasProfile: hasKey, sending })) return;
    const toSend = text;
    // The choice stays for the next message (it is remembered), but the preview does not: every
    // send needs a preview of the exact text, confirmed again.
    const result = await onSend(toSend, scope, confirmed?.sha256 ?? null);
    if (result.contextChanged) {
      // Nothing was stored or sent. Show what would be sent now.
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
  const isDefault = sameChoice(choice, defaultChoice(profiles));

  return (
    <div className="composer">
      {profiles.length === 0 && (
        <Banner tone="info">
          Set up a provider to start.{" "}
          <button type="button" className="btn-link" onClick={onOpenSettings}>
            Open provider settings
          </button>
        </Banner>
      )}
      {profile && !hasKey && (
        <Banner tone="attention">
          {profile.keychainOk
            ? "No key is set for this provider yet."
            : "The system keychain isn't available, so no key can be read."}{" "}
          <button type="button" className="btn-link" onClick={onOpenSettings}>
            Open provider settings
          </button>
        </Banner>
      )}

      <label className="field-label visually-hidden" htmlFor={textId}>
        Message
      </label>
      <div className="composer-box">
        <OptionBadges
          draft={draft}
          setDraft={setDraft}
          sessions={sessions}
          disabled={sending}
          onOpen={() => setOptionsOpen(true)}
        />
        <textarea
          id={textId}
          className="field textarea composer-text"
          rows={2}
          title={COMPOSER_HINT}
          maxLength={20000}
          value={text}
          readOnly={sending}
          aria-describedby={hintId}
          placeholder="Ask something…"
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            const action = composerKeyAction({
              key: e.key,
              shiftKey: e.shiftKey,
              ctrlKey: e.ctrlKey,
              metaKey: e.metaKey,
              altKey: e.altKey,
              isComposing: e.nativeEvent.isComposing,
              keyCode: e.keyCode,
            });
            if (action === "send") {
              e.preventDefault();
              submit();
            }
          }}
        />
        <div className="composer-tools">
          <button
            type="button"
            className="btn btn-quiet btn-small composer-model"
            aria-haspopup="dialog"
            title={sendsToText(host)}
            disabled={sending || profiles.length === 0}
            onClick={() => setChoosing(true)}
          >
            <span className="visually-hidden">Model: </span>
            {choiceText(profiles, choice)}
            {!isDefault && choice && <span className="composer-once"> (this message only)</span>}
          </button>
          <button
            type="button"
            className="btn btn-quiet btn-small"
            aria-haspopup="dialog"
            disabled={sending}
            onClick={() => setOptionsOpen(true)}
          >
            Options…
          </button>
        </div>
      </div>
      <span id={hintId} className="visually-hidden">
        {COMPOSER_HINT}
      </span>

      {restoreNote && (
        <p className="field-note composer-remembered" role="status">
          {restoreNote}
        </p>
      )}

      <div className="composer-actions">
        {(!scope || scope.kind !== "none") && (
          <p className="field-note composer-summary">
            {scope
              ? `This message will also send: ${scopeLabel(scope.kind).toLowerCase()}. You will see it first.`
              : "Finish choosing the context in Options, or remove it."}
          </p>
        )}
        {sending && (
          <button type="button" className="btn btn-secondary" onClick={onCancel}>
            Cancel
          </button>
        )}
        <button type="button" className="btn btn-primary" disabled={!ready} onClick={submit}>
          {sending ? "Sending…" : scope && scope.kind !== "none" ? "Review and send" : "Send"}
        </button>
      </div>

      <ModelChoiceDialog
        open={choosing}
        profiles={profiles}
        current={choice}
        onPick={onPickChoice}
        onClose={() => setChoosing(false)}
        onOpenSettings={() => {
          setChoosing(false);
          onOpenSettings();
        }}
      />

      <Dialog
        open={optionsOpen}
        title="Conversation options"
        onClose={() => setOptionsOpen(false)}
        actions={
          <button type="button" className="btn btn-primary" onClick={() => setOptionsOpen(false)}>
            Done
          </button>
        }
        description={
          <>
            <p>
              Choose what goes along with your next message. You will see the exact text before it
              is sent.
            </p>
            <p className="options-history">{historyText(historyCount, host)}</p>
          </>
        }
      >
        <ContextPicker draft={draft} onChange={setDraft} sessions={sessions} disabled={sending} />
      </Dialog>

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
                {profile ? ` (${choiceText(profiles, choice)})` : ""}. Scope:{" "}
                {scopeLabel(shown.scope.kind)}. {charCountText(shown.chars)}.
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
