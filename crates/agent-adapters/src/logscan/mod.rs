//! "Import sessions" (owner decision 34, ADR-024): find sessions in the agents' own local logs,
//! only when the user asks.
//!
//! Every log is untrusted input. The scan reads only the first [`Limits::head_bytes`] and the
//! last [`Limits::tail_bytes`] of a file, never the whole file; skips symlinks, junctions and
//! other reparse points and network paths; skips malformed lines; and stops at a file budget, a
//! session cap and a deadline, saying so (`incomplete`). No path read from a log is ever opened:
//! the working directory is display text only. Nothing is written and nothing leaves the device.
//!
//! What is kept per session: the agent, the session id exactly as that agent's hooks send it
//! (so a later hook event merges into the same session), the project folder (display only), a
//! title (the agent's own saved title, else the first line of the first prompt, cleaned and cut
//! to [`MAX_TITLE_CHARS`]), the time of the last write and a status guess. Titles are never
//! logged.

mod claude;
mod codex;
mod cursor;
mod fsutil;
mod opencode;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, SystemTime};

use time::OffsetDateTime;

use crate::redact;

pub use fsutil::Budget;

/// Characters of a title shown in the dialog (and kept, with consent).
pub const MAX_TITLE_CHARS: usize = 80;
/// Characters of a project path kept for display.
const MAX_PATH_CHARS: usize = 512;
/// A session id the scan accepts: what the four agents use (UUIDs, `ses_…`), nothing else.
const MAX_ID_CHARS: usize = 128;

/// Written this recently and the turn not ended: Working.
pub const WORKING_WITHIN: Duration = Duration::from_secs(120);
/// A tool call without a result, quiet for longer than this: May need you.
pub const PENDING_QUIET: Duration = Duration::from_secs(30);
/// …but not once the log has been quiet this long (an abandoned session, not a waiting one).
pub const PENDING_MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);

/// The agents whose logs can be imported, by adapter id.
pub const PROVIDERS: [&str; 4] = [
    crate::claude_code::ADAPTER_ID,
    crate::cursor::ADAPTER_ID,
    crate::codex::ADAPTER_ID,
    crate::opencode::ADAPTER_ID,
];

/// What the end of the log suggests. A guess, never an observed state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Guess {
    Working,
    /// The last turn ended: the agent is waiting for the user's next prompt.
    Waiting,
    /// A tool call has no result and the log has gone quiet: perhaps a permission prompt.
    MayNeedYou,
    Unknown,
}

impl Guess {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::Waiting => "waiting",
            Self::MayNeedYou => "may_need_you",
            Self::Unknown => "unknown",
        }
    }
}

/// One session found in a log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub provider: &'static str,
    pub session_id: String,
    /// The working directory as the log states it (display only, cleaned and capped).
    pub project_path: Option<String>,
    /// The agent's own saved title (Claude Code `custom-title`/`agent-name`, Codex thread name,
    /// OpenCode session title).
    pub agent_title: Option<String>,
    /// The first line of the first real user prompt.
    pub first_prompt: Option<String>,
    pub last_activity: OffsetDateTime,
    pub guess: Guess,
}

impl Found {
    /// What the dialog shows: the agent's title, else the first prompt line.
    pub fn title(&self) -> Option<&str> {
        self.agent_title.as_deref().or(self.first_prompt.as_deref())
    }
}

/// Whether an agent's logs were there to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogsState {
    /// The log folder (or database) exists and was read.
    Read,
    /// No log folder: the agent is not installed here, or keeps its logs elsewhere.
    NotFound,
    /// The folder or database exists but could not be read (or its format is unknown).
    Unreadable,
    /// The folder is on a network drive (or a drive whose type is unknown): not read.
    NotLocal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentLogs {
    pub provider: &'static str,
    pub state: LogsState,
    pub sessions: usize,
    /// A cap, the deadline or a cancel stopped this agent's part early.
    pub incomplete: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Report {
    /// Newest first.
    pub sessions: Vec<Found>,
    pub agents: Vec<AgentLogs>,
    /// A cap or the deadline stopped the scan: older sessions may be missing.
    pub incomplete: bool,
    /// The user cancelled; the report is partial and should be dropped.
    pub cancelled: bool,
}

/// How much the scan may read and for how long.
#[derive(Debug, Clone)]
pub struct Limits {
    pub head_bytes: u64,
    pub tail_bytes: u64,
    /// Sessions kept, newest by last write.
    pub max_sessions: usize,
    /// Directory entries looked at, over all agents.
    pub max_entries: usize,
    pub deadline: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            head_bytes: 64 * 1024,
            tail_bytes: 256 * 1024,
            max_sessions: 500,
            max_entries: 20_000,
            deadline: Duration::from_secs(5),
        }
    }
}

