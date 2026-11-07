// Import sessions dialog (owner decision 34, ADR-024). Nothing is read until the dialog opens;
// Cancel imports nothing. Logic lives in importSessions.ts.

import { useCallback, useEffect, useId, useMemo, useRef, useState } from "react";
import { providerName, relativeTime, shortId, stripInvisible } from "./format";
import {
  GUESS_LABELS,
  agentNotes,
  cancelScan,
  countText,
  errorText,
  filterFound,
  groupByFolder,
  groupTick,
  importPicked,
  keyOf,
  pickable,
  picksFrom,
  resultText,
  rowLabel,
  scanLogs,
  sortFound,
  toggleAllShown,
  toggleGroup,
  toggleOne,
  UNKNOWN_FOLDER,
  type FolderGroup,
  type FoundSession,
  type ScanResult,
} from "./importSessions";
import { Banner } from "./ui/Banner";
import { Dialog } from "./ui/Dialog";

interface Props {
  open: boolean;
  now: number;
  onClose: () => void;
  /** Called after an import that changed something, with the confirmation to show. */
  onImported: (message: string) => void;
}

type Phase =
  { kind: "loading" } | { kind: "error"; message: string } | { kind: "ready"; scan: ScanResult };

export function ImportSessionsDialog({ open, now, onClose, onImported }: Props) {
  const [phase, setPhase] = useState<Phase>({ kind: "loading" });
  const [attempt, setAttempt] = useState(0);
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<Set<string>>(() => new Set());
  const [importing, setImporting] = useState(false);
  const [importError, setImportError] = useState<string | null>(null);
  const filterId = useId();
  const countId = useId();
  // After "Try again" the button disappears; focus goes to the status line instead of the page.
  const refocus = useRef(false);
  const focusWhileLoading = useCallback((el: HTMLElement | null) => {
    if (el && refocus.current) el.focus();
  }, []);
  const focusResult = useCallback((el: HTMLElement | null) => {
    if (el && refocus.current) {
      refocus.current = false;
      el.focus();
    }
  }, []);

  // Closing while the logs are still being read stops the scan, so reopening isn't "busy".
  const close = () => {
    if (phase.kind === "loading") cancelScan().catch(() => undefined);
    onClose();
  };

  // Read the logs only while the dialog is open (and again on Try again). The parent mounts a
  // fresh dialog (new key) for each opening, so every opening starts empty.
  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    scanLogs()
      .then((scan) => {
        if (!cancelled) setPhase({ kind: "ready", scan });
      })
      .catch((e: unknown) => {
        if (!cancelled) setPhase({ kind: "error", message: errorText(e) });
      });
    return () => {
      cancelled = true;
    };
  }, [open, attempt]);

  const all = useMemo(
    () => (phase.kind === "ready" ? sortFound(phase.scan.sessions) : []),
    [phase],
  );
  const shown = useMemo(() => filterFound(all, query), [all, query]);
  const groups = useMemo(() => groupByFolder(shown), [shown]);
  const picks = picksFrom(all, selected);
  const shownPickable = pickable(shown);
  const allShownOn = shownPickable.length > 0 && shownPickable.every((s) => selected.has(keyOf(s)));

  const doImport = () => {
    if (picks.length === 0) return;
    setImporting(true);
    setImportError(null);
    importPicked(picks)
      .then((r) => {
        setImporting(false);
        onImported(resultText(r));
        onClose();
      })
      .catch((e: unknown) => {
        setImporting(false);
        setImportError(errorText(e));
      });
  };

  return (
    <Dialog
      open={open}
      title="Import sessions"
      onClose={close}
      busy={importing}
      description={
        <p>
          Lullshift lists sessions from the agents&apos; own logs on this computer. It reads only
          the start and end of each log, and nothing leaves your computer. Titles are saved only for
          agents with Task context turned on. Imported sessions are marked &ldquo;From log&rdquo;
          and never alert you; the agent&apos;s next update takes over.
        </p>
      }
      actions={
        <>
          <button type="button" className="btn btn-quiet" onClick={close} disabled={importing}>
            Cancel
          </button>
          <button
            type="button"
            className="btn btn-primary"
            onClick={doImport}
            disabled={picks.length === 0 || importing || phase.kind !== "ready"}
            aria-describedby={countId}
          >
            {importing ? "Importing…" : `Import (${picks.length})`}
          </button>
        </>
      }
    >
      {phase.kind === "loading" && (
        <p className="import-status" role="status" tabIndex={-1} ref={focusWhileLoading}>
          Reading the agents&apos; logs…
        </p>
      )}
      {phase.kind === "error" && (
        <>
          <Banner tone="danger">{phase.message}</Banner>
          <button
            type="button"
            className="btn btn-secondary"
            ref={focusResult}
            onClick={() => {
              refocus.current = true;
              setPhase({ kind: "loading" });
              setAttempt((n) => n + 1);
            }}
          >
            Try again
          </button>
        </>
      )}
      {phase.kind === "ready" && (
        <div className="import-body">
          {phase.scan.incomplete && (
            <Banner tone="info">
              The list may be incomplete: Lullshift stopped after the newest sessions to stay quick.
            </Banner>
          )}
          {agentNotes(phase.scan.agents).length > 0 && (
            <ul className="import-notes">
              {agentNotes(phase.scan.agents).map((n) => (
                <li key={n}>{n}</li>
              ))}
            </ul>
          )}
          {all.length === 0 ? (
            <p className="import-status" tabIndex={-1} ref={focusResult}>
              No sessions found in the agents&apos; logs.
            </p>
          ) : (
            <>
              <div className="import-tools">
                <label htmlFor={filterId} className="visually-hidden">
                  Filter sessions
                </label>
                <input
                  id={filterId}
                  type="search"
                  className="board-search import-filter"
                  placeholder="Filter by agent, folder, title or ID"
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                />
                <button
                  type="button"
                  className="btn btn-quiet btn-small"
                  disabled={shownPickable.length === 0}
                  aria-pressed={allShownOn}
                  onClick={() => setSelected((s) => toggleAllShown(s, shown))}
                >
                  Select all shown
                </button>
              </div>
              <p
                id={countId}
                className="import-count"
                role="status"
                aria-live="polite"
                tabIndex={-1}
                ref={focusResult}
              >
                {countText(picks.length, shown.length, all.length)}
              </p>
              {shown.length === 0 ? (
                <p className="import-status">No sessions match the filter.</p>
              ) : (
                <div className="import-groups" role="group" aria-label="Sessions found">
                  {groups.map((g) => (
                    <FolderSection
                      key={g.key}
                      group={g}
                      selected={selected}
                      now={now}
                      onToggleGroup={() => setSelected((cur) => toggleGroup(cur, g))}
                      onToggleOne={(s) => setSelected((cur) => toggleOne(cur, s))}
                    />
                  ))}
                </div>
              )}
            </>
          )}
          {importError && <Banner tone="danger">{importError}</Banner>}
        </div>
      )}
    </Dialog>
  );
}

