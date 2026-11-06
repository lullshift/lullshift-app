//! The shared plain-text corpus (decision 18.7): the host's check must agree with the schemas and
//! the SDK, which run the same file in their own tests.

use serde_json::Value;

use crate::manifest::{hidden_char, long_mark_run, plain, text_ok};

const CORPUS: &str = include_str!("../../../../tests/fixtures/plain-text/cases.json");

fn cases() -> Vec<(String, String, bool, Option<String>)> {
    let doc: Value = serde_json::from_str(CORPUS).expect("corpus is JSON");
    doc["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .map(|c| {
            (
                c["name"].as_str().expect("name").to_owned(),
                c["text"].as_str().expect("text").to_owned(),
                c["plain"].as_bool().expect("plain"),
                c["reason"].as_str().map(str::to_owned),
            )
        })
        .collect()
}

#[test]
fn the_host_check_agrees_with_every_case_in_the_shared_corpus() {
    let all = cases();
    assert!(all.len() >= 50, "the corpus is not empty");
    for (name, text, expected, reason) in all {
        assert_eq!(plain(&text, 10_000), expected, "{name}");
        assert_eq!(text_ok(&text, 10_000), expected, "{name}");
        match reason.as_deref() {
            Some("hidden") => assert!(text.chars().any(hidden_char), "{name}: hidden"),
            Some("marks") => {
                assert!(long_mark_run(&text), "{name}: marks");
                assert!(!text.chars().any(hidden_char), "{name}: only marks");
            }
            None => assert!(expected, "{name}: a refused case names its reason"),
            Some(other) => panic!("{name}: unknown reason {other}"),
        }
    }
}

#[test]
fn every_hidden_range_of_the_host_is_covered_at_both_ends() {
    // Each range of `hidden_char`, so a range added to the host without a corpus case fails here.
    let ranges: [(u32, u32); 19] = [
        (0x0, 0x1F),
        (0x7F, 0x9F),
        (0xAD, 0xAD),
        (0x34F, 0x34F),
        (0x61C, 0x61C),
        (0x115F, 0x1160),
        (0x17B4, 0x17B5),
        (0x180B, 0x180E),
        (0x200B, 0x200F),
        (0x2028, 0x202E),
        (0x2060, 0x2064),
        (0x2066, 0x206F),
        (0x3164, 0x3164),
        (0xFEFF, 0xFEFF),
        (0xFFA0, 0xFFA0),
        (0xFFF9, 0xFFFB),
        (0x1BCA0, 0x1BCA3),
        (0x1D173, 0x1D17A),
        (0xE0000, 0xE007F),
    ];
    for (lo, hi) in ranges {
        for cp in [lo, hi] {
            let c = char::from_u32(cp).expect("char");
            assert!(hidden_char(c), "U+{cp:04X} is hidden");
        }
        for cp in [lo.wrapping_sub(1), hi + 1] {
            let Some(c) = char::from_u32(cp) else {
                continue;
            };
            // Neighbours are either in the next range or allowed.
            let inside = ranges.iter().any(|(a, b)| (*a..=*b).contains(&cp));
            assert_eq!(hidden_char(c), inside, "U+{cp:04X}");
        }
    }
    let samples: Vec<char> = cases()
        .iter()
        .filter(|c| c.3.as_deref() == Some("hidden"))
        .flat_map(|c| {
            c.1.chars()
                .filter(|ch| hidden_char(*ch))
                .collect::<Vec<_>>()
        })
        .collect();
    for (lo, hi) in ranges {
        assert!(
            samples.iter().any(|c| (lo..=hi).contains(&u32::from(*c))),
            "the corpus has a case in U+{lo:04X}..U+{hi:04X}"
        );
    }
}
