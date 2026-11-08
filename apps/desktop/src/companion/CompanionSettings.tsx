// Companion settings, shown as a section of the Settings page (decision 36): providers, their
// models, keys, usage and the lookups switch. It reads the keychain, so it is only mounted while
// that section is expanded.

import { useCallback, useEffect, useId, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Banner } from "../ui/Banner";
import { Dialog } from "../ui/Dialog";
import {
  companionAddModel,
  companionClearKey,
  companionDeleteProfile,
  companionPrefs,
  companionProfiles,
  companionRemoveModel,
  companionSaveProfile,
  companionSetDefaultModel,
  companionSetKey,
  companionSetToolsEnabled,
  companionTest,
  companionUsage,
  toCommandError,
  type CompanionPrefs,
  type ProfileInput,
  type ProviderReason,
  type ProviderKind,
  type ProviderProfile,
  type Usage,
} from "./api";
import { providersChanged } from "./events";
import {
  baseUrlHint,
  baseUrlProblem,
  budgetView,
  errorText,
  KEY_MISSING_TEXT,
  KEY_SAVED_TEXT,
  KEYCHAIN_MISSING_TEXT,
  keyInputProblem,
  MODEL_HINT,
  MODEL_SUGGESTIONS,
  parseBudget,
  providerKindLabel,
  sendsToText,
  TOOLS_SETTING_HELP,
  TOOLS_SETTING_LABEL,
  UNAVAILABLE_TEXT,
} from "./logic";
import { modelIdProblem, modelsOf } from "./modelChoice";
import { ModelPickerDialog } from "./ModelPicker";
import { ProviderSaid } from "./ProviderSaid";
import "./companion.css";

type Editing = { mode: "new" } | { mode: "edit"; id: string } | null;
type Notice = {
  tone: "success" | "danger" | "info";
  text: string;
  /** What the provider said, shown apart from Lullshift's sentence. */
  reason?: ProviderReason | null;
};
type Boot =
  | { status: "loading" }
  | { status: "ready" }
  | { status: "unavailable" }
  | { status: "error"; text: string };

const KINDS: ProviderKind[] = ["anthropic", "openai", "openaiCompatible"];

