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
} from "./api";
import {
  canInstall,
  checkGitInput,
  entryStatus,
  entryTitle,
  errorText,
  fetchedLine,
  kindLabel,
  truncate,
  UNAVAILABLE_TEXT,
} from "./logic";
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

  const add = async () => {
    const c = checkGitInput(input);
    if (!c.ok) {
      setInputError(c.reason);
      return;
    }
    setInputError(null);
    setMessage(null);
    setBusy("add");
    setLive("Adding marketplace. Fetching its list with your Git access…");
    try {
      const m = await marketplaceAdd(c.value);
      setInput("");
      say("success", `${truncate(m.id, 60)} was added.`);
      await loadList();
      setSelected(m.id);
    } catch (err) {
      say("danger", errorText(toCommandError(err)));
    } finally {
      setBusy(null);
    }
  };

  const refresh = async (m: Marketplace) => {
    setMessage(null);
    setBusy(m.id);
    setLive(`Refreshing ${truncate(m.id, 60)}…`);
    try {
      await marketplaceRefresh(m.id);
      say("success", `${truncate(m.id, 60)} is up to date.`);
      await loadList();
      if (selected === m.id) await loadEntries(m.id);
    } catch (err) {
      say("danger", errorText(toCommandError(err)));
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
              }}
            />
            <button
              type="submit"
              className="btn btn-primary"
              disabled={busy !== null || input.trim() === "" || !check.ok}
            >
              {busy === "add" ? "Adding…" : "Add marketplace"}
            </button>
          </div>
          <p id="mk-url-hint" className="field-note">
            {inputError ?? hint ?? "Anyone can publish a marketplace; add only ones you trust."}
          </p>
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
      >
        <p>
          Lullshift forgets {removing ? truncate(removing.id, 60) : "this marketplace"} and deletes
          its downloaded list. Activities you already installed from it stay, and so does your saved
          progress.
        </p>
      </Dialog>

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
