import { useCallback, useEffect, useId, useState } from "react";
import { Banner } from "../ui/Banner";
import { Dialog } from "../ui/Dialog";
import {
  companionClearKey,
  companionDeleteProfile,
  companionSaveProfile,
  companionSetKey,
  companionTest,
  companionUsage,
  toCommandError,
  type ProfileInput,
  type ProviderKind,
  type ProviderProfile,
  type Usage,
} from "./api";
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
} from "./logic";

interface Props {
  profiles: readonly ProviderProfile[];
  onProfilesChanged: (profiles: ProviderProfile[]) => void;
  /** Re-reads profiles and threads from the core. */
  onReload: () => Promise<void>;
  onDone: () => void;
}

type Editing = { mode: "new" } | { mode: "edit"; id: string } | null;
type Notice = { tone: "success" | "danger" | "info"; text: string };

const KINDS: ProviderKind[] = ["anthropic", "openai", "openaiCompatible"];

function ProfileForm({
  initial,
  onSaved,
  onCancel,
}: {
  initial: ProviderProfile | null;
  onSaved: (p: ProviderProfile) => void;
  onCancel: () => void;
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
  const [busy, setBusy] = useState(false);
  const compatible = kind === "openaiCompatible";

  const submit = async () => {
    const found: string[] = [];
    if (!label.trim()) found.push("Give the provider a name you will recognise.");
    if (!model.trim()) found.push("Enter the model name.");
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
      model: model.trim(),
      monthlyTokenBudget: parsed,
      isDefault,
    };
    setBusy(true);
    try {
      onSaved(await companionSaveProfile(input));
    } catch (err) {
      setProblems([errorText(toCommandError(err))]);
    } finally {
      setBusy(false);
    }
  };

  return (
    <form
      className="card profile-form"
      onSubmit={(e) => {
        e.preventDefault();
        void submit();
      }}
    >
      <h3 className="card-title">{initial ? "Edit provider" : "Add a provider"}</h3>

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

      <label className="field-label" htmlFor={`${ids}-model`}>
        Model
      </label>
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
      <datalist id={`${ids}-models`}>
        {MODEL_SUGGESTIONS[kind].map((m) => (
          <option key={m} value={m} />
        ))}
      </datalist>
      <p id={`${ids}-model-hint`} className="field-note">
        {MODEL_HINT}
      </p>

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
        raise it. Counts come from the provider's own replies.
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

      <div className="actions">
        <button type="submit" className="btn btn-primary" disabled={busy}>
          {busy ? "Saving…" : "Save provider"}
        </button>
        <button type="button" className="btn btn-quiet" disabled={busy} onClick={onCancel}>
          Cancel
        </button>
      </div>
    </form>
  );
}

function KeyPanel({
  profile,
  onChanged,
  say,
}: {
  profile: ProviderProfile;
  onChanged: () => Promise<void>;
  say: (n: Notice) => void;
}) {
  const inputId = useId();
  // The key lives in this one input state only until the button is pressed; it is cleared
  // before the request is made and is never shown, logged or stored anywhere else.
  const [key, setKey] = useState("");
  const [fieldProblem, setFieldProblem] = useState<string | null>(null);
  const [busy, setBusy] = useState<"save" | "remove" | "test" | null>(null);
  const [result, setResult] = useState<string | null>(null);

  const saveKey = async () => {
    const problem = keyInputProblem(key);
    if (problem) {
      setFieldProblem(problem);
      return;
    }
    const toSend = key;
    setKey("");
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
      say({ tone: "danger", text: errorText(toCommandError(err), profile.host) });
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
      <h3 className="card-title">Key for {profile.label}</h3>
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
        onChange={(e) => setKey(e.target.value)}
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
        Test connection makes one small request to {profile.host}, only when you press it.
      </p>
      {result && (
        <div role="status">
          <Banner tone="success">{result}</Banner>
        </div>
      )}
    </div>
  );
}

/** Provider profiles, keys, usage. Companion's settings live here rather than in Settings so the
 * key, the host and the budget are next to the conversation they apply to. */
export function CompanionSettings({ profiles, onProfilesChanged, onReload, onDone }: Props) {
  const [editing, setEditing] = useState<Editing>(null);
  const [notice, setNotice] = useState<Notice | null>(null);
  const [usage, setUsage] = useState<Usage[] | null>(null);
  const [usageError, setUsageError] = useState<string | null>(null);
  const [deleting, setDeleting] = useState<ProviderProfile | null>(null);

  const loadUsage = useCallback(async () => {
    try {
      setUsage(await companionUsage());
      setUsageError(null);
    } catch (err) {
      setUsageError(errorText(toCommandError(err)));
    }
  }, []);

  useEffect(() => {
    let live = true;
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
    await onReload();
    await loadUsage();
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
      setNotice({ tone: "success", text: `${p.label} and its key were removed.` });
    } catch (err) {
      setNotice({ tone: "danger", text: errorText(toCommandError(err), p.host) });
    }
  };

  return (
    <div className="companion-settings">
      <div role="status" aria-live="polite">
        {notice && <Banner tone={notice.tone}>{notice.text}</Banner>}
      </div>
      {keychainDown && <Banner tone="attention">{KEYCHAIN_MISSING_TEXT}</Banner>}

      <h2 className="section-title">Providers</h2>
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
                    {providerKindLabel(p.kind)} · {p.model || "no model set"} ·{" "}
                    {sendsToText(p.host)}
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

      {editing === null && (
        <div className="actions">
          <button
            type="button"
            className="btn btn-primary"
            onClick={() => setEditing({ mode: "new" })}
          >
            Add a provider
          </button>
          <button type="button" className="btn btn-quiet" onClick={onDone}>
            Back to conversations
          </button>
        </div>
      )}

      {editing && (
        <ProfileForm
          key={editing.mode === "edit" ? editing.id : "new"}
          initial={editingProfile}
          onCancel={() => setEditing(null)}
          onSaved={(saved) => {
            setNotice({ tone: "success", text: `${saved.label} was saved.` });
            setEditing({ mode: "edit", id: saved.id });
            onProfilesChanged(
              profiles.some((p) => p.id === saved.id)
                ? profiles.map((p) => (p.id === saved.id ? saved : p))
                : [...profiles, saved],
            );
            void changed();
          }}
        />
      )}

      {editingProfile && <KeyPanel profile={editingProfile} onChanged={changed} say={setNotice} />}

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
            The provider profile and its key in your system keychain are deleted. Conversations
            stay, but they can't be continued with this provider.
          </p>
        }
      />
    </div>
  );
}
