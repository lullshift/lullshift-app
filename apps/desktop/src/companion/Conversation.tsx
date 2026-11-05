import { useCallback, useEffect, useRef, useState } from "react";
import { ellipsize } from "../format";
import type { SessionView } from "../sessions";
import { Banner } from "../ui/Banner";
import {
  companionCancel,
  companionMessages,
  companionRetry,
  companionSend,
  toCommandError,
  type CommandError,
  type Message,
  type ProviderProfile,
  type Scope,
  type Thread,
} from "./api";
import { Composer, type SendResult } from "./Composer";
import {
  CONTEXT_CHANGED_NOTE,
  errorText,
  historyCount,
  isContextChanged,
  needsSettings,
  plainLines,
  sharedLabel,
} from "./logic";
import { threadLabel } from "./ThreadList";

interface Props {
  thread: Thread;
  profiles: readonly ProviderProfile[];
  profile: ProviderProfile | null;
  onPickProfile: (id: string) => void;
  sessions: readonly SessionView[];
  onOpenSettings: () => void;
  /** Called after a send or retry finishes, so the thread list can refresh its order. */
  onActivity: () => void;
}

type Load = { status: "loading" } | { status: "ready" } | { status: "error"; text: string };

/** Plain text, one block per line. Never HTML, Markdown or links. */
function PlainText({ text, className }: { text: string; className?: string }) {
  return (
    <div className={className ?? "msg-text"}>
      {plainLines(text).map((line, i) => (
        <span key={i} className="msg-line">
          {line === "" ? " " : line}
        </span>
      ))}
    </div>
  );
}

function MessageItem({
  message,
  host,
  retrying,
  canRetry,
  onRetry,
  onOpenSettings,
}: {
  message: Message;
  host: string | null;
  retrying: boolean;
  canRetry: boolean;
  onRetry: (id: number) => void;
  onOpenSettings: () => void;
}) {
  const mine = message.role === "user";
  const failed = mine && message.status === "failed";
  const unconfirmed = mine && message.status === "pending" && !retrying;
  return (
    <li className={`msg ${mine ? "msg-user" : "msg-assistant"}`}>
      <span className="msg-who">{mine ? "You" : "Companion"}</span>
      <PlainText text={message.text} />
      {mine && message.attachedContext !== null && message.scopeKind !== "none" && (
        <details className="msg-shared">
          <summary>{sharedLabel(message.scopeKind, host)}</summary>
          <p className="field-note">
            This is the exact text that left this computer with the message.
          </p>
          <PlainText text={message.attachedContext} className="msg-attached" />
        </details>
      )}
      {mine && message.scopeKind === "none" && (
        <p className="msg-note">Nothing shared besides this message.</p>
      )}
      {retrying && <p className="msg-note">Sending again…</p>}
      {(failed || unconfirmed) && !retrying && (
        <div className="msg-failed">
          <Banner tone="danger">
            <strong>Not sent.</strong>{" "}
            {failed && message.error
              ? errorText(message.error, host)
              : "This message may not have reached the provider."}
            {failed && message.error && needsSettings(message.error.kind) && (
              <>
                {" "}
                <button type="button" className="btn-link" onClick={onOpenSettings}>
                  Open Companion settings
                </button>
              </>
            )}
          </Banner>
          <button
            type="button"
            className="btn btn-secondary"
            disabled={!canRetry}
            onClick={() => onRetry(message.id)}
          >
            Try again
          </button>
        </div>
      )}
    </li>
  );
}

