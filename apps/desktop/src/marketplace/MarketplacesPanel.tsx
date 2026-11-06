import { useCallback, useEffect, useState } from "react";
import { relativeTime } from "../format";
import { Banner } from "../ui/Banner";
import { Dialog } from "../ui/Dialog";
import { Icon } from "../ui/Icon";
import {
  marketplaceAdd,
  marketplaceEntries,
  marketplaceList,
  marketplaceRefresh,
  marketplaceRemove,
  toCommandError,
  type Marketplace,
  type MarketplaceEntry,
  type MovedTo,
} from "./api";
import {
  canInstall,
  checkGitInput,
  entryStatus,
  entryTitle,
  errorText,
  fetchedLine,
  kindLabel,
  looksPrivate,
  PRIVATE_HOST_LABEL,
  PRIVATE_HOST_NOTE,
  PRIVATE_HOST_TEXT,
  truncate,
  UNAVAILABLE_TEXT,
} from "./logic";

/** The explicit "on my own computer or network" confirmation (decision 17). Never pre-ticked. */
export function PrivateHostBox({
  id,
  checked,
  disabled,
  onChange,
}: {
  id: string;
  checked: boolean;
  disabled: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <div className="mk-private">
      <label className="mk-check" htmlFor={id}>
        <input
          id={id}
          type="checkbox"
          checked={checked}
          disabled={disabled}
          aria-describedby={`${id}-note`}
          onChange={(e) => onChange(e.target.checked)}
        />
        <span>{PRIVATE_HOST_LABEL}</span>
      </label>
      <p id={`${id}-note`} className="field-note">
        {PRIVATE_HOST_NOTE}
      </p>
    </div>
  );
}
import { ReviewDialog, type ReviewRequest } from "./ReviewDialog";

type ListState =
  | { status: "loading" }
  | { status: "ready"; list: Marketplace[] }
  | { status: "unavailable" }
  | { status: "error"; text: string };

type EntriesState =
  | { status: "ready"; id: string; entries: MarketplaceEntry[] }
  | { status: "error"; id: string; text: string };

interface Props {
  now: number;
  /** Something was installed, updated or removed: the library should re-read. */
  onChanged: () => void;
}

