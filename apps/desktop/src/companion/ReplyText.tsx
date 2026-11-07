import { memo, useEffect, useMemo, useRef, useState } from "react";
import { Banner } from "../ui/Banner";
import { Dialog } from "../ui/Dialog";
import { companionOpenLink, toCommandError } from "./api";
import { highlightToReact, languageLabel } from "./highlight";
import { errorText, plainLines } from "./logic";
import { displayUrl, findLinks, hostOf, parseReply } from "./replyFormat";
import "./reply.css";

type CopyState = "idle" | "copied" | "failed";

/** Copies with the async clipboard, or a hidden textarea when that is missing or refused. */
async function copyText(text: string): Promise<boolean> {
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
      return true;
    }
  } catch {
    // fall through to the textarea fallback
  }
  const area = document.createElement("textarea");
  area.value = text;
  area.setAttribute("readonly", "");
  area.setAttribute("aria-hidden", "true");
  area.style.position = "fixed";
  area.style.top = "0";
  area.style.left = "-9999px";
  document.body.append(area);
  try {
    area.select();
    return document.execCommand("copy");
  } catch {
    return false;
  } finally {
    area.remove();
  }
}

/** A Copy button that announces the result in a polite live region. */
export function CopyButton({ text, label }: { text: string; label: string }) {
  const [state, setState] = useState<CopyState>("idle");
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current);
    },
    [],
  );

  const onClick = async () => {
    const ok = await copyText(text);
    setState(ok ? "copied" : "failed");
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(() => setState("idle"), 2000);
  };

  return (
    <>
      <button
        type="button"
        className="btn btn-secondary reply-copy"
        aria-label={label}
        onClick={() => void onClick()}
      >
        {state === "copied" ? "Copied" : state === "failed" ? "Copy failed" : "Copy"}
      </button>
      <span className="visually-hidden" role="status" aria-live="polite">
        {state === "copied" ? "Copied" : state === "failed" ? "Couldn't copy" : ""}
      </span>
    </>
  );
}

/**
 * A fenced code block. Memoised (security review F2): while a reply streams, a block that is
 * already complete is not rendered or highlighted again, and a block that is still being written
 * is shown plain and highlighted once, when its fence closes or the reply ends.
 */
const CodeBlock = memo(function CodeBlock({
  lang,
  code,
  closed,
  streaming,
}: {
  lang: string | null;
  code: string;
  closed: boolean;
  streaming: boolean;
}) {
  const label = languageLabel(lang);
  const writing = !closed && streaming;
  const body = useMemo(
    () => (writing ? code : highlightToReact(lang, code)),
    [writing, lang, code],
  );
  return (
    <div className="reply-code" role="group" aria-label={`${label} code`}>
      <div className="reply-code-head">
        <span className="reply-code-lang">{label}</span>
        {writing && <span className="reply-code-note">Still writing…</span>}
        <CopyButton text={code} label={`Copy ${label} code`} />
      </div>
      <pre tabIndex={0}>
        <code className={`hljs${lang ? ` language-${lang}` : ""}`}>{body}</code>
      </pre>
    </div>
  );
});

/** One line of plain text. Memoised: links are found again only when the line itself changed. */
const Line = memo(function Line({ line, onLink }: { line: string; onLink: (url: string) => void }) {
  if (line === "") return <span className="msg-line"> </span>;
  return (
    <span className="msg-line">
      {findLinks(line).map((part, i) =>
        part.kind === "text" ? (
          part.text
        ) : (
          <button key={i} type="button" className="reply-link" onClick={() => onLink(part.url)}>
            {part.text}
          </button>
        ),
      )}
    </span>
  );
});

/** A plain-text segment. Memoised so earlier segments of a streaming reply aren't touched. */
const TextBlock = memo(function TextBlock({
  text,
  onLink,
}: {
  text: string;
  onLink: (url: string) => void;
}) {
  const lines = useMemo(() => plainLines(text), [text]);
  return (
    <div className="msg-text">
      {lines.map((line, j) => (
        <Line key={j} line={line} onLink={onLink} />
      ))}
    </div>
  );
});

function LinkDialog({ url, onClose }: { url: string | null; onClose: () => void }) {
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const shown = url ? displayUrl(url) : "";

  const close = () => {
    setError(null);
    onClose();
  };

  const open = async () => {
    setBusy(true);
    setError(null);
    try {
      await companionOpenLink(shown);
      close();
    } catch (err) {
      setError(errorText(toCommandError(err)));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog
      open={url !== null}
      title="Open this link?"
      onClose={close}
      busy={busy}
      description={
        <>
          <code className="reply-link-url">{shown}</code>
          <p>
            This opens <strong>{hostOf(shown)}</strong> in your browser. Lullshift doesn't check
            where links lead.
          </p>
        </>
      }
      actions={
        <>
          <button type="button" className="btn btn-secondary" autoFocus onClick={close}>
            Cancel
          </button>
          <button
            type="button"
            className="btn btn-primary"
            disabled={busy}
            onClick={() => void open()}
          >
            Open in browser
          </button>
        </>
      }
    >
      {error && <Banner tone="danger">{error}</Banner>}
    </Dialog>
  );
}

/** An assistant reply: plain text, fenced code blocks and http(s) links. Nothing else. */
export function ReplyText({ text, streaming }: { text: string; streaming?: boolean }) {
  const [link, setLink] = useState<string | null>(null);
  // Parsed once per text: a re-render for another reason (the dialog, a parent) reuses it.
  const segments = useMemo(() => parseReply(text), [text]);
  return (
    <div className="reply-text">
      {segments.map((seg, i) =>
        seg.kind === "text" ? (
          <TextBlock key={i} text={seg.text} onLink={setLink} />
        ) : (
          <CodeBlock
            key={i}
            lang={seg.lang}
            code={seg.code}
            closed={seg.closed}
            streaming={streaming === true}
          />
        ),
      )}
      <LinkDialog url={link} onClose={() => setLink(null)} />
    </div>
  );
}
