//! Claude Code: `<config>/projects/<encoded cwd>/<sessionId>.jsonl` (checked 2026-11-07 against
//! 75 real transcripts written by Claude Code CLI and the desktop app's Code tab on Windows).
//!
//! The file name is the session id the hooks send as `session_id` (confirmed: ids Lullshift
//! tracked from hooks match transcript names). Records are JSON lines with `type`. Used here:
//! `cwd` (any record), `custom-title.customTitle` and `agent-name.agentName` (the user's or app's
//! saved name; last wins), the first real `user` prompt, and for the turn: `assistant`
//! `message.content[].type == tool_use` (ids), `message.stop_reason == end_turn`, `user` tool
//! results (`tool_use_id`), and `system` records with subtype `stop_hook_summary` or
//! `turn_duration` (written when a turn ends). Sub-agent records (`isSidechain`) are ignored;
//! sub-agent files live in a sub-folder that is not scanned.

use std::path::Path;

use serde_json::Value;

use super::fsutil::{self, Budget, Window};
use super::{Candidate, Parsed, Turn, clean_path, clean_title, is_session_id, prompt_title};

pub(super) fn find(root: &Path, budget: &mut Budget) -> Result<Vec<Candidate>, ()> {
    let mut out = Vec::new();
    for (dir, meta) in fsutil::list(root, budget)? {
        if !meta.is_dir() {
            continue;
        }
        let Ok(files) = fsutil::list(&dir, budget) else {
            continue;
        };
        for (path, meta) in files {
            if !meta.is_file() {
                continue;
            }
            let Some(id) = path
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| n.strip_suffix(".jsonl"))
                .filter(|id| is_session_id(id))
            else {
                continue;
            };
            let Ok(modified) = meta.modified() else {
                continue;
            };
            out.push(Candidate {
                provider: crate::claude_code::ADAPTER_ID,
                session_id: id.to_owned(),
                path,
                modified,
            });
        }
    }
    Ok(out)
}

fn kind(r: &Value) -> &str {
    r.get("type").and_then(Value::as_str).unwrap_or("")
}

fn sidechain(r: &Value) -> bool {
    r.get("isSidechain").and_then(Value::as_bool) == Some(true)
}

/// The prompt text of a real user message: not meta, not a tool result, not markup.
fn user_prompt(r: &Value) -> Option<String> {
    if kind(r) != "user" || sidechain(r) || r.get("isMeta").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    if r.get("toolUseResult").is_some() {
        return None;
    }
    match r.pointer("/message/content")? {
        Value::String(s) => prompt_title(s),
        Value::Array(parts) => parts.iter().find_map(|p| {
            (p.get("type").and_then(Value::as_str) == Some("text"))
                .then(|| p.get("text").and_then(Value::as_str))
                .flatten()
                .and_then(prompt_title)
        }),
        _ => None,
    }
}

/// Titles, last one wins: `custom-title` before `agent-name`.
#[derive(Default)]
struct Titles {
    custom: Option<String>,
    agent: Option<String>,
}

impl Titles {
    fn take(&mut self, r: &Value) {
        match kind(r) {
            "custom-title" => {
                if let Some(t) = r
                    .get("customTitle")
                    .and_then(Value::as_str)
                    .and_then(clean_title)
                {
                    self.custom = Some(t);
                }
            }
            "agent-name" => {
                if let Some(t) = r
                    .get("agentName")
                    .and_then(Value::as_str)
                    .and_then(clean_title)
                {
                    self.agent = Some(t);
                }
            }
            _ => {}
        }
    }
}

fn turn_step(turn: &mut Turn, r: &Value) {
    if sidechain(r) {
        return;
    }
    match kind(r) {
        "assistant" => {
            let mut called = false;
            if let Some(parts) = r.pointer("/message/content").and_then(Value::as_array) {
                for p in parts {
                    if p.get("type").and_then(Value::as_str) == Some("tool_use")
                        && let Some(id) = p.get("id").and_then(Value::as_str)
                    {
                        turn.call(id);
                        called = true;
                    }
                }
            }
            let end_turn =
                r.pointer("/message/stop_reason").and_then(Value::as_str) == Some("end_turn");
            if end_turn && !called {
                turn.end();
            } else if !called {
                turn.activity();
            }
        }
        "user" => {
            let mut answered = false;
            if let Some(parts) = r.pointer("/message/content").and_then(Value::as_array) {
                for p in parts {
                    if p.get("type").and_then(Value::as_str) == Some("tool_result")
                        && let Some(id) = p.get("tool_use_id").and_then(Value::as_str)
                    {
                        turn.result(id);
                        answered = true;
                    }
                }
            }
            if !answered && r.get("isMeta").and_then(Value::as_bool) != Some(true) {
                // A new prompt: earlier calls are settled one way or another.
                turn.pending.clear();
                turn.activity();
            }
        }
        "system" => {
            if matches!(
                r.get("subtype").and_then(Value::as_str),
                Some("stop_hook_summary" | "turn_duration")
            ) {
                turn.end();
            }
        }
        _ => {}
    }
}

pub(super) fn parse(win: &Window) -> Parsed {
    let mut parsed = Parsed::default();
    let mut titles = Titles::default();
    for line in fsutil::lines(&win.head, false, !win.whole) {
        let Ok(r) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        if parsed.cwd.is_none()
            && let Some(cwd) = r.get("cwd").and_then(Value::as_str)
        {
            parsed.cwd = clean_path(cwd);
        }
        if parsed.first_prompt.is_none() {
            parsed.first_prompt = user_prompt(&r);
        }
        titles.take(&r);
    }
    let mut turn = Turn::default();
    for line in fsutil::lines(&win.tail, win.tail_cut, false) {
        let Ok(r) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        if parsed.cwd.is_none()
            && let Some(cwd) = r.get("cwd").and_then(Value::as_str)
        {
            parsed.cwd = clean_path(cwd);
        }
        if !win.whole {
            titles.take(&r);
        }
        turn_step(&mut turn, &r);
    }
    parsed.turn = turn;
    parsed.title = titles.custom.or(titles.agent);
    parsed
}