/** Add, refresh, remove and browse Git marketplaces. Network only on the user's action. */
export function MarketplacesPanel({ now, onChanged }: Props) {
  const [list, setList] = useState<ListState>({ status: "loading" });
  const [selected, setSelected] = useState<string | null>(null);
  const [entries, setEntries] = useState<EntriesState | null>(null);
  const [input, setInput] = useState("");
  const [inputError, setInputError] = useState<string | null>(null);
  /** The "on my own computer or network" box (decision 17). */
  const [privateOk, setPrivateOk] = useState(false);
  /** The backend said this address resolves to a private one, so the box must be shown. */
  const [privateAsked, setPrivateAsked] = useState(false);
  /** A refresh answered `privateHost`: the box shown under that marketplace. */
  const [refreshPrivate, setRefreshPrivate] = useState<{ id: string; ok: boolean } | null>(null);
  /**
   * A redirect Lullshift reported instead of following (decision 18.1). `context` is "add" or
   * the id of the marketplace that was refreshed.
   */
  const [moved, setMoved] = useState<{
    to: MovedTo;
    context: string;
    confirmPrivate: boolean;
  } | null>(null);
  /** Which marketplace is busy; "add" while adding. One git operation at a time per UI. */
  const [busy, setBusy] = useState<string | null>(null);
  const [message, setMessage] = useState<{ tone: "success" | "danger"; text: string } | null>(null);
  const [live, setLive] = useState("");
  const [removing, setRemoving] = useState<Marketplace | null>(null);
  const [review, setReview] = useState<ReviewRequest | null>(null);

  const say = useCallback((tone: "success" | "danger", text: string) => {
    setMessage({ tone, text });
    setLive(text);
  }, []);

  const loadList = useCallback(() => {
    return marketplaceList()
      .then((l) => setList({ status: "ready", list: l }))
      .catch((err) => {
        const e = toCommandError(err);
        setList(
          e.kind === "unavailable"
            ? { status: "unavailable" }
            : { status: "error", text: errorText(e) },
        );
      });
  }, []);

  useEffect(() => {
    void loadList();
  }, [loadList]);

  const loadEntries = useCallback((id: string) => {
    return marketplaceEntries(id)
      .then((e) => setEntries({ status: "ready", id, entries: e }))
      .catch((err) => setEntries({ status: "error", id, text: errorText(toCommandError(err)) }));
  }, []);

  useEffect(() => {
    if (selected) void loadEntries(selected);
  }, [selected, loadEntries]);

  // Entries belong to the marketplace they were loaded for; until then this one is loading.
  const shown = entries && entries.id === selected ? entries : null;
  const check = checkGitInput(input);
  const hint = input.trim() === "" ? null : check.ok ? check.hint : check.reason;
  const showPrivate = check.ok && (privateAsked || looksPrivate(check.value));

  const addAddress = async (value: string, confirmPrivate: boolean) => {
    setInputError(null);
    setMessage(null);
    setMoved(null);
    setBusy("add");
    setLive("Adding marketplace. Fetching its list with your Git access…");
    try {
      const m = await marketplaceAdd(value, confirmPrivate);
      setInput("");
      setPrivateOk(false);
      setPrivateAsked(false);
      say("success", `${truncate(m.id, 60)} was added.`);
      await loadList();
      setSelected(m.id);
    } catch (err) {
      const e = toCommandError(err);
      if (e.kind === "privateHost") setPrivateAsked(true);
      if (e.kind === "moved" && e.moved) setMoved({ to: e.moved, context: "add", confirmPrivate });
      say("danger", errorText(e));
    } finally {
      setBusy(null);
    }
  };

  const add = async () => {
    const c = checkGitInput(input);
    if (!c.ok) {
      setInputError(c.reason);
      return;
    }
    if ((privateAsked || looksPrivate(c.value)) && !privateOk) {
      setInputError(PRIVATE_HOST_TEXT);
      return;
    }
    await addAddress(c.value, showPrivate && privateOk);
  };

  const refresh = async (m: Marketplace, confirmPrivate = false) => {
    setMessage(null);
    setMoved(null);
    setBusy(m.id);
    setLive(`Refreshing ${truncate(m.id, 60)}…`);
    try {
      await marketplaceRefresh(m.id, confirmPrivate);
      setRefreshPrivate(null);
      say("success", `${truncate(m.id, 60)} is up to date.`);
      await loadList();
      if (selected === m.id) await loadEntries(m.id);
    } catch (err) {
      const e = toCommandError(err);
      // Its name now resolves to a private address: ask, never assume (decision 18.8).
      if (e.kind === "privateHost") setRefreshPrivate({ id: m.id, ok: false });
      if (e.kind === "moved" && e.moved) {
        setMoved({ to: e.moved, context: m.id, confirmPrivate: m.privateHost });
      }
      say("danger", errorText(e));
    } finally {
      setBusy(null);
    }
  };

  const remove = async () => {
    const m = removing;
    if (!m) return;
    setBusy(m.id);
    try {
      await marketplaceRemove(m.id);
      setRemoving(null);
      if (selected === m.id) setSelected(null);
      say("success", `${truncate(m.id, 60)} was removed. Activities you installed from it stay.`);
      await loadList();
      onChanged();
    } catch (err) {
      setRemoving(null);
      say("danger", errorText(toCommandError(err)));
    } finally {
      setBusy(null);
    }
  };

  if (list.status === "unavailable") {
    return <Banner tone="info">{UNAVAILABLE_TEXT}</Banner>;
  }

  const current = list.status === "ready" ? list.list.find((m) => m.id === selected) : undefined;

  return (
    <div className="mk">
      <p className="visually-hidden" role="status" aria-live="polite">
        {live}
      </p>
      {message && <Banner tone={message.tone}>{message.text}</Banner>}
      {moved && (
        <div className="card mk-moved" role="group" aria-label="Repository moved">
          <p className="card-text">
            {moved.to.sameHost
              ? "The server says this repository is now at:"
              : "The server points to an address on a different server:"}
          </p>
          <p className="mk-meta mk-wrap">
            <code>{truncate(moved.to.url, 300)}</code>
          </p>
          <div className="activity-actions">
            <button
              type="button"
              className="btn btn-quiet"
              disabled={busy !== null}
              onClick={() => setMoved(null)}
            >
              Dismiss
            </button>
            {moved.context === "add" && moved.to.sameHost && (
              <button
                type="button"
                className="btn btn-primary"
                disabled={busy !== null}
                onClick={() => void addAddress(moved.to.url, moved.confirmPrivate)}
              >
                Use the new address
              </button>
            )}
            {!moved.to.sameHost && (
              <button
                type="button"
                className="btn btn-secondary"
                disabled={busy !== null}
                onClick={() => {
                  // Nothing is fetched: the user reviews it and adds it deliberately.
                  setInput(moved.to.url);
                  setPrivateOk(false);
                  setPrivateAsked(false);
                  setMoved(null);
                }}
              >
                Put it in the address box
              </button>
            )}
          </div>
        </div>
      )}

      <section className="card" aria-labelledby="mk-add-title">
        <h2 className="card-title" id="mk-add-title">
          Add a marketplace
        </h2>
        <p className="card-text">
          A marketplace is a Git repository that lists activities. Lullshift uses your existing Git
          access to fetch it and never stores passwords or tokens. It only connects when you press a
          button here.
        </p>
        <form
          className="mk-add"
          onSubmit={(e) => {
            e.preventDefault();
            void add();
          }}
        >
          <label className="field-label" htmlFor="mk-url">
            Git address or owner/repo
          </label>
          <div className="mk-add-row">
            <input
              id="mk-url"
              className="field mk-url"
              type="text"
              inputMode="url"
              autoComplete="off"
              spellCheck={false}
              placeholder="owner/repo or https://example.com/owner/repo.git"
              value={input}
              disabled={busy !== null}
              aria-describedby="mk-url-hint"
              aria-invalid={inputError !== null || (input.trim() !== "" && !check.ok)}
              onChange={(e) => {
                setInput(e.target.value);
                setInputError(null);
                setMessage(null);
                // A new address needs its own answer.
                setPrivateOk(false);
                setPrivateAsked(false);
              }}
            />
            <button
              type="submit"
              className="btn btn-primary"
              disabled={
                busy !== null || input.trim() === "" || !check.ok || (showPrivate && !privateOk)
              }
            >
              {busy === "add" ? "Adding…" : "Add marketplace"}
            </button>
          </div>
          <p id="mk-url-hint" className="field-note">
            {inputError ?? hint ?? "Anyone can publish a marketplace; add only ones you trust."}
          </p>
          {showPrivate && (
            <PrivateHostBox
              id="mk-private"
              checked={privateOk}
              disabled={busy !== null}
              onChange={(v) => {
                setPrivateOk(v);
                setInputError(null);
              }}
            />
          )}
        </form>
        {busy === "add" && (
          <p className="mk-progress" aria-hidden="true">
            <span className="mk-spinner" /> Fetching…
          </p>
        )}
      </section>

      {list.status === "loading" && (
        <div className="skeletons" role="status">
          <span className="visually-hidden">Loading marketplaces…</span>
          <div className="skeleton" />
        </div>
      )}
      {list.status === "error" && <Banner tone="danger">{list.text}</Banner>}

      {list.status === "ready" && (
        <ul className="mk-list">
          {list.list.map((m) => (
            <li key={m.id} className="card mk-item">
              <div className="mk-item-head">
                <h2 className="card-title">{truncate(m.id, 60)}</h2>
                {m.builtin && <span className="activity-kind">Built in</span>}
                {m.privateHost && <span className="activity-kind">On your network</span>}
              </div>
              {m.description && <p className="card-text">{truncate(m.description, 200)}</p>}
              <p className="mk-meta mk-wrap">{truncate(m.url, 200)}</p>
              {!m.builtin && m.owner && (
                <p className="mk-meta">Published by {truncate(m.owner, 60)} (not verified)</p>
              )}
              {!m.builtin && (
                <p className="mk-meta">
                  {fetchedLine(m.lastFetchedCommit, m.lastFetchedAt, (iso) =>
                    relativeTime(iso, now),
                  )}
                </p>
              )}
              <div className="activity-actions">
                <button
                  type="button"
                  className="btn btn-secondary"
                  aria-pressed={selected === m.id}
                  onClick={() => setSelected(selected === m.id ? null : m.id)}
                >
                  {selected === m.id ? "Hide activities" : "Browse activities"}
                  <span className="visually-hidden"> in {truncate(m.id, 60)}</span>
                </button>
                {!m.builtin && (
                  <>
                    <button
                      type="button"
                      className="btn btn-quiet"
                      disabled={busy !== null}
                      onClick={() => void refresh(m)}
                    >
                      {busy === m.id ? "Refreshing…" : "Refresh"}
                      <span className="visually-hidden"> {truncate(m.id, 60)}</span>
                    </button>
                    <button
                      type="button"
                      className="btn btn-quiet"
                      disabled={busy !== null}
                      onClick={() => setRemoving(m)}
                    >
                      Remove
                      <span className="visually-hidden"> {truncate(m.id, 60)}</span>
                    </button>
                  </>
                )}
              </div>
              {refreshPrivate?.id === m.id && (
                <div className="mk-private-refresh">
                  <PrivateHostBox
                    id={`mk-private-${m.id}`}
                    checked={refreshPrivate.ok}
                    disabled={busy !== null}
                    onChange={(ok) => setRefreshPrivate({ id: m.id, ok })}
                  />
                  <div className="activity-actions">
                    <button
                      type="button"
                      className="btn btn-quiet"
                      disabled={busy !== null}
                      onClick={() => setRefreshPrivate(null)}
                    >
                      Cancel
                    </button>
                    <button
                      type="button"
                      className="btn btn-primary"
                      disabled={busy !== null || !refreshPrivate.ok}
                      onClick={() => void refresh(m, true)}
                    >
                      Confirm and refresh
                      <span className="visually-hidden"> {truncate(m.id, 60)}</span>
                    </button>
                  </div>
                </div>
              )}
            </li>
          ))}
        </ul>
      )}

      {current && (
        <section className="card" aria-labelledby="mk-entries-title">
          <h2 className="card-title" id="mk-entries-title">
            Activities in {truncate(current.id, 60)}
          </h2>
          {shown === null && <p role="status">Loading…</p>}
          {shown?.status === "error" && <Banner tone="danger">{shown.text}</Banner>}
          {shown?.status === "ready" && shown.entries.length === 0 && (
            <p className="card-text">
              Nothing listed.
              {!current.builtin && !current.lastFetchedAt && " Press Refresh to fetch the list."}
            </p>
          )}
          {shown?.status === "ready" && shown.entries.length > 0 && (
            <ul className="mk-entries">
              {shown.entries.map((e) => {
                const status = entryStatus(e);
                return (
                  <li key={e.activityId} className="mk-entry">
                    <div className="mk-entry-text">
                      <span className="row-label">
                        {entryTitle(e)}
                        {e.featured && <span className="mk-featured"> Featured</span>}
                      </span>
                      {e.name && e.name !== e.activityId && (
                        <span className="mk-meta">{truncate(e.activityId, 80)}</span>
                      )}
                      {e.description && (
                        <span className="row-desc">{truncate(e.description, 200)}</span>
                      )}
                      <span className="mk-meta">
                        {kindLabel(e.kind)}
                        {e.external && " · from another repository"}
                        {status.installed && ` · ${status.installed}`}
                      </span>
                      {status.blocked && (
                        <span className="mk-blocked">
                          <Icon name="info" size={16} /> {status.blocked}
                        </span>
                      )}
                    </div>
                    {canInstall(e) && (
                      <button
                        type="button"
                        className="btn btn-primary"
                        onClick={() =>
                          setReview({
                            mode: "install",
                            marketplaceId: current.id,
                            activityId: e.activityId,
                            name: entryTitle(e),
                          })
                        }
                      >
                        Install<span className="visually-hidden"> {entryTitle(e)}</span>
                      </button>
                    )}
                  </li>
                );
              })}
            </ul>
          )}
        </section>
      )}

      <Dialog
        open={removing !== null}
        title="Remove this marketplace?"
        busy={busy !== null}
        onClose={() => setRemoving(null)}
        actions={
          <>
            <button
              type="button"
              className="btn btn-quiet"
              autoFocus
              disabled={busy !== null}
              onClick={() => setRemoving(null)}
            >
              Cancel
            </button>
            <button
              type="button"
              className="btn btn-danger"
              disabled={busy !== null}
              onClick={() => void remove()}
            >
              {busy !== null ? "Removing…" : "Remove marketplace"}
            </button>
          </>
        }
        description={
          <p>
            Lullshift forgets {removing ? truncate(removing.id, 60) : "this marketplace"} and
            deletes its downloaded list. Activities you already installed from it stay, and so does
            your saved progress.
          </p>
        }
      />

      <ReviewDialog
        request={review}
        onClose={() => setReview(null)}
        onDone={(text) => {
          const id = review?.marketplaceId;
          setReview(null);
          say("success", text);
          if (id) void loadEntries(id);
          onChanged();
        }}
      />
    </div>
  );
}