function ProfileForm({
  formId,
  initial,
  onSaved,
  onBusy,
}: {
  /** The id of the <form>, so the dialog's Save button (in its footer) can submit it. */
  formId: string;
  initial: ProviderProfile | null;
  onSaved: (p: ProviderProfile) => void;
  onBusy: (busy: boolean) => void;
}) {
  const ids = useId();
  const [kind, setKind] = useState<ProviderKind>(initial?.kind ?? "anthropic");
  const [label, setLabel] = useState(initial?.label ?? providerKindLabel("anthropic"));
  const [baseUrl, setBaseUrl] = useState(
    initial?.kind === "openaiCompatible" ? initial.baseUrl : "",
  );
  const [model, setModel] = useState(initial?.model ?? "");
  const [budget, setBudget] = useState(
    initial?.monthlyTokenBudget ? String(initial.monthlyTokenBudget) : "",
  );
  const [isDefault, setIsDefault] = useState(initial?.isDefault ?? false);
  const [problems, setProblems] = useState<string[]>([]);
  const [picking, setPicking] = useState(false);
  const compatible = kind === "openaiCompatible";

  const submit = async () => {
    const found: string[] = [];
    if (!label.trim()) found.push("Give the provider a name you will recognise.");
    if (!initial) {
      const p = modelIdProblem(model);
      if (p) found.push(p);
    }
    if (compatible) {
      const p = baseUrlProblem(baseUrl);
      if (p) found.push(p);
    }
    const parsed = parseBudget(budget);
    if (parsed === "invalid") found.push("The budget must be a whole number of tokens, or empty.");
    setProblems(found);
    if (found.length > 0 || parsed === "invalid") return;
    const input: ProfileInput = {
      id: initial?.id,
      kind,
      label: label.trim(),
      baseUrl: compatible ? baseUrl.trim() : undefined,
      // An edit keeps the default model; models are changed in the provider's model list.
      model: initial ? initial.model : model.trim(),
      monthlyTokenBudget: parsed,
      isDefault,
    };
    onBusy(true);
    try {
      onSaved(await companionSaveProfile(input));
    } catch (err) {
      setProblems([errorText(toCommandError(err))]);
    } finally {
      onBusy(false);
    }
  };

  return (
    <form
      id={formId}
      className="profile-form"
      onSubmit={(e) => {
        e.preventDefault();
        void submit();
      }}
    >
      <label className="field-label" htmlFor={`${ids}-kind`}>
        Provider
      </label>
      <select
        id={`${ids}-kind`}
        className="field"
        value={kind}
        disabled={initial !== null}
        onChange={(e) => {
          const next = e.target.value as ProviderKind;
          if (label === providerKindLabel(kind)) setLabel(providerKindLabel(next));
          setKind(next);
        }}
      >
        {KINDS.map((k) => (
          <option key={k} value={k}>
            {providerKindLabel(k)}
          </option>
        ))}
      </select>

      <label className="field-label" htmlFor={`${ids}-label`}>
        Name
      </label>
      <input
        id={`${ids}-label`}
        className="field"
        type="text"
        maxLength={60}
        value={label}
        onChange={(e) => setLabel(e.target.value)}
      />

      {compatible && (
        <>
          <label className="field-label" htmlFor={`${ids}-url`}>
            Base URL
          </label>
          <input
            id={`${ids}-url`}
            className="field"
            type="text"
            inputMode="url"
            autoComplete="off"
            spellCheck={false}
            aria-describedby={`${ids}-url-hint`}
            placeholder="https://example.com/v1"
            value={baseUrl}
            onChange={(e) => setBaseUrl(e.target.value)}
          />
          <p id={`${ids}-url-hint`} className="field-note">
            {baseUrlHint(kind)}
          </p>
        </>
      )}

      {!initial && (
        <>
          <label className="field-label" htmlFor={`${ids}-model`}>
            First model
          </label>
          <div className="picker-typed-row">
            <input
              id={`${ids}-model`}
              className="field"
              type="text"
              autoComplete="off"
              spellCheck={false}
              list={`${ids}-models`}
              aria-describedby={`${ids}-model-hint`}
              value={model}
              onChange={(e) => setModel(e.target.value)}
            />
            <button type="button" className="btn btn-secondary" onClick={() => setPicking(true)}>
              Choose from list…
            </button>
          </div>
          <datalist id={`${ids}-models`}>
            {MODEL_SUGGESTIONS[kind].map((m) => (
              <option key={m} value={m} />
            ))}
          </datalist>
          <p id={`${ids}-model-hint`} className="field-note">
            {MODEL_HINT} You can add more models after saving.
          </p>
          {picking &&
            // In the document body, not inside the form: Enter in the picker must not save it.
            createPortal(
              // Synthetic events cross portals: a submit inside the picker must never reach this form.
              <div onSubmit={(e) => e.stopPropagation()}>
                <ModelPickerDialog
                  kind={kind}
                  title="Choose the first model"
                  chosen={model.trim()}
                  onChoose={(m) => {
                    setModel(m.model);
                    setPicking(false);
                  }}
                  onClose={() => setPicking(false)}
                />
              </div>,
              document.body,
            )}
        </>
      )}

      <label className="field-label" htmlFor={`${ids}-budget`}>
        Monthly token budget (optional)
      </label>
      <input
        id={`${ids}-budget`}
        className="field"
        type="text"
        inputMode="numeric"
        autoComplete="off"
        aria-describedby={`${ids}-budget-hint`}
        placeholder="No budget"
        value={budget}
        onChange={(e) => setBudget(e.target.value)}
      />
      <p id={`${ids}-budget-hint`} className="field-note">
        Sending stops when input plus output tokens reach this number, until next month or until you
        raise it. It covers every model of this provider. Counts come from the provider's own
        replies.
      </p>

      <label className="pick">
        <input
          type="checkbox"
          checked={isDefault}
          onChange={(e) => setIsDefault(e.target.checked)}
        />
        <span>Use this provider by default</span>
      </label>

      {problems.length > 0 && (
        <Banner tone="danger">
          <ul className="problem-list">
            {problems.map((p) => (
              <li key={p}>{p}</li>
            ))}
          </ul>
        </Banner>
      )}
    </form>
  );
}

