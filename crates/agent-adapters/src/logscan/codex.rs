//! Codex: `$CODEX_HOME/sessions/YYYY/MM/DD/rollout-<local time>-<uuid>.jsonl` (checked
//! 2026-11-07 against 18 real rollouts written by Codex CLI, the VS Code extension and Codex
//! Desktop on Windows). Lines are `{timestamp, type, payload}`. The first is `session_meta`, whose
//! `payload.id` (equal to `payload.session_id` in every file seen) is the UUID at the end of the
//! file name. The hooks send that thread id as `session_id` (openai/codex source); no Codex
//! session had been tracked from a hook on this machine to compare with, so ADR-024 records the
//! id match as source-based.
//!
//! Used: `session_meta.payload.cwd`; the first user `response_item` message text that is not
//! markup (Codex also injects `<environment_context>` and similar blocks as user messages);
//! `event_msg` `task_started` / `task_complete` / `turn_aborted` for the turn; and
//! `response_item` `function_call` / `custom_tool_call` / `local_shell_call` against their
//! `*_output` by `call_id` for pending calls. Thread names (titles) come from
//! `$CODEX_HOME/session_index.jsonl` lines `{id, thread_name, updated_at}`, last one wins.

use std::collections::HashMap;
use std::path::Path;

use serde_json::Value;

use super::fsutil::{self, Budget, Window};
use super::{Candidate, Parsed, Turn, clean_path, clean_title, is_session_id, prompt_title};

fn is_uuid(s: &str) -> bool {
    s.len() == 36
        && s.bytes().enumerate().all(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

/// The UUID at the end of `rollout-…-<uuid>.jsonl`.
fn id_from_name(name: &str) -> Option<&str> {
    let stem = name.strip_prefix("rollout-")?.strip_suffix(".jsonl")?;
    let id = stem.get(stem.len().checked_sub(36)?..)?;
    (is_uuid(id) && is_session_id(id)).then_some(id)
}

fn numeric_dirs(
    dir: &Path,
    digits: usize,
    budget: &mut Budget,
) -> Result<Vec<std::path::PathBuf>, ()> {
    let mut out: Vec<_> = fsutil::list(dir, budget)?
        .into_iter()
        .filter(|(p, m)| {
            m.is_dir()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.len() == digits && n.bytes().all(|b| b.is_ascii_digit()))
        })
        .map(|(p, _)| p)
        .collect();
    // Newest first, so a budget that runs out drops the oldest days.
    out.sort();
    out.reverse();
    Ok(out)
}

pub(super) fn find(root: &Path, budget: &mut Budget) -> Result<Vec<Candidate>, ()> {
    let mut out = Vec::new();
    for year in numeric_dirs(root, 4, budget)? {
        for month in numeric_dirs(&year, 2, budget).unwrap_or_default() {
            for day in numeric_dirs(&month, 2, budget).unwrap_or_default() {
                let Ok(files) = fsutil::list(&day, budget) else {
                    continue;
                };
                for (path, meta) in files {
                    if !meta.is_file() {
                        continue;
                    }
                    let Some(id) = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .and_then(id_from_name)
                    else {
                        continue;
                    };
                    let Ok(modified) = meta.modified() else {
                        continue;
                    };
                    out.push(Candidate {
                        provider: crate::codex::ADAPTER_ID,
                        session_id: id.to_owned(),
                        path,
                        modified,
                    });
                }
            }
        }
    }
    Ok(out)
}

/// Thread names by id from the end of `session_index.jsonl` (at most `tail_bytes`).
pub(super) fn thread_names(path: &Path, tail_bytes: u64) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Some(win) = fsutil::read_window(path, 0, tail_bytes) else {
        return out;
    };
    for line in fsutil::lines(&win.tail, win.tail_cut, false) {
        let Ok(r) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        let (Some(id), Some(name)) = (
            r.get("id").and_then(Value::as_str),
            r.get("thread_name").and_then(Value::as_str),
        ) else {
            continue;
        };
        if is_session_id(id)
            && let Some(t) = clean_title(name)
        {
            if out.len() >= 10_000 && !out.contains_key(id) {
                continue;
            }
            out.insert(id.to_owned(), t);
        }
    }
    out
}