export function Conversation({
  thread,
  profiles,
  profile,
  onPickProfile,
  sessions,
  onOpenSettings,
  onActivity,
}: Props) {
  const [load, setLoad] = useState<Load>({ status: "loading" });
  const [messages, setMessages] = useState<Message[]>([]);
  const [sending, setSending] = useState(false);
  const [pendingText, setPendingText] = useState<string | null>(null);
  const [retryingId, setRetryingId] = useState<number | null>(null);
  const [problem, setProblem] = useState<CommandError | null>(null);
  const [live, setLive] = useState("");
  const endRef = useRef<HTMLDivElement>(null);
  const host = profile?.host ?? null;
  const earlier = historyCount(messages, host);

  const reload = useCallback(async (): Promise<Message[] | null> => {
    try {
      const list = await companionMessages(thread.id);
      setMessages(list);
      setLoad({ status: "ready" });
      return list;
    } catch (err) {
      setLoad({ status: "error", text: errorText(toCommandError(err)) });
      return null;
    }
  }, [thread.id]);

  useEffect(() => {
    let live = true;
    companionMessages(thread.id)
      .then((list) => {
        if (!live) return;
        setMessages(list);
        setLoad({ status: "ready" });
      })
      .catch((err: unknown) => {
        if (live) setLoad({ status: "error", text: errorText(toCommandError(err)) });
      });
    return () => {
      live = false;
    };
  }, [thread.id]);

  useEffect(() => {
    endRef.current?.scrollIntoView({ block: "end" });
  }, [messages.length, pendingText]);

  /** Runs a send or retry; resolves true once the message is stored in the thread. */
  const track = async (
    before: number,
    work: () => Promise<Message>,
    options: { optimisticText: string | null; retryId: number | null },
  ): Promise<SendResult> => {
    setSending(true);
    setProblem(null);
    setPendingText(options.optimisticText);
    setRetryingId(options.retryId);
    setLive(`Sending to ${host ?? "the provider"}…`);
    let stored: boolean;
    let contextChanged = false;
    try {
      const reply = await work();
      const list = await reload();
      stored = list ? list.length > before : true;
      setLive(`Reply received. ${ellipsize(reply.text.replace(/\s+/g, " "), 600)}`);
    } catch (err) {
      const e = toCommandError(err);
      const list = await reload();
      stored = options.retryId !== null || (list ? list.length > before : false);
      contextChanged = options.retryId === null && isContextChanged(e);
      const text = contextChanged ? CONTEXT_CHANGED_NOTE : errorText(e, host);
      setLive(`Message not sent. ${text}`);
      // A stale preview is handled by the composer, which shows the new preview. Otherwise a failed
      // message in the thread shows its own error; show this only if nothing was stored.
      if (!stored && !contextChanged) setProblem(e);
    } finally {
      setSending(false);
      setPendingText(null);
      setRetryingId(null);
      onActivity();
    }
    return { stored, contextChanged };
  };

  const send = (text: string, scope: Scope, expectedSha256: string | null) => {
    if (!profile) return Promise.resolve({ stored: false, contextChanged: false });
    return track(
      messages.length,
      () => companionSend(thread.id, profile.id, text, scope, expectedSha256),
      {
        optimisticText: text,
        retryId: null,
      },
    );
  };

  const retry = (id: number) => {
    void track(messages.length, () => companionRetry(id), { optimisticText: null, retryId: id });
  };

  const cancel = async () => {
    setLive("Cancelling…");
    try {
      await companionCancel(thread.id);
    } catch (err) {
      setLive(errorText(toCommandError(err), host));
    }
  };

  return (
    <div className="conversation">
      <h2 className="conversation-title">{threadLabel(thread)}</h2>
      <div className="visually-hidden" role="status" aria-live="polite">
        {live}
      </div>

      <div className="card messages" tabIndex={0} aria-label="Messages">
        {load.status === "loading" && <p className="card-sub">Loading messages…</p>}
        {load.status === "error" && (
          <Banner tone="danger">
            {load.text}{" "}
            <button type="button" className="btn-link" onClick={() => void reload()}>
              Try again
            </button>
          </Banner>
        )}
        {load.status === "ready" && messages.length === 0 && pendingText === null && (
          <p className="card-sub">
            Nothing here yet. Messages stay on this computer, except what you send to the provider.
          </p>
        )}
        <ol className="msg-list">
          {messages.map((m) => (
            <MessageItem
              key={m.id}
              message={m}
              host={host}
              retrying={retryingId === m.id}
              canRetry={!sending}
              onRetry={retry}
              onOpenSettings={onOpenSettings}
            />
          ))}
          {pendingText !== null && (
            <li className="msg msg-user msg-sending">
              <span className="msg-who">You</span>
              <PlainText text={pendingText} />
              <p className="msg-note">Sending…</p>
            </li>
          )}
        </ol>
        <div ref={endRef} />
      </div>

      {problem && (
        <Banner tone="danger">
          <strong>Not sent.</strong> {errorText(problem, host)}
          {needsSettings(problem.kind) && (
            <>
              {" "}
              <button type="button" className="btn-link" onClick={onOpenSettings}>
                Open Companion settings
              </button>
            </>
          )}
        </Banner>
      )}

      <Composer
        profiles={profiles}
        profile={profile}
        onPickProfile={onPickProfile}
        sessions={sessions}
        sending={sending}
        historyCount={earlier}
        onSend={send}
        onCancel={() => void cancel()}
        onOpenSettings={onOpenSettings}
      />
    </div>
  );
}
