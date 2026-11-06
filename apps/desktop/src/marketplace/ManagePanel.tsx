import { useCallback, useEffect, useState } from "react";
import { Banner } from "../ui/Banner";
import { Dialog } from "../ui/Dialog";
import { Switch } from "../ui/Switch";
import {
  activitiesManageList,
  activitySetEnabled,
  activityUninstall,
  toCommandError,
  type ManageItem,
} from "./api";
import {
  canCheckUpdate,
  errorText,
  manageSubtitle,
  NOT_FOUND_AT_NEW_ADDRESS_TEXT,
  problemText,
  truncate,
  UNAVAILABLE_TEXT,
} from "./logic";
import { ReviewDialog, type ReviewRequest } from "./ReviewDialog";

type State =
  | { status: "loading" }
  | { status: "ready"; items: ManageItem[] }
  | { status: "unavailable" }
  | { status: "error"; text: string };

interface Props {
  /** The set of enabled or installed activities changed: the library should re-read. */
  onChanged: () => void;
}

/** Turn any activity on or off, uninstall installed ones, and check them for updates. */
export function ManagePanel({ onChanged }: Props) {
  const [state, setState] = useState<State>({ status: "loading" });
  const [message, setMessage] = useState<{ tone: "success" | "danger"; text: string } | null>(null);
  const [live, setLive] = useState("");
  const [pending, setPending] = useState<string | null>(null);
  const [uninstalling, setUninstalling] = useState<ManageItem | null>(null);
  const [deleteProgress, setDeleteProgress] = useState(false);
  const [review, setReview] = useState<ReviewRequest | null>(null);

  const say = useCallback((tone: "success" | "danger", text: string) => {
    setMessage({ tone, text });
    setLive(text);
  }, []);

  const load = useCallback(() => {
    return activitiesManageList()
      .then((items) => setState({ status: "ready", items }))
      .catch((err) => {
        const e = toCommandError(err);
        setState(
          e.kind === "unavailable"
            ? { status: "unavailable" }
            : { status: "error", text: errorText(e) },
        );
      });
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const toggle = async (item: ManageItem, enabled: boolean) => {
    setPending(item.activityId);
    setMessage(null);
    try {
      await activitySetEnabled(item.activityId, enabled);
      say("success", `${truncate(item.name, 60)} is ${enabled ? "on" : "off"}.`);
      onChanged();
    } catch (err) {
      say("danger", errorText(toCommandError(err)));
    } finally {
      await load();
      setPending(null);
    }
  };

  const uninstall = async () => {
    const item = uninstalling;
    if (!item) return;
    setPending(item.activityId);
    try {
      await activityUninstall(item.activityId, deleteProgress);
      setUninstalling(null);
      say(
        "success",
        `${truncate(item.name, 60)} was uninstalled${
          deleteProgress ? " and its saved progress deleted" : ". Its saved progress was kept"
        }.`,
      );
      onChanged();
    } catch (err) {
      setUninstalling(null);
      say("danger", errorText(toCommandError(err)));
    } finally {
      await load();
      setPending(null);
    }
  };

  if (state.status === "unavailable") return <Banner tone="info">{UNAVAILABLE_TEXT}</Banner>;

  return (
    <div className="mk">
      <p className="visually-hidden" role="status" aria-live="polite">
        {live}
      </p>
      {message && <Banner tone={message.tone}>{message.text}</Banner>}
      {state.status === "loading" && (
        <div className="skeletons" role="status">
          <span className="visually-hidden">Loading activities…</span>
          <div className="skeleton" />
        </div>
      )}
      {state.status === "error" && <Banner tone="danger">{state.text}</Banner>}
      {state.status === "ready" && (
        <>
          <p className="view-lede">
            Turned-off activities are hidden from your library. Nothing is deleted.
          </p>
          <ul className="mk-list">
            {state.items.map((item) => (
              <li key={item.activityId} className="card mk-item">
                <Switch
                  checked={item.enabled}
                  disabled={pending !== null || item.problem === "tampered"}
                  onChange={(on) => void toggle(item, on)}
                  label={truncate(item.name, 80)}
                  description={manageSubtitle(item)}
                />
                {item.problem && (
                  <Banner tone="attention">
                    <strong>
                      {item.problem === "tampered" ? "Damaged, reinstall it. " : "Engine missing. "}
                    </strong>
                    {problemText(item.problem)}
                  </Banner>
                )}
                {item.sourceNote === "notFoundAtNewAddress" && (
                  <Banner tone="info">
                    <strong>Not found at the new address. </strong>
                    {NOT_FOUND_AT_NEW_ADDRESS_TEXT}
                  </Banner>
                )}
                {!item.builtin && (
                  <div className="activity-actions">
                    {canCheckUpdate(item) && (
                      <button
                        type="button"
                        className="btn btn-secondary"
                        disabled={pending !== null}
                        onClick={() =>
                          setReview({
                            mode: "update",
                            marketplaceId: item.marketplaceId,
                            activityId: item.activityId,
                            name: item.name,
                          })
                        }
                      >
                        Check for update
                        <span className="visually-hidden"> {truncate(item.name, 60)}</span>
                      </button>
                    )}
                    <button
                      type="button"
                      className="btn btn-quiet"
                      disabled={pending !== null}
                      onClick={() => {
                        setDeleteProgress(false);
                        setUninstalling(item);
                      }}
                    >
                      Uninstall
                      <span className="visually-hidden"> {truncate(item.name, 60)}</span>
                    </button>
                  </div>
                )}
              </li>
            ))}
          </ul>
        </>
      )}

      <Dialog
        open={uninstalling !== null}
        title="Uninstall this activity?"
        busy={pending !== null}
        onClose={() => setUninstalling(null)}
        actions={
          <>
            <button
              type="button"
              className="btn btn-quiet"
              autoFocus
              disabled={pending !== null}
              onClick={() => setUninstalling(null)}
            >
              Cancel
            </button>
            <button
              type="button"
              className="btn btn-danger"
              disabled={pending !== null}
              onClick={() => void uninstall()}
            >
              {pending !== null ? "Uninstalling…" : "Uninstall"}
            </button>
          </>
        }
        description={
          <p>
            {uninstalling ? truncate(uninstalling.name, 60) : "This activity"} will be removed from
            this device. You can install it again from its marketplace.
          </p>
        }
      >
        <label className="mk-check">
          <input
            type="checkbox"
            checked={deleteProgress}
            onChange={(e) => setDeleteProgress(e.target.checked)}
          />
          <span>Also delete saved progress</span>
        </label>
      </Dialog>

      <ReviewDialog
        request={review}
        onClose={() => setReview(null)}
        onDone={(text) => {
          setReview(null);
          say("success", text);
          onChanged();
          void load();
        }}
      />
    </div>
  );
}
