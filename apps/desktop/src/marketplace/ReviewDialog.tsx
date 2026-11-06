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
  asksAboutSavedState,
  canConfirmReview,
  CODE_RISK_TICK_HINT,
  codeRiskView,
  errorText,
  FOREIGN_STATE_DELETE,
  FOREIGN_STATE_KEEP,
  FOREIGN_STATE_KEEP_WARNING,
  FOREIGN_STATE_TEXT,
  installSummary,
  kindLabel,
  needsCodeConsent,
  permissionExplanation,
  PUBLISHER_TEXT_LABEL,
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
  // The answer about saved progress from another source, tagged like `result`, so every new
  // review starts at "delete".
  const [keepFor, setKeepFor] = useState<ReviewRequest | null>(null);
  const keepSaved = keepFor !== null && keepFor === request;
  // The consent box under a third-party code warning, tagged the same way, so every new review
  // (install or update) starts unticked (owner decision 27.5).
  const [consentFor, setConsentFor] = useState<ReviewRequest | null>(null);
  const consented = consentFor !== null && consentFor === request;

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
    if (!canConfirmReview(p, consented)) return;
    const acceptCodeRisk = needsCodeConsent(p) && consented;
    setState({ phase: "working", preview: p });
    try {
      if (request.mode === "install")
        await marketplaceInstall(p.token, asksAboutSavedState(p) && keepSaved, acceptCodeRisk);
      else await marketplaceUpdate(p.token, acceptCodeRisk);
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
            <ConfirmButton
              label={isUpdate ? "Update" : "Install"}
              blocked={!canConfirmReview(state.preview, consented)}
              onConfirm={() => void confirm()}
            />
          )}
          {working && (
            <button type="button" className="btn btn-primary" disabled>
              {isUpdate ? "Updating…" : "Installing…"}
            </button>
          )}
        </>
      }
      description={
        state.phase === "ready" && !unchanged ? (
          <p>
            Check what this activity asks for before you continue. Nothing is{" "}
            {isUpdate ? "updated" : "installed"} until you choose {isUpdate ? "Update" : "Install"}.
          </p>
        ) : undefined
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
            <ReviewBody
              preview={preview}
              update={isUpdate ? (preview as UpdatePreview) : null}
              keepSaved={keepSaved}
              consented={consented}
              disabled={working}
              onKeepSaved={(keep) => setKeepFor(keep ? request : null)}
              onConsent={(yes) => setConsentFor(yes ? request : null)}
            />
          )}
        </div>
      )}
    </Dialog>
  );
}

/**
 * Install or Update. While the code-risk box is unticked it is disabled and described by the
 * hint that says why, so a screen reader hears the reason on the button.
 */
export function ConfirmButton({
  label,
  blocked,
  onConfirm,
  onBlockedClick = focusConsentBox,
}: {
  label: string;
  blocked: boolean;
  onConfirm: () => void;
  /** What pressing the button does while it is blocked. Defaults to focusing the consent box. */
  onBlockedClick?: () => void;
}) {
  // aria-disabled, not disabled: the button stays focusable, so a keyboard or screen-reader
  // user lands on it, hears why it can't be used, and pressing it takes them to the box.
  return (
    <button
      type="button"
      className="btn btn-primary"
      aria-disabled={blocked ? "true" : undefined}
      aria-describedby={blocked ? "review-code-risk-hint" : undefined}
      onClick={blocked ? onBlockedClick : onConfirm}
    >
      {label}
    </button>
  );
}

export const CONSENT_BOX_ID = "review-code-risk-consent";

function focusConsentBox() {
  document.getElementById(CONSENT_BOX_ID)?.focus();
}

/**
 * The warning for code from a third-party marketplace and the consent box (owner decision
 * 27.5). The box is never ticked in advance; the warning text describes it.
 */
export function CodeRiskSection({
  preview,
  consented,
  disabled,
  onChange,
}: {
  preview: InstallPreview;
  consented: boolean;
  disabled: boolean;
  onChange: (consented: boolean) => void;
}) {
  if (!codeRiskView(preview)) return null;
  return (
    <>
      <CodeRiskWarning preview={preview} />
      <CodeRiskConsent
        preview={preview}
        consented={consented}
        disabled={disabled}
        onChange={onChange}
      />
    </>
  );
}