fn user_text(payload: &Value) -> Option<String> {
    if payload.get("type").and_then(Value::as_str) != Some("message")
        || payload.get("role").and_then(Value::as_str) != Some("user")
    {
        return None;
    }
    payload
        .get("content")
        .and_then(Value::as_array)?
        .iter()
        .find_map(|p| {
            (p.get("type").and_then(Value::as_str) == Some("input_text"))
                .then(|| p.get("text").and_then(Value::as_str))
                .flatten()
                .and_then(prompt_title)
        })
}

fn turn_step(turn: &mut Turn, r: &Value) {
    let Some(payload) = r.get("payload") else {
        return;
    };
    let ptype = payload.get("type").and_then(Value::as_str).unwrap_or("");
    let call_id = || payload.get("call_id").and_then(Value::as_str);
    match (r.get("type").and_then(Value::as_str), ptype) {
        (Some("event_msg"), "task_started") => turn.activity(),
        (Some("event_msg"), "task_complete" | "turn_aborted") => turn.end(),
        (Some("response_item"), "function_call" | "custom_tool_call" | "local_shell_call") => {
            match call_id() {
                Some(id) => turn.call(id),
                None => turn.activity(),
            }
        }
        (
            Some("response_item"),
            "function_call_output" | "custom_tool_call_output" | "local_shell_call_output",
        ) => match call_id() {
            Some(id) => turn.result(id),
            None => turn.activity(),
        },
        (Some("response_item"), "message" | "reasoning") => turn.activity(),
        _ => {}
    }
}

/// `None` unless the first record is a `session_meta` naming the same session as the file name
/// (a renamed or tampered file is not listed under an id it doesn't hold; a first record too
/// long for the head window can't be checked, so it isn't listed either: security review L3).
pub(super) fn parse(win: &Window, id: &str) -> Option<Parsed> {
    let mut parsed = Parsed::default();
    let mut verified = false;
    for (i, line) in fsutil::lines(&win.head, false, !win.whole).enumerate() {
        let Ok(r) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        let Some(payload) = r.get("payload") else {
            continue;
        };
        if i == 0 && r.get("type").and_then(Value::as_str) == Some("session_meta") {
            if payload.get("id").and_then(Value::as_str) != Some(id) {
                return None;
            }
            verified = true;
            parsed.cwd = payload
                .get("cwd")
                .and_then(Value::as_str)
                .and_then(clean_path);
            continue;
        }
        if parsed.first_prompt.is_none()
            && r.get("type").and_then(Value::as_str) == Some("response_item")
        {
            parsed.first_prompt = user_text(payload);
        }
        if parsed.first_prompt.is_some() && parsed.cwd.is_some() {
            break;
        }
    }
    if !verified {
        return None;
    }
    let mut turn = Turn::default();
    for line in fsutil::lines(&win.tail, win.tail_cut, false) {
        let Ok(r) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        turn_step(&mut turn, &r);
    }
    parsed.turn = turn;
    Some(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_come_only_from_well_formed_rollout_names() {
        assert_eq!(
            id_from_name("rollout-2026-04-23T12-04-52-019db995-c01a-7b2e-9f3c-1234567890ab.jsonl"),
            Some("019db995-c01a-7b2e-9f3c-1234567890ab")
        );
        for bad in [
            "rollout-x.jsonl",
            "other-2026-04-23T12-04-52-019db995-c01a-7b2e-9f3c-1234567890ab.jsonl",
            "rollout-2026-04-23T12-04-52-019db995-c01a-7b2e-9f3c-1234567890ab.json",
            "rollout-2026-04-23T12-04-52-019db995_c01a-7b2e-9f3c-1234567890ab.jsonl",
        ] {
            assert_eq!(id_from_name(bad), None, "{bad}");
        }
    }
}
