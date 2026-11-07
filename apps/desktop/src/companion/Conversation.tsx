import { useCallback, useEffect, useRef, useState } from "react";
import { ellipsize } from "../format";
import type { SessionView } from "../sessions";
import { Banner } from "../ui/Banner";
import {
  companionCancel,
  companionMessages,
  companionRetry,
  companionSend,
  companionToolDecision,
  toCommandError,
  type CommandError,
  type Message,
  type ProviderProfile,
  type Scope,
  type StreamEvent,
  type Thread,
} from "./api";
import { Composer, type SendResult } from "./Composer";
import { acknowledge, runRequest, useIsWatching, useThreadInflight } from "./inflight";
import { answerBadgeText, effectiveChoice, retryChoice, type ModelChoice } from "./modelChoice";
import { ReplyText } from "./ReplyText";
import {
  CONTEXT_CHANGED_NOTE,
  errorText,
  historyCount,
  needsSettings,
  plainLines,
  sharedLabel,
  streamStatus,
  withoutInFlightMessage,
} from "./logic";
import { threadLabel } from "./ThreadList";
import { ToolApprovalCard, ToolNotices, ToolRecords } from "./ToolCards";

interface Props {
  thread: Thread;
  profiles: readonly ProviderProfile[];
  sessions: readonly SessionView[];
  /** The context choice of the last message sent, if any. */
  rememberedScope: Scope | null;
  /** A message went out with this context choice. */
  onScopeUsed: (scope: Scope) => void;
  onOpenSettings: () => void;
  /** The user clicked a suggested activity. */
  onStartActivity: (activityId: string) => void;
  /** Called after a send or retry finishes, so the thread list can refresh its order. */
  onActivity: () => void;
}

type Load = { status: "loading" } | { status: "ready" } | { status: "error"; text: string };

