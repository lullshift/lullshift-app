import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import {
  failureText,
  fixErrorText,
  fixExplanation,
  fixResultText,
  packagedWarning,
  scanNote,
  staleDetail,
  staleHeadline,
  type ConnectionHealth,
  type FixReport,
  type PackagedContext,
  type PackageRef,
} from "./connection";
import { Banner } from "./ui/Banner";

type Message = { tone: "success" | "info" | "danger"; text: string };

/**
 * Persistent warning while Lullshift itself runs inside another app's package (decision 33).
 * Shows nothing when it doesn't, or when that can't be told (the warning is a hint, not a gate).
 */
export function PackagedBanner() {
  const [pkg, setPkg] = useState<PackageRef | null>(null);
  useEffect(() => {
    let cancelled = false;
    invoke<PackagedContext>("packaged_context")
      .then((c) => {
        if (!cancelled) setPkg(c.package ?? null);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, []);
  return pkg ? <Banner tone="attention">{packagedWarning(pkg)}</Banner> : null;
}

/** The connection problems found, with a Fix per package. Presentational; see below. */
export function ConnectionProblemsView({
  health,
  busy,
  onFix,
}: {
  health: ConnectionHealth;
  busy: string | null;
  onFix: (pkg: PackageRef) => void;
}) {
  const families = new Set(health.stale.map((s) => s.package.family));
  const loose = health.failures.filter((f) => !f.package || !families.has(f.package.family));
  const note = scanNote(health.scan);
  return (
    <>
      {loose.map((f) => (
        <Banner tone="attention" key={`${f.adapter}-${f.package?.family ?? "local"}-${f.utc}`}>
          {failureText(f)}
        </Banner>
      ))}
      {health.stale.map((copy) => {
        const id = `stale-${copy.package.family}`;
        const notes = health.failures.filter((f) => f.package?.family === copy.package.family);
        return (
          <article
            className="card integration"
            aria-labelledby={id}
            aria-busy={busy === copy.package.family}
            key={copy.package.family}
          >
            <h2 id={id} className="card-title">
              {staleHeadline(copy)}
            </h2>
            {notes.map((f) => (
              <p className="card-text" key={`${f.adapter}-${f.utc}`}>
                {failureText(f)}
              </p>
            ))}
            <p className="card-text">{staleDetail(copy.files)}</p>
            <p className="card-text">{fixExplanation(copy.package)}</p>
            <div className="actions">
              <button
                type="button"
                className="btn btn-primary"
                disabled={busy !== null}
                aria-label={`Fix ${copy.package.name}`}
                onClick={() => onFix(copy.package)}
              >
                {busy === copy.package.family ? "Fixing…" : "Fix"}
              </button>
            </div>
          </article>
        );
      })}
      {note && <p className="card-sub">{note}</p>}
    </>
  );
}

/**
 * Integrations: out-of-date connection files in other apps' private storage and recent hook
 * failures (decision 33). Reads on open; deletes only when the user presses Fix.
 */
export default function ConnectionProblems() {
  const [health, setHealth] = useState<ConnectionHealth | null>(null);
  const [failed, setFailed] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [message, setMessage] = useState<Message | null>(null);
  /** Always present, so the result is announced; focus moves here after Fix. */
  const resultRef = useRef<HTMLDivElement>(null);

  const refresh = () =>
    invoke<ConnectionHealth>("connection_health")
      .then((h) => {
        setHealth(h);
        setFailed(false);
      })
      .catch(() => setFailed(true));

  useEffect(() => {
    void refresh();
  }, []);

  const fix = async (pkg: PackageRef) => {
    setBusy(pkg.family);
    setMessage(null);
    try {
      const r = await invoke<FixReport>("connection_fix_stale", { family: pkg.family });
      setMessage(fixResultText(pkg, r));
    } catch (err: unknown) {
      setMessage({ tone: "danger", text: fixErrorText(String(err)) });
    } finally {
      setBusy(null);
      void refresh();
      // The card (and its button) usually goes away; keep focus on the result instead.
      requestAnimationFrame(() => resultRef.current?.focus());
    }
  };

  return (
    <>
      <div
        ref={resultRef}
        tabIndex={-1}
        aria-live={message?.tone === "danger" ? "assertive" : "polite"}
        aria-atomic="true"
      >
        {message && (
          <Banner tone={message.tone} live={false}>
            {message.text}
          </Banner>
        )}
      </div>
      {failed && (
        <p className="card-sub">
          Lullshift couldn't check whether agents in other apps can reach it.
        </p>
      )}
      {health && <ConnectionProblemsView health={health} busy={busy} onFix={(p) => void fix(p)} />}
    </>
  );
}