/**
 * Adding or editing a provider happens in a modal dialog: focus is trapped, Esc closes it, and
 * focus goes back to the button that opened it. After a new provider is saved the dialog stays
 * open in edit mode so its key can be pasted right away.
 */
function ProviderDialog({
  editing,
  profile,
  onClose,
  onSaved,
  onKeyChanged,
  say,
}: {
  editing: Editing;
  profile: ProviderProfile | null;
  onClose: () => void;
  onSaved: (p: ProviderProfile) => void;
  onKeyChanged: () => Promise<void>;
  say: (n: Notice) => void;
}) {
  const formId = useId();
  const [busy, setBusy] = useState(false);
  // The page behind a modal dialog can't be seen, so what the key panel says is shown here too.
  const [local, setLocal] = useState<Notice | null>(null);
  const [keyBusy, setKeyBusy] = useState(false);
  const [keyDirty, setKeyDirty] = useState(false);
  // False once the dialog is closed, so a key request that answers later says nothing on the
  // next opening.
  const open = useRef(false);
  useEffect(() => {
    open.current = editing !== null;
  }, [editing]);
  const close = () => {
    setLocal(null);
    setKeyDirty(false);
    setKeyBusy(false);
    onClose();
  };
  return (
    // Always rendered and opened by `editing`, so closing it returns focus to the button that
    // opened it. The form inside is mounted only while open, so each opening starts fresh.
    <Dialog
      open={editing !== null}
      title={profile ? `Edit ${profile.label}` : "Add a provider"}
      busy={busy || keyBusy}
      onClose={close}
      actions={
        <>
          <button type="button" className="btn btn-quiet" disabled={busy} onClick={close}>
            {profile ? "Close" : "Cancel"}
          </button>
          <button type="submit" form={formId} className="btn btn-primary" disabled={busy}>
            {busy ? "Saving…" : "Save provider"}
          </button>
        </>
      }
    >
      {editing && (
        <div className="provider-dialog">
          <ProfileForm
            key={editing.mode === "edit" ? editing.id : "new"}
            formId={formId}
            initial={profile}
            onSaved={onSaved}
            onBusy={setBusy}
          />
          {profile && (
            <KeyPanel
              profile={profile}
              onChanged={onKeyChanged}
              onBusy={setKeyBusy}
              onDirty={setKeyDirty}
              say={(n) => {
                if (open.current) setLocal(n);
                say(n);
              }}
            />
          )}
          {keyDirty && <p className="field-note">Key not saved yet. Press Save key to keep it.</p>}
          <div role="status" aria-live="polite">
            {local && (
              <Banner tone={local.tone}>
                {local.text}
                <ProviderSaid reason={local.reason} />
              </Banner>
            )}
          </div>
        </div>
      )}
    </Dialog>
  );
}