/** Plain text, one block per line. Never HTML, Markdown or links. Used for your own messages. */
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
  profiles,
  host,
  retrying,
  canRetry,
  onRetry,
  onOpenSettings,
  onStartActivity,
}: {
  message: Message;
  profiles: readonly ProviderProfile[];
  host: string | null;
  retrying: boolean;
  canRetry: boolean;
  onRetry: (id: number) => void;
  onOpenSettings: () => void;
  onStartActivity: (activityId: string) => void;
}) {
  const mine = message.role === "user";
  const failed = mine && message.status === "failed";
  const unconfirmed = mine && message.status === "pending" && !retrying;
  return (
    <li className={`msg ${mine ? "msg-user" : "msg-assistant"}`}>
      <span className="msg-who">{mine ? "You" : "Companion"}</span>
      {mine ? <PlainText text={message.text} /> : <ReplyText text={message.text} />}
      {!mine && (
        <p className="msg-model">
          <span className="visually-hidden">Answered by </span>
          {answerBadgeText(message, profiles)}
        </p>
      )}
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
      {mine && (message.toolCalls?.length ?? 0) > 0 && (
        <ToolRecords records={message.toolCalls} onStartActivity={onStartActivity} />
      )}
      {mine && (message.toolNotices?.length ?? 0) > 0 && (
        <ToolNotices notices={message.toolNotices} />
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
                  Open provider settings
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
  sessions,
  rememberedScope,
  onScopeUsed,
  onOpenSettings,
  onStartActivity,
  onActivity,
}: Props) {
  const [load, setLoad] = useState<Load>({ status: "loading" });
  const [messages, setMessages] = useState<Message[]>([]);
  const [problem, setProblem] = useState<CommandError | null>(null);
  const [live, setLive] = useState("");
  // A send refused because the context changed, which this screen was not around to handle: the
  // typed text, offered back to the message box.
  const [contextOffer, setContextOffer] = useState<string | null>(null);
  const [restore, setRestore] = useState<{ text: string; nonce: number } | null>(null);
  const mountedRef = useRef(false);
  // True once this screen itself got the result of the send (the composer handles it then).
  const composerHandled = useRef(false);
  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);
  const [deciding, setDeciding] = useState(false);
  // The request in flight for this thread lives in the app-level store (inflight.ts), not here:
  // it keeps running and keeps receiving events while this component is not on screen.
  const entry = useThreadInflight(thread.id);
  const watching = useIsWatching(thread.id);
  const sending = entry?.status === "running";
  // A request this component saw running stays on screen after it ends until the stored reply
  // has been read, so the reply never disappears for a moment. A request that ended while this
  // thread was not open shows only its stored result.
  const [seen, setSeen] = useState<string | null>(null);
  const [settled, setSettled] = useState<string | null>(null);
  if (entry?.status === "running" && seen !== entry.requestId) setSeen(entry.requestId);
  const shown =
    entry &&
    (entry.status === "running" || (seen === entry.requestId && settled !== entry.requestId))
      ? entry
      : null;
  const stream = shown?.stream ?? null;
  const pendingText = shown?.optimisticText ?? null;
  const retryingId = shown?.retryId ?? null;
  const finished = entry?.status === "finished" && entry.outcome ? entry : null;
  const finishedId = finished?.requestId ?? null;
  // The provider and model picked for the next message only; null means the default.
  const [picked, setPicked] = useState<ModelChoice | null>(null);
  const endRef = useRef<HTMLDivElement>(null);
  const choice = effectiveChoice(profiles, picked);
  const profile = profiles.find((p) => p.id === choice?.profileId) ?? null;
  const host = profile?.host ?? null;
  const earlier = historyCount(messages, host);
  // One short sentence per phase for screen readers; it does not change per token.
  const status = streamStatus(stream, host);

  // Every read of the thread takes a ticket, and only the newest read may set the list: reads can
  // finish out of order, and an older one (before the reply was stored) must not win.
  const loadSeq = useRef(0);

  const reload = useCallback(async (): Promise<Message[] | null> => {
    const ticket = ++loadSeq.current;
    try {
      const list = await companionMessages(thread.id);
      if (loadSeq.current === ticket) {
        setMessages(list);
        setLoad({ status: "ready" });
      }
      return list;
    } catch (err) {
      if (loadSeq.current === ticket) {
        setLoad({ status: "error", text: errorText(toCommandError(err)) });
      }
      return null;
    }
  }, [thread.id]);

  useEffect(() => {
    let live = true;
    const ticket = ++loadSeq.current;
    companionMessages(thread.id)
      .then((list) => {
        if (!live || loadSeq.current !== ticket) return;
        setMessages(list);
        setLoad({ status: "ready" });
      })
      .catch((err: unknown) => {
        if (live && loadSeq.current === ticket) {
          setLoad({ status: "error", text: errorText(toCommandError(err)) });
        }
      });
    return () => {
      live = false;
    };
  }, [thread.id]);

  useEffect(() => {
    endRef.current?.scrollIntoView({ block: "end" });
  }, [messages.length, pendingText]);

  // A request of this thread ended (while this screen was open or not): read the stored result,
  // say how it went, and, once the user can see this thread, clear its unread mark.
  useEffect(() => {
    if (!finished || !finished.outcome) return;
    const o = finished.outcome;
    let current = true;
    const done = () => {
      if (current) setSettled(finished.requestId);
    };
    const ticket = ++loadSeq.current;
    companionMessages(thread.id)
      .then((list) => {
        if (!current) return;
        if (loadSeq.current === ticket) {
          setMessages(list);
          setLoad({ status: "ready" });
        }
        if (o.kind === "replied") {
          setLive(`Reply received. ${ellipsize(o.text.replace(/\s+/g, " "), 600)}`);
        } else if (o.kind === "failed") {
          const text = o.contextChanged ? CONTEXT_CHANGED_NOTE : errorText(o.error, finished.host);
          setLive(`Message not sent. ${text}`);
          // A stale preview is handled by the composer, which shows the new preview. Otherwise a
          // failed message in the thread shows its own error; show this only if nothing was stored.
          if (!o.stored && !o.contextChanged) setProblem(o.error);
          // The composer shows the new preview if it was here for the result; if you left and
          // came back it has nothing, so say so and offer the text back.
          if (o.contextChanged && !composerHandled.current && finished.optimisticText) {
            setContextOffer(finished.optimisticText);
          }
        } else {
          setLive("Cancelled.");
        }
        setDeciding(false);
        done();
      })
      .catch((err: unknown) => {
        if (current && loadSeq.current === ticket) {
          setLoad({ status: "error", text: errorText(toCommandError(err)) });
        }
        done();
      });
    return () => {
      current = false;
    };
  }, [finished, thread.id]);

  useEffect(() => {
    if (watching && finishedId !== null && settled === finishedId) {
      acknowledge(thread.id, finishedId);
    }
  }, [watching, finishedId, settled, thread.id]);

  /** Starts a send or retry. It runs to its end in the store, whether or not this is on screen. */
  const track = async (
    before: number,
    to: ModelChoice,
    work: (requestId: string, onEvent: (e: StreamEvent) => void) => Promise<Message>,
    options: { optimisticText: string | null; retryId: number | null },
  ): Promise<SendResult> => {
    setProblem(null);
    setContextOffer(null);
    setDeciding(false);
    composerHandled.current = false;
    const outcome = await runRequest(
      {
        threadId: thread.id,
        title: thread.title,
        optimisticText: options.optimisticText,
        retryId: options.retryId,
        profileId: to.profileId,
        model: to.model,
        host,
        before,
      },
      work,
    );
    if (mountedRef.current) composerHandled.current = true;
    onActivity();
    return {
      stored: outcome.stored,
      contextChanged: outcome.kind === "failed" && outcome.contextChanged,
    };
  };

  const send = async (text: string, scope: Scope, expectedSha256: string | null) => {
    if (!choice) return { stored: false, contextChanged: false };
    const to = choice;
    const result = await track(
      messages.length,
      to,
      (requestId, onEvent) =>
        companionSend(
          thread.id,
          to.profileId,
          to.model,
          text,
          scope,
          expectedSha256,
          requestId,
          onEvent,
        ),
      {
        optimisticText: text,
        retryId: null,
      },
    );
    if (result.stored) {
      onScopeUsed(scope);
      // The pick was for this message only: the next one goes to the default again.
      setPicked(null);
    }
    return result;
  };

  const retry = (id: number) => {
    const m = messages.find((x) => x.id === id);
    const to = m ? retryChoice(profiles, m, choice) : null;
    if (!to) {
      setProblem({ kind: "notFound", message: "Set up a provider first." });
      return;
    }
    void track(
      messages.length,
      to,
      (requestId, onEvent) => companionRetry(id, to.profileId, to.model, requestId, onEvent),
      {
        optimisticText: null,
        retryId: id,
      },
    );
  };

  const decide = async (approve: boolean) => {
    const pending = stream?.pending;
    if (!stream || !pending) return;
    setDeciding(true);
    try {
      await companionToolDecision(stream.requestId, pending.approvalId, approve);
      setLive(approve ? "Lookup approved and sent." : "Lookup declined.");
    } catch (err) {
      setLive(errorText(toCommandError(err), host));
    } finally {
      setDeciding(false);
    }
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
      <h2 className="conversation-title">
        <bdi>{threadLabel(thread)}</bdi>
      </h2>
      <div className="visually-hidden" role="status" aria-live="polite">
        {status || live}
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
          {withoutInFlightMessage(messages, pendingText).map((m) => (
            <MessageItem
              key={m.id}
              message={m}
              profiles={profiles}
              host={host}
              retrying={retryingId === m.id}
              canRetry={!sending}
              onRetry={retry}
              onOpenSettings={onOpenSettings}
              onStartActivity={onStartActivity}
            />
          ))}
          {pendingText !== null && (
            <li className="msg msg-user msg-sending">
              <span className="msg-who">You</span>
              <PlainText text={pendingText} />
              <p className="msg-note">{stream && stream.text ? "Sent." : "Sending…"}</p>
            </li>
          )}
          {stream && stream.text !== "" && (
            // Not a live region: the text grows token by token. A short status sentence in the
            // hidden live region tells screen-reader users the reply is coming, and the finished
            // reply is announced once.
            <li className="msg msg-assistant msg-streaming" aria-busy="true">
              <span className="msg-who">Companion</span>
              <ReplyText text={stream.text} streaming />
              <p className="msg-note">Still writing. You can cancel at any time.</p>
            </li>
          )}
          {stream && stream.notices.length > 0 && (
            <li>
              <ToolNotices notices={stream.notices} />
            </li>
          )}
        </ol>
        {stream?.pending && (
          <ToolApprovalCard
            pending={stream.pending}
            busy={deciding}
            onDecide={(approve) => void decide(approve)}
          />
        )}
        <div ref={endRef} />
      </div>

      {contextOffer !== null && (
        <Banner tone="danger">
          <strong>Not sent.</strong> {CONTEXT_CHANGED_NOTE}{" "}
          <button
            type="button"
            className="btn-link"
            onClick={() => {
              setRestore((r) => ({ text: contextOffer, nonce: (r?.nonce ?? 0) + 1 }));
              setContextOffer(null);
            }}
          >
            Put my message back
          </button>
        </Banner>
      )}

      {problem && (
        <Banner tone="danger">
          <strong>Not sent.</strong> {errorText(problem, host)}
          {needsSettings(problem.kind) && (
            <>
              {" "}
              <button type="button" className="btn-link" onClick={onOpenSettings}>
                Open provider settings
              </button>
            </>
          )}
        </Banner>
      )}

      <Composer
        profiles={profiles}
        choice={choice}
        onPickChoice={setPicked}
        sessions={sessions}
        sending={sending}
        historyCount={earlier}
        rememberedScope={rememberedScope}
        onSend={send}
        onCancel={() => void cancel()}
        onOpenSettings={onOpenSettings}
        restore={restore}
      />
    </div>
  );
}
