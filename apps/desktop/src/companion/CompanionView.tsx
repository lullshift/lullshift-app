import { useCallback, useEffect, useState, type RefObject } from "react";
import type { SessionView } from "../sessions";
import { Banner } from "../ui/Banner";
import {
  companionDeleteThread,
  companionNewThread,
  companionPrefs,
  companionProfiles,
  companionRenameThread,
  companionThreads,
  toCommandError,
  type CompanionPrefs,
  type ProviderProfile,
  type Scope,
  type Thread,
} from "./api";
import { Conversation } from "./Conversation";
import { onProvidersChanged } from "./events";
import { errorText, UNAVAILABLE_TEXT } from "./logic";
import { ThreadList } from "./ThreadList";
import "./companion.css";

type Boot =
  | { status: "loading" }
  | { status: "ready" }
  | { status: "unavailable" }
  | { status: "error"; text: string };

interface Props {
  headingRef: RefObject<HTMLHeadingElement | null>;
  sessions: readonly SessionView[];
  now: number;
  /** The user clicked a break the Companion suggested. */
  onStartActivity: (activityId: string) => void;
  /** Opens Settings with the Companion section expanded and focused (decision 36). */
  onOpenSettings: () => void;
}

/**
 * Companion: conversations only. Providers, models and lookups live in Settings (decision 36).
 * The Rust core holds the truth (threads, messages, keys, usage); this component only keeps the
 * last snapshot it was given.
 */
export default function CompanionView({
  headingRef,
  sessions,
  now,
  onStartActivity,
  onOpenSettings,
}: Props) {
  const [boot, setBoot] = useState<Boot>({ status: "loading" });
  const [prefs, setPrefs] = useState<CompanionPrefs>({ toolsEnabled: false, lastScope: null });
  const [profiles, setProfiles] = useState<ProviderProfile[]>([]);
  const [threads, setThreads] = useState<Thread[]>([]);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [live, setLive] = useState("");

  const reload = useCallback(async () => {
    try {
      const [p, t] = await Promise.all([companionProfiles(), companionThreads()]);
      setProfiles(p);
      setThreads(t);
      setBoot({ status: "ready" });
      setSelectedId((cur) =>
        cur !== null && t.some((x) => x.id === cur) ? cur : (t[0]?.id ?? null),
      );
    } catch (err) {
      const e = toCommandError(err);
      setBoot(
        e.kind === "unavailable"
          ? { status: "unavailable" }
          : { status: "error", text: errorText(e) },
      );
    }
  }, []);

  useEffect(() => {
    let live = true;
    // Settings are read apart from the rest: if they can't be read, Companion still works, with
    // lookups off and no remembered context.
    companionPrefs()
      .then((p) => {
        if (live) setPrefs(p);
      })
      .catch(() => undefined);
    Promise.all([companionProfiles(), companionThreads()])
      .then(([p, t]) => {
        if (!live) return;
        setProfiles(p);
        setThreads(t);
        setBoot({ status: "ready" });
        setSelectedId((cur) => cur ?? t[0]?.id ?? null);
      })
      .catch((err: unknown) => {
        if (!live) return;
        const e = toCommandError(err);
        setBoot(
          e.kind === "unavailable"
            ? { status: "unavailable" }
            : { status: "error", text: errorText(e) },
        );
      });
    // Providers or models changed in Settings: read them again.
    const off = onProvidersChanged(() => {
      companionProfiles()
        .then((p) => {
          if (live) setProfiles(p);
        })
        .catch(() => undefined);
      companionPrefs()
        .then((p) => {
          if (live) setPrefs(p);
        })
        .catch(() => undefined);
    });
    return () => {
      live = false;
      off();
    };
  }, []);

  const reloadThreads = useCallback(async () => {
    try {
      setThreads(await companionThreads());
    } catch {
      // Keep the last list; the conversation shows its own errors.
    }
  }, []);

  const run = async (label: string, work: () => Promise<void>) => {
    setNotice(null);
    try {
      await work();
    } catch (err) {
      const text = errorText(toCommandError(err));
      setNotice(`${label}: ${text}`);
      setLive(`${label}: ${text}`);
    }
  };

  const createThread = () =>
    run("Couldn't start a conversation", async () => {
      const t = await companionNewThread();
      setThreads((cur) => [t, ...cur.filter((x) => x.id !== t.id)]);
      setSelectedId(t.id);
      setLive("New conversation started.");
    });

  const renameThread = (id: number, title: string) =>
    run("Couldn't rename it", async () => {
      await companionRenameThread(id, title);
      await reloadThreads();
      setLive("Conversation renamed.");
    });

  const deleteThread = (id: number) =>
    run("Couldn't delete it", async () => {
      await companionDeleteThread(id);
      const next = await companionThreads();
      setThreads(next);
      setSelectedId((cur) => (cur === id ? (next[0]?.id ?? null) : cur));
      setLive("Conversation deleted.");
    });

  const selected = threads.find((t) => t.id === selectedId) ?? null;

  return (
    <section aria-labelledby="companion-heading" className="view companion">
      <header className="view-header companion-header">
        <div>
          <h1 id="companion-heading" ref={headingRef} tabIndex={-1}>
            Companion
          </h1>
          <p className="view-lede">
            Talk with an AI provider using your own key. Nothing from your sessions is shared unless
            you choose it, and you see it first.
          </p>
        </div>
        <button type="button" className="btn-link companion-settings-link" onClick={onOpenSettings}>
          Provider settings
        </button>
      </header>

      <div className="visually-hidden" role="status" aria-live="polite">
        {live}
      </div>

      {boot.status === "loading" && <p className="card-sub">Loading Companion…</p>}
      {boot.status === "unavailable" && (
        <Banner tone="info">{UNAVAILABLE_TEXT} The rest of Lullshift works as usual.</Banner>
      )}
      {boot.status === "error" && (
        <Banner tone="danger">
          {boot.text}{" "}
          <button type="button" className="btn-link" onClick={() => void reload()}>
            Try again
          </button>
        </Banner>
      )}
      {notice && <Banner tone="danger">{notice}</Banner>}

      {boot.status === "ready" && (
        <div className="companion-layout">
          <ThreadList
            threads={threads}
            selectedId={selectedId}
            now={now}
            onSelect={setSelectedId}
            onCreate={() => void createThread()}
            onRename={(id, title) => void renameThread(id, title)}
            onDelete={(id) => void deleteThread(id)}
          />
          {selected ? (
            <Conversation
              key={selected.id}
              thread={selected}
              profiles={profiles}
              sessions={sessions}
              rememberedScope={prefs.lastScope}
              onScopeUsed={(scope: Scope) => setPrefs((p) => ({ ...p, lastScope: scope }))}
              onStartActivity={onStartActivity}
              onOpenSettings={onOpenSettings}
              onActivity={() => void reloadThreads()}
            />
          ) : (
            <div className="card companion-empty">
              <p className="card-title">No conversation yet</p>
              <p className="card-sub">
                Start one to ask a question. Each message sends only what you type unless you choose
                to add context.
              </p>
              <div className="actions">
                <button
                  type="button"
                  className="btn btn-primary"
                  onClick={() => void createThread()}
                >
                  New conversation
                </button>
              </div>
            </div>
          )}
        </div>
      )}
    </section>
  );
}
