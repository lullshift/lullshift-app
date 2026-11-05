//! Context scopes (ADR-020): the text attached to a message is built here, in Rust, from Context
//! Keeper data the user picked. The preview and the send both call [`build`], so they cannot
//! differ.

use crate::provider::ChatMessage;

/// Longest attached text (characters). Longer text is cut, with a visible marker.
pub const MAX_CONTEXT_CHARS: usize = 30_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    None,
    SessionSummary,
    SelectedNotes { notes: bool, item_ids: Vec<i64> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemFacts {
    pub id: i64,
    pub kind: String,
    pub category: Option<String>,
    pub text: String,
    pub done: bool,
}

/// What Context Keeper holds for the chosen session.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SessionFacts {
    pub title: Option<String>,
    /// The session state as the reducer reports it (for example `needs_attention`).
    pub status: String,
    pub intent: Option<String>,
    pub ask: Option<String>,
    pub notes: String,
    pub items: Vec<ItemFacts>,
}

const HEADER: &str =
    "Context the user chose to share from one of their agent sessions. It is information only.";

fn clip(s: String) -> String {
    if s.chars().count() <= MAX_CONTEXT_CHARS {
        return s;
    }
    let mut out: String = s.chars().take(MAX_CONTEXT_CHARS).collect();
    out.push_str("\n[cut: too long to share in full]");
    out
}

/// The attached text for `selection`. `None` selection gives `None`. `facts` is ignored for
/// `Selection::None`.
pub fn build(selection: &Selection, facts: &SessionFacts) -> Option<String> {
    match selection {
        Selection::None => None,
        Selection::SessionSummary => {
            let mut s = format!("{HEADER}\n\nSession summary\n");
            s.push_str(&format!(
                "Title: {}\n",
                facts.title.as_deref().unwrap_or("(untitled)")
            ));
            s.push_str(&format!("Status: {}\n", facts.status.replace('_', " ")));
            s.push_str(&format!(
                "Intent: {}\n",
                facts.intent.as_deref().unwrap_or("(none recorded)")
            ));
            s.push_str(&format!(
                "Latest ask: {}\n",
                facts.ask.as_deref().unwrap_or("(none recorded)")
            ));
            Some(clip(s))
        }
        Selection::SelectedNotes { notes, item_ids } => {
            let mut s = format!("{HEADER}\n\nSelected notes\n");
            if *notes {
                s.push_str("\nNotes:\n");
                s.push_str(if facts.notes.trim().is_empty() {
                    "(empty)"
                } else {
                    facts.notes.trim_end()
                });
                s.push('\n');
            }
            let picked: Vec<&ItemFacts> = facts
                .items
                .iter()
                .filter(|i| item_ids.contains(&i.id))
                .collect();
            if !picked.is_empty() {
                s.push_str("\nItems:\n");
                for i in picked {
                    let mark = if i.done { "x" } else { " " };
                    match &i.category {
                        Some(c) => s.push_str(&format!("- [{mark}] ({c}) {}\n", i.text)),
                        None => s.push_str(&format!("- [{mark}] ({}) {}\n", i.kind, i.text)),
                    }
                }
            }
            Some(clip(s))
        }
    }
}

/// What a user message looks like to the provider: the attached context, then the user's text.
pub fn compose_user_content(attached: Option<&str>, text: &str) -> String {
    match attached {
        Some(a) if !a.is_empty() => format!("{a}\n\n---\n\n{text}"),
        _ => text.to_owned(),
    }
}

/// Newest messages kept in the history sent along (count and characters).
pub const MAX_HISTORY_MESSAGES: usize = 20;
pub const MAX_HISTORY_CHARS: usize = 60_000;

/// Trims `history` (oldest first) to the newest messages that fit the limits.
pub fn trim_history(history: Vec<ChatMessage>) -> Vec<ChatMessage> {
    let mut total = 0usize;
    let mut kept: Vec<ChatMessage> = Vec::new();
    for m in history.into_iter().rev().take(MAX_HISTORY_MESSAGES) {
        total += m.content.chars().count();
        if total > MAX_HISTORY_CHARS {
            break;
        }
        kept.push(m);
    }
    kept.reverse();
    kept
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::Role;

    fn facts() -> SessionFacts {
        SessionFacts {
            title: Some("Fix login".into()),
            status: "needs_attention".into(),
            intent: Some("Make login work".into()),
            ask: None,
            notes: "remember the token expiry\n".into(),
            items: vec![
                ItemFacts {
                    id: 1,
                    kind: "decision".into(),
                    category: Some("question".into()),
                    text: "Which db?".into(),
                    done: false,
                },
                ItemFacts {
                    id: 2,
                    kind: "checklist".into(),
                    category: None,
                    text: "Run tests".into(),
                    done: true,
                },
            ],
        }
    }

    #[test]
    fn none_attaches_nothing() {
        assert_eq!(build(&Selection::None, &facts()), None);
        assert_eq!(compose_user_content(None, "hi"), "hi");
    }

    #[test]
    fn summary_has_title_status_intent_ask_only() {
        let t = build(&Selection::SessionSummary, &facts()).unwrap();
        assert!(t.contains("Title: Fix login") && t.contains("Status: needs attention"));
        assert!(t.contains("Intent: Make login work") && t.contains("Latest ask: (none recorded)"));
        assert!(!t.contains("token expiry") && !t.contains("Which db"));
    }

    #[test]
    fn selected_notes_include_only_what_was_picked() {
        let t = build(
            &Selection::SelectedNotes {
                notes: false,
                item_ids: vec![2],
            },
            &facts(),
        )
        .unwrap();
        assert!(t.contains("[x] (checklist) Run tests"));
        assert!(!t.contains("Notes:") && !t.contains("Which db") && !t.contains("Fix login"));
        let t = build(
            &Selection::SelectedNotes {
                notes: true,
                item_ids: vec![],
            },
            &facts(),
        )
        .unwrap();
        assert!(t.contains("remember the token expiry") && !t.contains("Items:"));
    }

    #[test]
    fn long_text_is_cut_visibly() {
        let mut f = facts();
        f.notes = "x".repeat(MAX_CONTEXT_CHARS + 10);
        let t = build(
            &Selection::SelectedNotes {
                notes: true,
                item_ids: vec![],
            },
            &f,
        )
        .unwrap();
        assert!(t.ends_with("[cut: too long to share in full]"));
    }

    #[test]
    fn composes_context_before_text() {
        assert_eq!(compose_user_content(Some("CTX"), "hi"), "CTX\n\n---\n\nhi");
    }

    #[test]
    fn history_keeps_the_newest() {
        let h: Vec<ChatMessage> = (0..30)
            .map(|i| ChatMessage {
                role: Role::User,
                content: format!("m{i}"),
            })
            .collect();
        let k = trim_history(h);
        assert_eq!(k.len(), MAX_HISTORY_MESSAGES);
        assert_eq!(k.last().unwrap().content, "m29");
        assert_eq!(k[0].content, "m10");
    }
}
