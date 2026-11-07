//! Cursor: `~/.cursor/projects/<encoded workspace>/agent-transcripts/<id>/<id>.jsonl` (checked
//! 2026-11-07 against 54 real transcripts on Windows; in all of them the folder name equals the
//! file name). The id is the conversation id; Cursor's hooks send `conversation_id`, which the
//! adapter uses as the session id. Cursor documents agent transcripts as keyed by conversation;
//! a live hook event was not compared on this machine (no Cursor session was tracked yet), so
//! the match is recorded as source/docs-based in ADR-024.
//!
//! Lines are `{role, message:{content:[{type, text?}]}}` plus `{type:"turn_ended", status}`.
//! There are no timestamps (the file's last write time is used), no working directory (the
//! encoded workspace folder name is shown instead) and no tool results (so a trailing tool call
//! is not read as "may need you"; Cursor has no needs-you signal, ADR-021). Sub-agent
//! transcripts in `<id>/subagents/` are not scanned.

use std::path::Path;

use serde_json::Value;

use super::fsutil::{self, Budget, Window};
use super::{Candidate, Parsed, Turn, clean_path, is_session_id, prompt_title};

pub(super) fn find(root: &Path, budget: &mut Budget) -> Result<Vec<Candidate>, ()> {
    let mut out = Vec::new();
    for (project, meta) in fsutil::list(root, budget)? {
        if !meta.is_dir() {
            continue;
        }
        let transcripts = project.join("agent-transcripts");
        if fsutil::plain_dir(&transcripts).is_none() {
            continue;
        }
        let Ok(convs) = fsutil::list(&transcripts, budget) else {
            continue;
        };
        for (dir, meta) in convs {
            if !meta.is_dir() {
                continue;
            }
            let Some(id) = dir
                .file_name()
                .and_then(|n| n.to_str())
                .filter(|id| is_session_id(id))
            else {
                continue;
            };
            if !budget.take_entry() {
                break;
            }
            let path = dir.join(format!("{id}.jsonl"));
            let Some(file) = fsutil::plain_file(&path) else {
                continue;
            };
            let Ok(modified) = file.modified() else {
                continue;
            };
            out.push(Candidate {
                provider: crate::cursor::ADAPTER_ID,
                session_id: id.to_owned(),
                path,
                modified,
            });
        }
    }
    Ok(out)
}

/// The workspace folder Cursor named the project after, from `<root>/<project>/agent-transcripts/..`.
pub(super) fn project_label(path: &Path) -> Option<String> {
    let project = path.parent()?.parent()?.parent()?.file_name()?.to_str()?;
    clean_path(project)
}

/// The user's words: the text inside `<user_query>…</user_query>` if present (Cursor wraps the
/// prompt in it, often after a `<timestamp>` block), else the text itself.
fn user_query(text: &str) -> Option<String> {
    if let Some(start) = text.find("<user_query>") {
        let rest = &text[start + "<user_query>".len()..];
        let inner = rest.find("</user_query>").map_or(rest, |end| &rest[..end]);
        return prompt_title(inner);
    }
    prompt_title(text)
}

pub(super) fn parse(win: &Window) -> Parsed {
    let mut parsed = Parsed::default();
    for line in fsutil::lines(&win.head, false, !win.whole) {
        let Ok(r) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        if r.get("role").and_then(Value::as_str) != Some("user") {
            continue;
        }
        let parts = r.pointer("/message/content").and_then(Value::as_array);
        parsed.first_prompt = parts.into_iter().flatten().find_map(|p| {
            (p.get("type").and_then(Value::as_str) == Some("text"))
                .then(|| p.get("text").and_then(Value::as_str))
                .flatten()
                .and_then(user_query)
        });
        // Only the first user message is a title candidate.
        break;
    }
    let mut turn = Turn::default();
    for line in fsutil::lines(&win.tail, win.tail_cut, false) {
        let Ok(r) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        if r.get("type").and_then(Value::as_str) == Some("turn_ended") {
            turn.end();
        } else if matches!(
            r.get("role").and_then(Value::as_str),
            Some("user" | "assistant")
        ) {
            turn.activity();
        }
    }
    parsed.turn = turn;
    parsed
}
