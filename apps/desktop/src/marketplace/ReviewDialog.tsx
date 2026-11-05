import { useEffect, useState } from "react";
import { Banner } from "../ui/Banner";
import { Dialog } from "../ui/Dialog";
import { Icon } from "../ui/Icon";
import {
  marketplaceInstall,
  marketplacePreviewInstall,
  marketplacePreviewUpdate,
  marketplaceRefresh,
  marketplaceUpdate,
  toCommandError,
  type InstallPreview,
  type UpdatePreview,
} from "./api";
import {
  errorText,
  installSummary,
  kindLabel,
  permissionExplanation,
  permissionLabel,
  savedStateLine,
  truncate,
  UNVERIFIED_TEXT,
  updateView,
} from "./logic";

/** What the user asked to review. `marketplaceId` is the source marketplace. */
export interface ReviewRequest {
  mode: "install" | "update";
  marketplaceId: string;
  activityId: string;
  /** Untrusted display name, only used for the loading text. */
  name: string;
}

type State =
  | { phase: "loading" }
  | { phase: "error"; text: string }
  | { phase: "ready"; preview: InstallPreview | UpdatePreview }
  | { phase: "working"; preview: InstallPreview | UpdatePreview };

interface Props {
  request: ReviewRequest | null;
  onClose: () => void;
  /** Called after a successful install or update, with a message for the live region. */
  onDone: (message: string) => void;
}

/**
 * The install and update review. Nothing is written until the user presses Install or Update;
 * Cancel has the initial focus.
 */
export function ReviewDialog({ request, onClose, onDone }: Props) {
  // Tagged with the request it belongs to, so a new request starts at "loading" with no reset.
  const [result, setResult] = useState<{ for: ReviewRequest; state: State } | null>(null);
  const state: State = result && result.for === request ? result.state : { phase: "loading" };
  const setState = (s: State) => {
    if (request) setResult({ for: request, state: s });
  };

  useEffect(() => {
    if (!request) return;
    let cancelled = false;
    const run = async () => {
      if (request.mode === "install") {
        return await marketplacePreviewInstall(request.marketplaceId, request.activityId);
      }
      // Updates are only ever checked when the user asks: refresh first, then compare.
      await marketplaceRefresh(request.marketplaceId);
      return await marketplacePreviewUpdate(request.activityId);
    };
    run()
      .then((preview) => {
        if (!cancelled) setResult({ for: request, state: { phase: "ready", preview } });
      })
      .catch((err) => {
        if (!cancelled)
          setResult({
            for: request,
            state: { phase: "error", text: errorText(toCommandError(err)) },
          });
      });
    return () => {
      cancelled = true;
    };
  }, [request]);

  const preview = state.phase === "ready" || state.phase === "working" ? state.preview : null;
  const isUpdate = request?.mode === "update";
  const unchanged = isUpdate && preview !== null && (preview as UpdatePreview).unchanged;
  const working = state.phase === "working";

  const confirm = async () => {
    if (state.phase !== "ready" || !request) return;
    const p = state.preview;
    setState({ phase: "working", preview: p });
    try {
      if (request.mode === "install") await marketplaceInstall(p.token);
      else await marketplaceUpdate(p.token);
      onDone(
        request.mode === "install"
          ? `${truncate(p.name, 60)} was installed and turned on.`
          : `${truncate(p.name, 60)} was updated to version ${truncate(p.version, 20)}.`,
      );
    } catch (err) {
      setState({ phase: "error", text: errorText(toCommandError(err)) });
    }
  };

  const title = !request
    ? ""
    : request.mode === "install"
      ? "Review before installing"
      : unchanged
        ? "No update needed"
        : "Review the update";

  return (
    <Dialog
      open={request !== null}
      title={title}
      busy={working}
      onClose={onClose}
      actions={
        <>
          <button
            type="button"
            className="btn btn-quiet"
            autoFocus
            disabled={working}
            onClick={onClose}
          >
            {unchanged ? "Close" : "Cancel"}
          </button>
          {state.phase === "ready" && !unchanged && (
            <button type="button" className="btn btn-primary" onClick={() => void confirm()}>
              {isUpdate ? "Update" : "Install"}
            </button>
          )}
          {working && (
            <button type="button" className="btn btn-primary" disabled>
              {isUpdate ? "Updating…" : "Installing…"}
            </button>
          )}
        </>
      }
    >
      {request && (
        <div className="review" aria-live="polite">
          {state.phase === "loading" && (
            <p role="status">
              {request.mode === "install"
                ? `Fetching and checking ${truncate(request.name, 60)}…`
                : `Looking for an update to ${truncate(request.name, 60)}…`}
            </p>
          )}
          {state.phase === "error" && <Banner tone="danger">{state.text}</Banner>}
          {preview && (
            <ReviewBody preview={preview} update={isUpdate ? (preview as UpdatePreview) : null} />
          )}
        </div>
      )}
    </Dialog>
  );
}

