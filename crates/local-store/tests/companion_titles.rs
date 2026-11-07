//! Thread titles taken from the first message. No schema change: only the stored title moves.

use local_store::{CompanionProfile, NewUserMessage, Store, title_from_message};
use time::OffsetDateTime;
use time::macros::datetime;

const T0: OffsetDateTime = datetime!(2026-11-05 10:00 UTC);

fn store_with_profile() -> Store {
    let mut s = Store::open_in_memory().expect("store");
    s.companion_save_profile(
        &CompanionProfile {
            id: "a".to_owned(),
            kind: "openaiCompatible".to_owned(),
            label: "Profile a".to_owned(),
            base_url: "http://localhost:11434/v1".to_owned(),
            model: "llama3".to_owned(),
            monthly_token_budget: None,
            is_default: true,
        },
        T0,
    )
    .expect("profile");
    s
}

fn msg(thread: i64, text: &str) -> NewUserMessage<'_> {
    NewUserMessage {
        thread_id: thread,
        text,
        attached_context: None,
        scope_kind: "none",
        profile_id: "a",
        model: "llama3",
        host: "localhost:11434",
    }
}

fn title(s: &Store, id: i64) -> String {
    s.companion_thread(id).expect("thread").expect("some").title
}

#[test]
fn the_first_message_names_a_thread_with_the_default_title() {
    let mut s = store_with_profile();
    let t = s.companion_new_thread(None, T0).expect("thread");
    s.companion_add_user_message(
        &msg(t.id, "  How do I\n\tfix   the flaky\u{200B} test?\u{7} "),
        T0,
    )
    .expect("first");
    assert_eq!(title(&s, t.id), "How do I fix the flaky test?");
    // A later message never renames it.
    s.companion_add_user_message(&msg(t.id, "second question"), T0)
        .expect("second");
    assert_eq!(title(&s, t.id), "How do I fix the flaky test?");
}

#[test]
fn a_renamed_or_named_thread_keeps_its_title() {
    let mut s = store_with_profile();
    let t = s.companion_new_thread(None, T0).expect("thread");
    s.companion_rename_thread(t.id, "My notes").expect("rename");
    s.companion_add_user_message(&msg(t.id, "hello there"), T0)
        .expect("first");
    assert_eq!(title(&s, t.id), "My notes");

    let named = s.companion_new_thread(Some("Planning"), T0).expect("named");
    s.companion_add_user_message(&msg(named.id, "something else"), T0)
        .expect("msg");
    assert_eq!(title(&s, named.id), "Planning");
}

#[test]
fn a_message_of_only_invisible_characters_gives_no_title() {
    assert_eq!(title_from_message("\u{200B}\u{7}\u{202E}"), None);
    assert_eq!(title_from_message(" \n\t "), None);
    let mut s = store_with_profile();
    let t = s.companion_new_thread(None, T0).expect("thread");
    s.companion_add_user_message(&msg(t.id, "\u{200B}\u{202E}\u{7}x"), T0)
        .expect("first");
    assert_eq!(title(&s, t.id), "x");
}

#[test]
fn titles_are_cut_on_a_word_boundary_with_an_ellipsis() {
    let t = title_from_message;
    assert_eq!(t("short one").as_deref(), Some("short one"));
    let exactly_48 = "a".repeat(48);
    assert_eq!(t(&exactly_48).as_deref(), Some(exactly_48.as_str()));
    let long = "Please explain why the retry loop in the token refresh code never stops";
    let cut = t(long).expect("title");
    assert_eq!(cut, "Please explain why the retry loop in the token…");
    assert!(cut.chars().count() <= 49);
    // A cut that lands on a space leaves no trailing space.
    let on_space = format!("{} tail words here", "b".repeat(47));
    assert_eq!(t(&on_space).expect("title"), format!("{}…", "b".repeat(47)));
    // One very long word is cut hard.
    assert_eq!(
        t(&"c".repeat(100)).expect("title"),
        format!("{}…", "c".repeat(48))
    );
    // Characters are counted, not bytes.
    assert_eq!(t(&"é".repeat(60)).expect("title").chars().count(), 49);
}
