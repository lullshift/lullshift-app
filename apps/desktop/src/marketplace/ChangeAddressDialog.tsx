import { useState } from "react";
import { Banner } from "../ui/Banner";
import { Dialog } from "../ui/Dialog";
import { marketplaceChangeAddress, toCommandError, type AddressChanged } from "./api";
import { changeAddressCheck, errorText, looksPrivate, PRIVATE_HOST_TEXT, truncate } from "./logic";
import { PrivateHostBox } from "./PrivateHostBox";

export interface ChangeAddressRequest {
  id: string;
  /** The current address, display form. */
  current: string;
  /** A proposed new address (a same-host redirect the server reported), or "". */
  proposed: string;
}

interface Props {
  request: ChangeAddressRequest | null;
  onClose: () => void;
  onDone: (result: AddressChanged) => void;
}

/**
 * "Change address" (decision 18.4). Shows the old and new addresses side by side; nothing is
 * fetched or changed until the user presses Change address. Cancel has the initial focus.
 */
export function ChangeAddressDialog({ request, onClose, onDone }: Props) {
  // Keyed by the request so a new one starts clean.
  const [state, setState] = useState<{
    for: ChangeAddressRequest | null;
    input: string;
    privateOk: boolean;
    privateAsked: boolean;
    error: string | null;
    busy: boolean;
  }>({ for: null, input: "", privateOk: false, privateAsked: false, error: null, busy: false });
  const s =
    state.for === request
      ? state
      : {
          for: request,
          input: request?.proposed ?? "",
          privateOk: false,
          privateAsked: false,
          error: null,
          busy: false,
        };
  const update = (patch: Partial<typeof s>) => setState({ ...s, ...patch });

  const check = request ? changeAddressCheck(request.current, s.input) : null;
  const newAddress = check?.ok ? check.value : null;
  const showPrivate = newAddress !== null && (s.privateAsked || looksPrivate(newAddress));
  const canSubmit = newAddress !== null && !s.busy && (!showPrivate || s.privateOk);

  const submit = async () => {
    if (!request || !newAddress) return;
    if (showPrivate && !s.privateOk) {
      update({ error: PRIVATE_HOST_TEXT });
      return;
    }
    update({ busy: true, error: null });
    try {
      const result = await marketplaceChangeAddress(request.id, newAddress, showPrivate);
      setState({ ...s, busy: false });
      onDone(result);
    } catch (err) {
      const e = toCommandError(err);
      setState({
        ...s,
        busy: false,
        privateAsked: s.privateAsked || e.kind === "privateHost",
        error: errorText(e),
      });
    }
  };

  return (
    <Dialog
      open={request !== null}
      title="Change this marketplace's address?"
      busy={s.busy}
      onClose={onClose}
      actions={
        <>
          <button
            type="button"
            className="btn btn-quiet"
            autoFocus
            disabled={s.busy}
            onClick={onClose}
          >
            Cancel
          </button>
          <button
            type="button"
            className="btn btn-primary"
            disabled={!canSubmit}
            onClick={() => void submit()}
          >
            {s.busy ? "Changing…" : "Change address"}
          </button>
        </>
      }
      description={
        <p>
          Use this when a marketplace moved. Lullshift fetches the new address when you choose
          Change address and checks that it uses the same marketplace name. Installed activities
          keep their versions and saved progress. If you later install something you removed, you
          are asked about any progress it left behind.
        </p>
      }
    >
      {request && (
        <div className="review">
          <dl className="review-facts">
            <dt>Now</dt>
            <dd className="review-wrap">
              <code>{truncate(request.current, 300)}</code>
            </dd>
            <dt>New</dt>
            <dd className="review-wrap">
              {newAddress ? <code>{truncate(newAddress, 300)}</code> : "Not entered yet"}
            </dd>
          </dl>
          <label className="field-label" htmlFor="mk-new-url">
            New Git address
          </label>
          <input
            id="mk-new-url"
            className="field mk-url"
            type="text"
            inputMode="url"
            autoComplete="off"
            spellCheck={false}
            value={s.input}
            disabled={s.busy}
            aria-describedby="mk-new-url-hint"
            aria-invalid={s.input.trim() !== "" && !check?.ok}
            onChange={(e) =>
              update({ input: e.target.value, privateOk: false, privateAsked: false, error: null })
            }
          />
          <p id="mk-new-url-hint" className="field-note">
            {s.input.trim() !== "" && check && !check.ok
              ? check.reason
              : "If a pinned version isn't at the new address, that activity stays exactly as installed."}
          </p>
          {showPrivate && (
            <PrivateHostBox
              id="mk-new-private"
              checked={s.privateOk}
              disabled={s.busy}
              onChange={(v) => update({ privateOk: v, error: null })}
            />
          )}
          {s.busy && <p role="status">Fetching the new address and checking pinned versions…</p>}
          {s.error && <Banner tone="danger">{s.error}</Banner>}
        </div>
      )}
    </Dialog>
  );
}