function ReviewBody({
  preview: p,
  update,
}: {
  preview: InstallPreview;
  update: UpdatePreview | null;
}) {
  const view = update ? updateView(update) : null;
  if (view?.unchanged) {
    return (
      <Banner tone="success">
        {view.headline}. {truncate(p.name, 60)} is at version {truncate(p.version, 20)}.
      </Banner>
    );
  }
  const added = new Set(update?.addedPermissions ?? []);
  return (
    <>
      <p className="review-name">
        <strong>{truncate(p.name, 80)}</strong>{" "}
        <span className="review-version">{truncate(p.version, 20)}</span>
      </p>
      {p.description && <p>{truncate(p.description, 400)}</p>}

      {p.thirdParty && <Banner tone="attention">{UNVERIFIED_TEXT}</Banner>}
      {savedStateLine(p) && <Banner tone="attention">{savedStateLine(p)}</Banner>}

      {view && (
        <div className="review-diff">
          <p>
            <strong>Version:</strong> {view.versionLine}
          </p>
          <p>
            <strong>Commit:</strong> <code>{view.commitLine}</code>
          </p>
          {view.sourceChange && (
            <div className="review-source-changed" role="group" aria-label="Source changed">
              <Banner tone="attention">
                <p>
                  <strong>Source changed.</strong> This update comes from a different repository
                  than the one you installed from.
                </p>
                <dl className="review-facts">
                  <dt>Was</dt>
                  <dd className="review-wrap">{view.sourceChange.old}</dd>
                  <dt>Now</dt>
                  <dd className="review-wrap">{view.sourceChange.new}</dd>
                </dl>
              </Banner>
            </div>
          )}
          {view.warnings.length > 0 && (
            <Banner tone="attention">
              <ul className="plain-list">
                {view.warnings.map((w) => (
                  <li key={w}>{w}</li>
                ))}
              </ul>
              <p>Needs your approval before updating.</p>
            </Banner>
          )}
        </div>
      )}

      <dl className="review-facts">
        <dt>Marketplace</dt>
        <dd>{truncate(p.marketplaceId, 60)}</dd>
        <dt>Source</dt>
        <dd className="review-wrap">{truncate(p.sourceUrl, 300)}</dd>
        {p.sourcePath && (
          <>
            <dt>Path</dt>
            <dd className="review-wrap">{truncate(p.sourcePath, 200)}</dd>
          </>
        )}
        <dt>Commit</dt>
        <dd>
          <code className="review-commit">{p.commit}</code>
        </dd>
        <dt>Type</dt>
        <dd>{kindLabel(p.kind)}</dd>
        {p.engine && (
          <>
            <dt>Engine</dt>
            <dd>{truncate(p.engine, 60)}</dd>
          </>
        )}
        <dt>Size</dt>
        <dd>{installSummary(p)}</dd>
      </dl>

      <h3 className="review-subtitle">Permissions</h3>
      {p.permissions.length === 0 ? (
        <p>None. It can't save anything on this device.</p>
      ) : (
        <ul className="checklist">
          {p.permissions.map((perm) => (
            <li key={perm}>
              <Icon name={added.has(perm) ? "alert" : "check"} size={18} />
              <span>
                <code>{permissionLabel(perm)}</code>
                {added.has(perm) && <strong> New, needs approval.</strong>}{" "}
                {permissionExplanation(perm)}
              </span>
            </li>
          ))}
        </ul>
      )}
      {update && update.removedPermissions.length > 0 && (
        <p className="review-removed">
          No longer asks for:{" "}
          {update.removedPermissions.map((perm) => permissionLabel(perm)).join(", ")}.
        </p>
      )}
    </>
  );
}
