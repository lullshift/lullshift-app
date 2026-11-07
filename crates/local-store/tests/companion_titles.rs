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

#[test]
fn fillers_and_tag_characters_are_not_visible_text() {
    // U+3164 (Hangul filler), U+061C (Arabic letter mark) and tag characters draw nothing.
    assert_eq!(title_from_message("\u{3164}"), None);
    assert_eq!(title_from_message("\u{3164}\u{3164} \u{061C}"), None);
    assert_eq!(title_from_message("\u{E0041}\u{E0042}"), None);
    assert_eq!(
        title_from_message("a\u{E0041}\u{3164}b").as_deref(),
        Some("ab")
    );

    let mut s = store_with_profile();
    let t = s.companion_new_thread(None, T0).expect("thread");
    s.companion_add_user_message(&msg(t.id, "\u{3164}"), T0)
        .expect("first");
    assert_eq!(title(&s, t.id), local_store::DEFAULT_THREAD_TITLE);
}

#[test]
fn a_cut_never_splits_a_cluster() {
    let t = title_from_message;
    // 47 letters, then "e" + combining acute exactly at the limit, then more text.
    let accent = format!("{}e\u{0301}fghij klm", "a".repeat(47));
    let cut = t(&accent).expect("title");
    assert_eq!(cut, format!("{}…", "a".repeat(47)));
    assert!(!cut.contains('\u{0301}'));
    // The cluster fits whole when the limit allows it.
    let fits = format!("{}e\u{0301}", "a".repeat(46));
    assert_eq!(t(&fits).as_deref(), Some(fits.as_str()));
    // Skin tone modifier, variation selector and joiner stay with their base.
    let tone = format!("{}\u{1F44D}\u{1F3FD}xyz", "a".repeat(47));
    assert_eq!(t(&tone).expect("title"), format!("{}…", "a".repeat(47)));
    let vs = format!("{}\u{2764}\u{FE0F}xyz", "a".repeat(47));
    assert_eq!(t(&vs).expect("title"), format!("{}…", "a".repeat(47)));
    // The joiner is an invisible character and is removed before the cut, so the two emoji
    // are separate characters by then; the cut never ends on a joiner.
    let zwj = format!("{}\u{1F468}\u{200D}\u{1F469}xyz", "a".repeat(47));
    assert_eq!(
        t(&zwj).expect("title"),
        format!("{}\u{1F468}…", "a".repeat(47))
    );
    // A flag (two regional indicators) is kept whole or dropped whole.
    let flag = format!("{}\u{1F1E9}\u{1F1EA}xyz", "a".repeat(47));
    assert_eq!(t(&flag).expect("title"), format!("{}…", "a".repeat(47)));
    let flag_inside = format!("{}\u{1F1E9}\u{1F1EA}xyz", "a".repeat(46));
    assert_eq!(
        t(&flag_inside).expect("title"),
        format!("{}\u{1F1E9}\u{1F1EA}…", "a".repeat(46))
    );
    // A lone indicator at the end of the cut is dropped too.
    let lone = format!("{}\u{1F1E9}xyz1234", "a".repeat(47));
    assert_eq!(t(&lone).expect("title"), format!("{}…", "a".repeat(47)));
}

#[test]
fn renaming_to_the_default_title_still_lets_the_first_message_name_the_thread() {
    // Known edge, kept on purpose: a renamed "New conversation" looks like an untouched thread.
    let mut s = store_with_profile();
    let t = s.companion_new_thread(None, T0).expect("thread");
    s.companion_rename_thread(t.id, local_store::DEFAULT_THREAD_TITLE)
        .expect("rename");
    s.companion_add_user_message(&msg(t.id, "hello there"), T0)
        .expect("first");
    assert_eq!(title(&s, t.id), "hello there");
}
