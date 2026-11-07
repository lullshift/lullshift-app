// What the provider itself said about a refused request, and the note for a reply that was not
// streamed. Both are plain React text in their own elements: the provider's words never go
// through ReplyText, link detection or any markup, so a message such as "click here:
// https://evil.example" is just text (no link, no button).

import type { ProviderReason } from "./api";

/** The provider's words, labelled as theirs and set apart from Lullshift's own sentence. */
export function ProviderSaid({ reason }: { reason: ProviderReason | null | undefined }) {
  if (!reason || !reason.text.trim()) return null;
  return (
    <span className="provider-said" data-testid="provider-said">
      <span className="provider-said-by">{reason.speaker} said:</span>{" "}
      <q className="provider-said-text">{reason.text}</q>
    </span>
  );
}

/** A delivery note beside a reply that arrived all at once. It is not part of the reply. */
export function NotStreamedNote({ show }: { show: boolean | undefined }) {
  if (!show) return null;
  return (
    <p className="msg-note" data-testid="not-streamed-note">
      This reply arrived all at once: your provider didn&apos;t stream it, so Lullshift asked for it
      in one piece.
    </p>
  );
}