/// Where each agent keeps its logs. `None`: not looked for.
#[derive(Debug, Clone, Default)]
pub struct Roots {
    /// `<claude config>/projects`.
    pub claude_projects: Option<PathBuf>,
    /// `~/.cursor/projects`.
    pub cursor_projects: Option<PathBuf>,
    /// `$CODEX_HOME` or `~/.codex`.
    pub codex_home: Option<PathBuf>,
    /// OpenCode 2.x database file.
    pub opencode_db: Option<PathBuf>,
}

impl Roots {
    /// The same locations the adapters use: `$CLAUDE_CONFIG_DIR` (else `~/.claude`),
    /// `~/.cursor`, `$CODEX_HOME` (absolute only, else `~/.codex`), and OpenCode's
    /// `$OPENCODE_DB` (absolute only) else `$XDG_DATA_HOME/opencode/opencode.db`
    /// (else `~/.local/share/opencode/opencode.db`), on every platform.
    pub fn from_env() -> Self {
        let var = |k: &str| {
            std::env::var_os(k)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        let home = var(if cfg!(windows) { "USERPROFILE" } else { "HOME" });
        let claude = crate::claude_code::install::default_settings_path()
            .and_then(|s| s.parent().map(|d| d.join("projects")));
        let opencode_db = var("OPENCODE_DB").filter(|p| p.is_absolute()).or_else(|| {
            var("XDG_DATA_HOME")
                .filter(|p| p.is_absolute())
                .or_else(|| home.as_ref().map(|h| h.join(".local").join("share")))
                .map(|d| d.join("opencode").join("opencode.db"))
        });
        Self {
            claude_projects: claude,
            cursor_projects: home.as_ref().map(|h| h.join(".cursor").join("projects")),
            codex_home: crate::codex::install::codex_home(),
            opencode_db,
        }
    }
}

/// A log file that may hold a session, before it is read.
pub(crate) struct Candidate {
    pub provider: &'static str,
    pub session_id: String,
    pub path: PathBuf,
    pub modified: SystemTime,
}

/// What a parser takes from one file's windows.
#[derive(Debug, Default)]
pub(crate) struct Parsed {
    pub cwd: Option<String>,
    pub title: Option<String>,
    pub first_prompt: Option<String>,
    pub turn: Turn,
}

/// The state of the last turn, from the end of the log.
#[derive(Debug, Default)]
pub(crate) struct Turn {
    /// The last turn-related record was a turn end.
    pub ended: bool,
    /// Tool calls with no result yet.
    pub pending: HashSet<String>,
}

impl Turn {
    pub fn activity(&mut self) {
        self.ended = false;
    }
    pub fn end(&mut self) {
        self.ended = true;
        self.pending.clear();
    }
    pub fn call(&mut self, id: &str) {
        self.ended = false;
        if self.pending.len() < 256 {
            self.pending.insert(id.to_owned());
        }
    }
    pub fn result(&mut self, id: &str) {
        self.ended = false;
        self.pending.remove(id);
    }
}

/// The status guess (ADR-024): turn ended → Waiting; a pending tool call quiet for over 30 s
/// (and under a day) → May need you, only for agents whose logs record tool results; written
/// in the last 2 minutes → Working; anything else → Unknown.
pub fn guess(turn_ended: bool, pending: bool, age: Duration, pending_counts: bool) -> Guess {
    if turn_ended {
        Guess::Waiting
    } else if pending_counts && pending && age > PENDING_QUIET && age < PENDING_MAX_AGE {
        Guess::MayNeedYou
    } else if age < WORKING_WITHIN {
        Guess::Working
    } else {
        Guess::Unknown
    }
}

/// True for an id the scan will report: 1 to 128 of ASCII letters, digits, `-`, `_` and `.`,
/// not starting with a dot.
pub fn is_session_id(s: &str) -> bool {
    (1..=MAX_ID_CHARS).contains(&s.len())
        && !s.starts_with('.')
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

/// A title for display: cleaned (control, bidi and invisible characters removed, secret-like
/// text hidden), the first non-empty line, whitespace collapsed, cut to [`MAX_TITLE_CHARS`]
/// with an ellipsis. `None` if nothing is left.
pub fn clean_title(raw: &str) -> Option<String> {
    // Bound the work before cleaning: a title never needs more than a few hundred characters.
    let bounded: String = raw.chars().take(2_000).collect();
    let cleaned = redact::clean(&bounded, 2_000);
    let line = cleaned.lines().map(str::trim).find(|l| !l.is_empty())?;
    let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= MAX_TITLE_CHARS {
        return Some(line);
    }
    let cut: String = line.chars().take(MAX_TITLE_CHARS - 1).collect();
    Some(format!("{}…", cut.trim_end()))
}

/// A working directory for display: control and invisible characters removed, one line,
/// capped. Never opened.
pub fn clean_path(raw: &str) -> Option<String> {
    let s: String = raw
        .chars()
        .take(MAX_PATH_CHARS)
        .filter(|c| !c.is_control() && !agent_core::text::is_invisible(*c))
        .collect();
    let s = s.trim().to_owned();
    (!s.is_empty()).then_some(s)
}

/// A user prompt as a title candidate: skips text that is markup the agent added (it starts
/// with `<`), else the cleaned first line.
pub(crate) fn prompt_title(text: &str) -> Option<String> {
    if text.trim_start().starts_with('<') {
        return None;
    }
    clean_title(text)
}

/// Finds and reads sessions. Runs synchronously; call it off the UI thread.
pub fn scan(roots: &Roots, limits: &Limits, now: OffsetDateTime) -> Report {
    scan_with(roots, limits, now, None)
}

/// [`scan`] that stops early (and reports `cancelled`) once `cancel` is set.
pub fn scan_with(
    roots: &Roots,
    limits: &Limits,
    now: OffsetDateTime,
    cancel: Option<Arc<AtomicBool>>,
) -> Report {
    let mut budget = Budget::new(limits.deadline, limits.max_entries);
    if let Some(c) = cancel {
        budget = budget.with_cancel(c);
    }
    // Each file-based agent gets its own share of the entry budget, so one agent with a huge
    // log folder can't hide another (security review L4).
    let share = limits.max_entries / 3;
    let mut agents = Vec::new();
    let mut candidates = Vec::new();

    let mut gather = |provider: &'static str,
                      root: Option<&Path>,
                      find: fn(&Path, &mut Budget) -> Result<Vec<Candidate>, ()>,
                      budget: &Budget| {
        let mut own = budget.share(share);
        let state = match root.filter(|r| fsutil::plain_dir(r).is_some()) {
            None => LogsState::NotFound,
            Some(r) if !fsutil::is_local(r) => LogsState::NotLocal,
            Some(r) => match find(r, &mut own) {
                Ok(found) => {
                    candidates.extend(found);
                    LogsState::Read
                }
                Err(()) => LogsState::Unreadable,
            },
        };
        agents.push(AgentLogs {
            provider,
            state,
            sessions: 0,
            incomplete: own.incomplete,
        });
    };
    gather(
        crate::claude_code::ADAPTER_ID,
        roots.claude_projects.as_deref(),
        claude::find,
        &budget,
    );
    gather(
        crate::cursor::ADAPTER_ID,
        roots.cursor_projects.as_deref(),
        cursor::find,
        &budget,
    );
    let codex_sessions = roots.codex_home.as_ref().map(|h| h.join("sessions"));
    gather(
        crate::codex::ADAPTER_ID,
        codex_sessions.as_deref(),
        codex::find,
        &budget,
    );
    let mark = |agents: &mut Vec<AgentLogs>, provider: &str| {
        if let Some(a) = agents.iter_mut().find(|a| a.provider == provider) {
            a.incomplete = true;
        }
    };

    // Newest first; only the newest `max_sessions` are read.
    candidates.sort_by_key(|c| std::cmp::Reverse(c.modified));
    if candidates.len() > limits.max_sessions {
        for c in candidates.drain(limits.max_sessions..) {
            mark(&mut agents, c.provider);
        }
    }
    let codex_titles = roots
        .codex_home
        .as_ref()
        .filter(|_| {
            candidates
                .iter()
                .any(|c| c.provider == crate::codex::ADAPTER_ID)
        })
        .map(|h| codex::thread_names(&h.join("session_index.jsonl"), limits.tail_bytes))
        .unwrap_or_default();

    let mut sessions = Vec::new();
    let mut pending = candidates.into_iter();
    for c in pending.by_ref() {
        if !budget.time_left() {
            mark(&mut agents, c.provider);
            break;
        }
        // A file time outside the calendar is skipped, never a panic (security review M1).
        let Some(modified) = fsutil::file_time(c.modified) else {
            continue;
        };
        let Some(win) = fsutil::read_window(&c.path, limits.head_bytes, limits.tail_bytes) else {
            continue;
        };
        let parsed = match c.provider {
            p if p == crate::claude_code::ADAPTER_ID => claude::parse(&win, &c.session_id),
            p if p == crate::cursor::ADAPTER_ID => Some(cursor::parse(&win)),
            _ => codex::parse(&win, &c.session_id),
        };
        // A log whose own records name another session (or can't be checked) is not listed.
        let Some(parsed) = parsed else {
            continue;
        };
        let last_activity = modified.min(now);
        let age = Duration::try_from(now - last_activity).unwrap_or_default();
        // Cursor's transcript has no tool results, so a trailing tool call says nothing.
        let pending_counts = c.provider != crate::cursor::ADAPTER_ID;
        let agent_title = if c.provider == crate::codex::ADAPTER_ID {
            codex_titles.get(&c.session_id).cloned().or(parsed.title)
        } else {
            parsed.title
        };
        let project_path = if c.provider == crate::cursor::ADAPTER_ID {
            cursor::project_label(&c.path)
        } else {
            parsed.cwd
        };
        sessions.push(Found {
            provider: c.provider,
            session_id: c.session_id,
            project_path,
            agent_title,
            first_prompt: parsed.first_prompt,
            last_activity,
            guess: guess(
                parsed.turn.ended,
                !parsed.turn.pending.is_empty(),
                age,
                pending_counts,
            ),
        });
    }
    for c in pending {
        mark(&mut agents, c.provider);
    }

    let mut oc_incomplete = false;
    let oc_state = match roots
        .opencode_db
        .as_deref()
        .filter(|p| fsutil::plain_file(p).is_some())
    {
        None => LogsState::NotFound,
        Some(db) if !fsutil::is_local(db) => LogsState::NotLocal,
        Some(db) if budget.time_left() => match opencode::read(
            db,
            limits.max_sessions,
            now,
            budget.until(),
            budget.cancel_flag(),
        ) {
            Ok(found) => {
                sessions.extend(found);
                LogsState::Read
            }
            Err(()) => {
                oc_incomplete = !budget.time_left();
                LogsState::Unreadable
            }
        },
        Some(_) => {
            oc_incomplete = true;
            LogsState::Unreadable
        }
    };
    agents.push(AgentLogs {
        provider: crate::opencode::ADAPTER_ID,
        state: oc_state,
        sessions: 0,
        incomplete: oc_incomplete,
    });

    sessions.sort_by(|a, b| {
        b.last_activity
            .cmp(&a.last_activity)
            .then_with(|| a.provider.cmp(b.provider))
            .then_with(|| a.session_id.cmp(&b.session_id))
    });
    // The same session can't be listed twice (a copied log, say): the newest wins.
    let mut seen = HashSet::new();
    sessions.retain(|s| seen.insert((s.provider, s.session_id.clone())));
    if sessions.len() > limits.max_sessions {
        for s in sessions.drain(limits.max_sessions..) {
            mark(&mut agents, s.provider);
        }
    }
    for a in &mut agents {
        a.sessions = sessions.iter().filter(|s| s.provider == a.provider).count();
    }
    let incomplete = budget.incomplete || agents.iter().any(|a| a.incomplete);
    Report {
        sessions,
        agents,
        incomplete,
        cancelled: budget.cancelled(),
    }
}

#[cfg(test)]
mod tests;