/** The warning text. It comes before anything the publisher wrote. */
export function CodeRiskWarning({ preview }: { preview: InstallPreview }) {
  const view = codeRiskView(preview);
  if (!view) return null;
  return (
    <section className="review-code-risk" aria-labelledby="review-code-risk-title">
      <Banner tone={view.failed ? "danger" : "attention"}>
        <h3 id="review-code-risk-title" className="review-subtitle">
          {view.heading}
        </h3>
        <div id="review-code-risk-text">
          <ul className="plain-list review-code-risk-points">
            {view.points.map((point) => (
              <li key={point}>{point}</li>
            ))}
          </ul>
          <p>{view.verification}</p>
          <p>
            <strong>{view.worstCase}</strong>
          </p>
          <p>{view.noGuarantee}</p>
        </div>
      </Banner>
    </section>
  );
}

/** The consent box, the last thing in the review before the buttons. Never ticked in advance. */
export function CodeRiskConsent({
  preview,
  consented,
  disabled,
  onChange,
}: {
  preview: InstallPreview;
  consented: boolean;
  disabled: boolean;
  onChange: (consented: boolean) => void;
}) {
  const view = codeRiskView(preview);
  if (!view) return null;
  return (
    <div className="review-code-risk">
      <label className="mk-check review-code-risk-consent">
        <input
          type="checkbox"
          id={CONSENT_BOX_ID}
          checked={consented}
          disabled={disabled}
          aria-describedby="review-code-risk-text"
          onChange={(e) => onChange(e.currentTarget.checked)}
        />
        <span>{view.consentLabel}</span>
      </label>
      {!consented && (
        <p id="review-code-risk-hint" className="review-code-risk-hint">
          {CODE_RISK_TICK_HINT}
        </p>
      )}
    </div>
  );
}

/**
 * Asks what to do with saved progress recorded as another marketplace's or repository's
 * (decision 18.6). Delete is selected and takes the focus when the question appears; keeping it
 * shows a warning.
 */
export function ForeignStateChoice({
  keep,
  disabled,
  onChange,
}: {
  keep: boolean;
  disabled: boolean;
  onChange: (keep: boolean) => void;
}) {
  return (
    <fieldset className="review-saved-choice" aria-describedby="review-saved-text">
      <legend className="review-subtitle">Saved progress</legend>
      <p id="review-saved-text">{FOREIGN_STATE_TEXT}</p>
      <label className="mk-check">
        <input
          type="radio"
          name="review-saved"
          autoFocus
          checked={!keep}
          disabled={disabled}
          onChange={() => onChange(false)}
        />
        <span>{FOREIGN_STATE_DELETE}</span>
      </label>
      <label className="mk-check">
        <input
          type="radio"
          name="review-saved"
          checked={keep}
          disabled={disabled}
          aria-describedby={keep ? "review-saved-warning" : undefined}
          onChange={() => onChange(true)}
        />
        <span>{FOREIGN_STATE_KEEP}</span>
      </label>
      {keep && (
        <div id="review-saved-warning">
          <Banner tone="attention">{FOREIGN_STATE_KEEP_WARNING}</Banner>
        </div>
      )}
    </fieldset>
  );
}

export function ReviewBody({
  preview: p,
  update,
  keepSaved,
  consented,
  disabled,
  onKeepSaved,
  onConsent,
}: {
  preview: InstallPreview;
  update: UpdatePreview | null;
  keepSaved: boolean;
  consented: boolean;
  disabled: boolean;
  onKeepSaved: (keep: boolean) => void;
  onConsent: (consented: boolean) => void;
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
      {/* The warning comes first: everything the publisher wrote (name, description) is below it. */}
      <CodeRiskWarning preview={p} />
      {p.thirdParty && <Banner tone="attention">{UNVERIFIED_TEXT}</Banner>}
      <p className="review-name">
        <strong>{truncate(p.name, 80)}</strong>{" "}
        <span className="review-version">{truncate(p.version, 20)}</span>
      </p>
      {p.description && (
        <div className="review-publisher">
          <p className="review-publisher-label">{PUBLISHER_TEXT_LABEL}</p>
          <p>{truncate(p.description, 400)}</p>
        </div>
      )}
      {savedStateLine(p) && <Banner tone="attention">{savedStateLine(p)}</Banner>}
      {!update && asksAboutSavedState(p) && (
        <ForeignStateChoice keep={keepSaved} disabled={disabled} onChange={onKeepSaved} />
      )}

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
      {/* Last, just before the buttons. */}
      <CodeRiskConsent preview={p} consented={consented} disabled={disabled} onChange={onConsent} />
    </>
  );
}