/** A provider's models: add (from the list or typed), make default, remove. */
export function ModelList({
  profile,
  onChanged,
  say,
}: {
  profile: ProviderProfile;
  onChanged: (p: ProviderProfile) => void;
  say: (n: Notice) => void;
}) {
  const id = useId();
  const [picking, setPicking] = useState(false);
  const [busy, setBusy] = useState(false);
  const models = modelsOf(profile);

  const act = async (work: () => Promise<ProviderProfile>, done: string) => {
    setBusy(true);
    try {
      onChanged(await work());
      say({ tone: "success", text: done });
    } catch (err) {
      say({ tone: "danger", text: errorText(toCommandError(err), profile.host) });
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="model-list" aria-labelledby={`${id}-h`} role="group">
      <h4 className="model-list-title" id={`${id}-h`}>
        Models of {profile.label}
      </h4>
      <ul className="model-rows">
        {models.map((m) => {
          const name = m.label ?? m.model;
          return (
            <li key={m.model} className="model-row">
              <span className="model-row-name">
                {name}
                {m.label && <code className="model-row-id">{m.model}</code>}
              </span>
              {m.isDefault ? (
                <span className="pill tone-neutral">Default</span>
              ) : (
                <button
                  type="button"
                  className="btn btn-quiet btn-small"
                  disabled={busy}
                  aria-label={`Make ${name} the default model of ${profile.label}`}
                  onClick={() =>
                    void act(
                      () => companionSetDefaultModel(profile.id, m.model),
                      `${name} is now the default model of ${profile.label}.`,
                    )
                  }
                >
                  Make default
                </button>
              )}
              <button
                type="button"
                className="btn btn-quiet btn-small"
                disabled={busy || models.length <= 1}
                aria-label={`Remove ${name} from ${profile.label}`}
                title={models.length <= 1 ? "A provider keeps at least one model." : undefined}
                onClick={() =>
                  void act(
                    () => companionRemoveModel(profile.id, m.model),
                    `${name} was removed from ${profile.label}.`,
                  )
                }
              >
                Remove
              </button>
            </li>
          );
        })}
      </ul>
      <button
        type="button"
        className="btn btn-secondary"
        disabled={busy}
        onClick={() => setPicking(true)}
      >
        Add a model…
      </button>
      {picking && (
        <ModelPickerDialog
          kind={profile.kind}
          title={`Add a model to ${profile.label}`}
          chosen=""
          onChoose={(m) => {
            setPicking(false);
            void act(
              () =>
                companionAddModel(profile.id, m.model, m.name ? m.name.slice(0, 80) : null, false),
              `${m.name ?? m.model} was added to ${profile.label}.`,
            );
          }}
          onClose={() => setPicking(false)}
        />
      )}
    </div>
  );
}

function KeyPanel({
  profile,
  onChanged,
  onBusy,
  onDirty,
  say,
}: {
  profile: ProviderProfile;
  onChanged: () => Promise<void>;
  /** A key request is running (the dialog must not close meanwhile). */
  onBusy?: (busy: boolean) => void;
  /** The key field has text that is not saved yet. Only the fact, never the text. */
  onDirty?: (dirty: boolean) => void;
  say: (n: Notice) => void;
}) {
  const inputId = useId();
  // The key lives in this one input state only until the button is pressed; it is cleared
  // before the request is made and is never shown, logged or stored anywhere else.
  const [key, setKey] = useState("");
  const [fieldProblem, setFieldProblem] = useState<string | null>(null);
  const [busy, setBusyState] = useState<"save" | "remove" | "test" | null>(null);
  const setBusy = (b: "save" | "remove" | "test" | null) => {
    setBusyState(b);
    onBusy?.(b !== null);
  };
  const [result, setResult] = useState<string | null>(null);

  const saveKey = async () => {
    const problem = keyInputProblem(key);
    if (problem) {
      setFieldProblem(problem);
      return;
    }
    const toSend = key;
    setKey("");
    onDirty?.(false);
    setFieldProblem(null);
    setBusy("save");
    try {
      await companionSetKey(profile.id, toSend);
      await onChanged();
      say({ tone: "success", text: `${KEY_SAVED_TEXT}.` });
    } catch (err) {
      say({
        tone: "danger",
        text: `The key wasn't saved. ${errorText(toCommandError(err), profile.host)} Paste it again to retry.`,
      });
    } finally {
      setBusy(null);
    }
  };

  const removeKey = async () => {
    setBusy("remove");
    try {
      await companionClearKey(profile.id);
      await onChanged();
      say({ tone: "success", text: "Key removed from your system keychain." });
    } catch (err) {
      say({ tone: "danger", text: errorText(toCommandError(err), profile.host) });
    } finally {
      setBusy(null);
    }
  };

  const test = async () => {
    setBusy("test");
    setResult(null);
    try {
      const r = await companionTest(profile.id);
      setResult(`Connected to ${profile.host}. It answered using ${r.model}.`);
    } catch (err) {
      const e = toCommandError(err);
      say({ tone: "danger", text: errorText(e, profile.host), reason: e.providerReason });
    } finally {
      setBusy(null);
    }
  };

  const status = !profile.keychainOk
    ? "Keychain unavailable"
    : profile.hasKey
      ? KEY_SAVED_TEXT
      : KEY_MISSING_TEXT;

  return (
    <div className="card key-panel">
      <h4 className="card-title">Key for {profile.label}</h4>
      <p className="key-status" role="status">
        {status}
      </p>
      {!profile.keychainOk && <Banner tone="attention">{KEYCHAIN_MISSING_TEXT}</Banner>}
      <label className="field-label" htmlFor={inputId}>
        {profile.hasKey ? "Replace key" : "Paste key"}
      </label>
      <input
        id={inputId}
        className="field"
        type="password"
        name="companion-provider-credential"
        autoComplete="off"
        autoCorrect="off"
        autoCapitalize="off"
        spellCheck={false}
        disabled={!profile.keychainOk || busy !== null}
        aria-invalid={fieldProblem ? true : undefined}
        aria-describedby={`${inputId}-hint`}
        value={key}
        onChange={(e) => {
          setKey(e.target.value);
          onDirty?.(e.target.value.length > 0);
        }}
      />
      <p id={`${inputId}-hint`} className={fieldProblem ? "field-note field-error" : "field-note"}>
        {fieldProblem ??
          "It goes straight to your system keychain and is never shown again. Companion can't read it back."}
      </p>
      <div className="actions">
        <button
          type="button"
          className="btn btn-primary"
          disabled={!profile.keychainOk || busy !== null || key.length === 0}
          onClick={() => void saveKey()}
        >
          {busy === "save" ? "Saving…" : "Save key"}
        </button>
        <button
          type="button"
          className="btn btn-secondary"
          disabled={!profile.hasKey || busy !== null}
          onClick={() => void removeKey()}
        >
          Remove key
        </button>
        <button
          type="button"
          className="btn btn-secondary"
          disabled={!profile.hasKey || busy !== null}
          onClick={() => void test()}
        >
          {busy === "test" ? "Testing…" : "Test connection"}
        </button>
      </div>
      <p className="field-note">
        Test connection makes one small request to {profile.host} with the default model, only when
        you press it.
      </p>
      {result && (
        <div role="status">
          <Banner tone="success">{result}</Banner>
        </div>
      )}
    </div>
  );
}

/** The switch for read-only lookups. Off by default; every single lookup still needs approval. */
export function ToolsSetting({
  enabled,
  onChanged,
  say,
}: {
  enabled: boolean;
  onChanged: (prefs: CompanionPrefs) => void;
  say: (n: Notice) => void;
}) {
  const id = useId();
  const [busy, setBusy] = useState(false);
  const toggle = async (next: boolean) => {
    setBusy(true);
    try {
      onChanged(await companionSetToolsEnabled(next));
      say({
        tone: "success",
        text: next
          ? "Lookups are on. You will be asked about each one."
          : "Lookups are off. Requests carry no tools.",
      });
    } catch (err) {
      say({ tone: "danger", text: errorText(toCommandError(err)) });
    } finally {
      setBusy(false);
    }
  };
  return (
    <section className="tools-setting" aria-labelledby={`${id}-h`}>
      <h3 className="section-title" id={`${id}-h`}>
        Lookups
      </h3>
      <label className="check-row" htmlFor={id}>
        <input
          id={id}
          type="checkbox"
          checked={enabled}
          disabled={busy}
          onChange={(e) => void toggle(e.target.checked)}
        />{" "}
        {TOOLS_SETTING_LABEL}
      </label>
      <p className="field-note">{TOOLS_SETTING_HELP}</p>
    </section>
  );
}

/** Providers, models, keys, usage and lookups. Reads everything itself from the core. */
export function CompanionSettings() {
  const [boot, setBoot] = useState<Boot>({ status: "loading" });
  const [profiles, setProfiles] = useState<ProviderProfile[]>([]);
  const [toolsEnabled, setToolsEnabled] = useState(false);
  const [editing, setEditing] = useState<Editing>(null);
  const [notice, setNotice] = useState<Notice | null>(null);
  const [usage, setUsage] = useState<Usage[] | null>(null);
  const [usageError, setUsageError] = useState<string | null>(null);
  const [deleting, setDeleting] = useState<ProviderProfile | null>(null);

  const failed = (err: unknown) => {
    const e = toCommandError(err);
    setBoot(
      e.kind === "unavailable"
        ? { status: "unavailable" }
        : { status: "error", text: errorText(e) },
    );
  };

  const loadUsage = useCallback(async () => {
    try {
      setUsage(await companionUsage());
      setUsageError(null);
    } catch (err) {
      setUsageError(errorText(toCommandError(err)));
    }
  }, []);

  const reload = useCallback(async () => {
    try {
      setProfiles(await companionProfiles());
      setBoot({ status: "ready" });
    } catch (err) {
      failed(err);
    }
  }, []);

  useEffect(() => {
    let live = true;
    companionProfiles()
      .then((p) => {
        if (!live) return;
        setProfiles(p);
        setBoot({ status: "ready" });
      })
      .catch((err: unknown) => {
        if (live) failed(err);
      });
    companionPrefs()
      .then((p) => {
        if (live) setToolsEnabled(p.toolsEnabled);
      })
      .catch(() => undefined);
    companionUsage()
      .then((u) => {
        if (live) setUsage(u);
      })
      .catch((err: unknown) => {
        if (live) setUsageError(errorText(toCommandError(err)));
      });
    return () => {
      live = false;
    };
  }, []);

  const changed = async () => {
    await reload();
    await loadUsage();
    providersChanged();
  };

  const replace = (saved: ProviderProfile) => {
    setProfiles((cur) =>
      cur.some((p) => p.id === saved.id)
        ? cur.map((p) => (p.id === saved.id ? saved : p))
        : [...cur, saved],
    );
    providersChanged();
  };

  const editingProfile =
    editing?.mode === "edit" ? (profiles.find((p) => p.id === editing.id) ?? null) : null;
  const keychainDown = profiles.some((p) => !p.keychainOk);

  const confirmDelete = async () => {
    const p = deleting;
    setDeleting(null);
    if (!p) return;
    try {
      await companionDeleteProfile(p.id);
      if (editing?.mode === "edit" && editing.id === p.id) setEditing(null);
      await changed();
      setNotice({ tone: "success", text: `${p.label}, its models and its key were removed.` });
    } catch (err) {
      setNotice({ tone: "danger", text: errorText(toCommandError(err), p.host) });
    }
  };

  if (boot.status === "loading") return <p className="card-sub">Loading Companion settings…</p>;
  if (boot.status === "unavailable") return <Banner tone="info">{UNAVAILABLE_TEXT}</Banner>;
  if (boot.status === "error") {
    return (
      <Banner tone="danger">
        {boot.text}{" "}
        <button type="button" className="btn-link" onClick={() => void reload()}>
          Try again
        </button>
      </Banner>
    );
  }

  return (
    <div className="companion-settings">
      <div role="status" aria-live="polite">
        {notice && (
          <Banner tone={notice.tone}>
            {notice.text}
            <ProviderSaid reason={notice.reason} />
          </Banner>
        )}
      </div>
      {keychainDown && <Banner tone="attention">{KEYCHAIN_MISSING_TEXT}</Banner>}

      <h3 className="section-title">Providers</h3>
      <p className="field-note">
        Each message goes to the default model of the default provider unless you pick another one
        for that message in the conversation.
      </p>
      {profiles.length === 0 && (
        <p className="card-sub">
          No provider yet. Companion talks only to a provider you add, using your own key.
        </p>
      )}
      <ul className="profile-list">
        {profiles.map((p) => {
          const u = usage?.find((x) => x.profileId === p.id) ?? null;
          const b = budgetView(u, p.monthlyTokenBudget);
          return (
            <li key={p.id} className="card profile-card">
              <div className="profile-head">
                <div>
                  <p className="card-title">
                    {p.label}
                    {p.isDefault && <span className="pill tone-neutral"> Default</span>}
                  </p>
                  <p className="card-sub">
                    {providerKindLabel(p.kind)} · {sendsToText(p.host)}
                  </p>
                  <p className="card-sub">
                    {p.keychainOk
                      ? p.hasKey
                        ? KEY_SAVED_TEXT
                        : KEY_MISSING_TEXT
                      : "Keychain unavailable"}
                  </p>
                </div>
                <div className="profile-buttons">
                  <button
                    type="button"
                    className="btn btn-secondary"
                    aria-label={`Edit ${p.label}`}
                    onClick={() => setEditing({ mode: "edit", id: p.id })}
                  >
                    Edit
                  </button>
                  <button
                    type="button"
                    className="btn btn-danger"
                    aria-label={`Remove ${p.label}`}
                    onClick={() => setDeleting(p)}
                  >
                    Remove
                  </button>
                </div>
              </div>
              <ModelList profile={p} onChanged={replace} say={setNotice} />
              <div className="usage">
                <p>{usageError && !u ? "This month's usage couldn't be read." : b.line}</p>
                {b.fraction !== null && (
                  <progress
                    className={`usage-bar usage-${b.state}`}
                    max={100}
                    value={Math.round(b.fraction * 100)}
                    aria-label={`Budget used for ${p.label}`}
                  />
                )}
              </div>
            </li>
          );
        })}
      </ul>

      <div className="actions">
        <button
          type="button"
          className="btn btn-primary"
          onClick={() => setEditing({ mode: "new" })}
        >
          Add a provider
        </button>
      </div>

      <ProviderDialog
        editing={editing}
        profile={editingProfile}
        onClose={() => setEditing(null)}
        onSaved={(saved) => {
          setNotice({ tone: "success", text: `${saved.label} was saved.` });
          setEditing({ mode: "edit", id: saved.id });
          replace(saved);
          void changed();
        }}
        onKeyChanged={changed}
        say={setNotice}
      />

      <ToolsSetting
        enabled={toolsEnabled}
        onChanged={(p) => {
          setToolsEnabled(p.toolsEnabled);
          providersChanged();
        }}
        say={setNotice}
      />

      <Dialog
        open={deleting !== null}
        title="Remove this provider?"
        onClose={() => setDeleting(null)}
        actions={
          <>
            <button
              type="button"
              className="btn btn-quiet"
              autoFocus
              onClick={() => setDeleting(null)}
            >
              Keep it
            </button>
            <button type="button" className="btn btn-danger" onClick={() => void confirmDelete()}>
              Remove and delete its key
            </button>
          </>
        }
        description={
          <p>
            The provider, its models and its key in your system keychain are deleted. Conversations
            stay, but they can't be continued with this provider.
          </p>
        }
      />
    </div>
  );
}