/** One project folder: a heading with its count, a tri-state "select all", and its sessions. */
function FolderSection({
  group,
  selected,
  now,
  onToggleGroup,
  onToggleOne,
}: {
  group: FolderGroup;
  selected: ReadonlySet<string>;
  now: number;
  onToggleGroup: () => void;
  onToggleOne: (s: FoundSession) => void;
}) {
  const headingId = useId();
  const box = useRef<HTMLInputElement>(null);
  const tick = groupTick(group, selected);
  const canTick = pickable(group.sessions).length > 0;
  useEffect(() => {
    if (box.current) box.current.indeterminate = tick === "some";
  }, [tick]);
  const n = group.sessions.length;
  // Folder names come from agents' logs: strip invisible characters again before showing them.
  const label = stripInvisible(group.label) || UNKNOWN_FOLDER;
  const path = group.path ? stripInvisible(group.path) : undefined;
  return (
    <section className="import-group" aria-labelledby={headingId}>
      <div className="import-group-head">
        <h3 id={headingId} className="import-group-title" title={path}>
          <span className="import-group-name">{label}</span>
          <span className="import-group-count">
            {n} {n === 1 ? "session" : "sessions"}
          </span>
        </h3>
        <label className="import-group-all">
          <input
            ref={box}
            type="checkbox"
            checked={tick === "all"}
            disabled={!canTick}
            onChange={onToggleGroup}
            aria-label={`Select all in this folder, ${label}`}
          />
          <span aria-hidden="true">Select all in this folder</span>
        </label>
      </div>
      <ul className="import-list">
        {group.sessions.map((s) => (
          <Row
            key={keyOf(s)}
            s={s}
            now={now}
            checked={selected.has(keyOf(s))}
            onToggle={() => onToggleOne(s)}
          />
        ))}
      </ul>
    </section>
  );
}

function Row({
  s,
  now,
  checked,
  onToggle,
}: {
  s: FoundSession;
  now: number;
  checked: boolean;
  onToggle: () => void;
}) {
  return (
    <li className={`import-row${s.tracked ? " is-tracked" : ""}`}>
      <label>
        <input
          type="checkbox"
          checked={s.tracked ? false : checked}
          disabled={s.tracked}
          onChange={onToggle}
          aria-label={rowLabel(s, now)}
        />
        <span className="import-main">
          <span className="import-title">{s.title ?? `Session ${shortId(s.sessionId)}`}</span>
          <span className="import-meta">
            <span>{providerName(s.provider)}</span>
            <time dateTime={s.lastActivityUtc} title={new Date(s.lastActivityUtc).toLocaleString()}>
              {relativeTime(s.lastActivityUtc, now)}
            </time>
          </span>
        </span>
        <span className="import-guess">
          {s.tracked ? "Already in Lullshift" : GUESS_LABELS[s.guess]}
        </span>
      </label>
    </li>
  );
}
