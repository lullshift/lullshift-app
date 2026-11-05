//! Cleaning of captured task text (ADR-013). Used by the hook forwarder before anything leaves
//! the hook process, and again by the app before anything is stored.
//!
//! [`clean`] removes control, bidi and invisible formatting characters, replaces secret-like
//! strings with `[hidden]` and cuts the result to a length in characters. Redaction is a safety
//! net, not a guarantee (the consent panel says so). `clean` is idempotent.

/// What replaces a secret-like string.
pub const HIDDEN: &str = "[hidden]";

/// Text longer than this is cut before any other work, so a huge prompt costs little time.
const MAX_INPUT_CHARS: usize = 20_000;

/// Unbroken runs of this many key-like characters (letters, digits and `+ / = - _`: keys,
/// hashes, base64 blobs) are hidden. A standard UUID (36 characters) stays readable.
const LONG_RUN: usize = 40;

/// Key prefixes of common credentials, matched at the start of a word.
const KEY_PREFIXES: [&str; 13] = [
    "sk_live_",
    "sk_test_",
    "rk_live_",
    "npm_",
    "glpat-",
    "sk-",
    "ghp_",
    "gho_",
    "github_pat_",
    "xoxb-",
    "xoxp-",
    "AKIA",
    "AIza",
];

/// Names whose value (after `=` or `:`) is hidden. Matched case-insensitively.
const SECRET_NAMES: [&str; 8] = [
    "password", "passwd", "pwd", "secret", "token", "api_key", "apikey", "api-key",
];

/// Strips unsafe characters, hides secrets and keeps at most `max_chars` characters.
pub fn clean(text: &str, max_chars: usize) -> String {
    let head: String = text.chars().take(MAX_INPUT_CHARS).collect();
    let s = hide_secrets(&strip_unsafe(&head));
    let s: String = s.trim().chars().take(max_chars).collect();
    s.trim_end().to_owned()
}

/// Removes control characters (newline and tab stay), bidi controls and invisible formatting
/// characters, which can disguise text or break its layout.
pub fn strip_unsafe(text: &str) -> String {
    text.chars()
        .filter(|&c| {
            (!c.is_control() || c == '\n' || c == '\t') && !agent_core::text::is_invisible(c)
        })
        .collect()
}

/// Replaces secret-like strings with [`HIDDEN`]: private key blocks, common key prefixes,
/// `password=` style assignments, bearer tokens and long unbroken letter/digit runs.
pub fn hide_secrets(text: &str) -> String {
    let s = hide_private_keys(text);
    let mut chars: Vec<char> = s.chars().collect();
    chars = hide_url_userinfo(&chars);
    chars = hide_assignments(&chars);
    chars = hide_prefixed_keys(&chars);
    chars = hide_long_runs(&chars);
    chars.into_iter().collect()
}

fn hide_private_keys(s: &str) -> String {
    const BEGIN: &str = "-----BEGIN ";
    const END: &str = "-----END ";
    const DASHES: &str = "-----";
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find(BEGIN) {
        let after = &rest[i + BEGIN.len()..];
        let Some(header_len) = after.find(DASHES) else {
            break;
        };
        if after[..header_len].contains("PRIVATE KEY") {
            out.push_str(&rest[..i]);
            out.push_str(HIDDEN);
            let body = &after[header_len + DASHES.len()..];
            // Up to the matching END line, or to the end of the text if it is cut off.
            rest = body
                .find(END)
                .and_then(|e| {
                    let tail = &body[e + END.len()..];
                    tail.find(DASHES).map(|d| &tail[d + DASHES.len()..])
                })
                .unwrap_or("");
        } else {
            out.push_str(&rest[..i + BEGIN.len()]);
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

fn starts_with_ignore_case(chars: &[char], at: usize, word: &str) -> bool {
    let mut it = chars[at..].iter();
    word.chars()
        .all(|w| it.next().is_some_and(|c| c.eq_ignore_ascii_case(&w)))
}

fn is_value_end(c: char) -> bool {
    c.is_whitespace() || matches!(c, '"' | '\'' | ',' | ';' | '&')
}

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

/// `scheme://user:pass@host`: hides `user:pass`. Linear: each `://` scans only its own
/// authority, which ends at the first `/`, `?`, `#`, quote or whitespace.
fn hide_url_userinfo(chars: &[char]) -> Vec<char> {
    let hidden: Vec<char> = HIDDEN.chars().collect();
    let mut out = Vec::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == ':' && chars.get(i + 1) == Some(&'/') && chars.get(i + 2) == Some(&'/') {
            let start = i + 3;
            let mut k = start;
            let mut at = None;
            while k < chars.len()
                && !chars[k].is_whitespace()
                && !matches!(chars[k], '/' | '?' | '#' | '"' | '\u{27}')
            {
                if chars[k] == '@' {
                    at = Some(k);
                }
                k += 1;
            }
            if let Some(at) = at
                && at > start
                && chars[start..at].contains(&':')
            {
                out.extend_from_slice(&chars[i..start]);
                out.extend_from_slice(&hidden);
                i = at;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn hide_assignments(chars: &[char]) -> Vec<char> {
    let hidden: Vec<char> = HIDDEN.chars().collect();
    let mut out = Vec::with_capacity(chars.len());
    // End of the name-like run ([A-Za-z0-9_-]) that holds each position, so a key word may
    // carry any prefix or suffix (`aws_secret_access_key`, `DB_PASSWORD_PROD`) in O(1).
    let mut run_end = vec![0usize; chars.len() + 1];
    run_end[chars.len()] = chars.len();
    for j in (0..chars.len()).rev() {
        run_end[j] = if is_name_char(chars[j]) && j + 1 < chars.len() + 1 {
            if j + 1 < chars.len() && is_name_char(chars[j + 1]) {
                run_end[j + 1]
            } else {
                j + 1
            }
        } else {
            j
        };
    }
    let mut i = 0;
    while i < chars.len() {
        let name_len = SECRET_NAMES
            .iter()
            .find(|n| starts_with_ignore_case(chars, i, n))
            .map(|n| n.len());
        if let Some(len) = name_len {
            let mut k = i + len;
            if k < chars.len() && is_name_char(chars[k]) {
                k = run_end[k];
            }
            if k < chars.len() && matches!(chars[k], '"' | '\'') {
                k += 1;
            }
            while k < chars.len() && matches!(chars[k], ' ' | '\t') {
                k += 1;
            }
            if k < chars.len() && matches!(chars[k], '=' | ':') {
                k += 1;
                while k < chars.len() && matches!(chars[k], ' ' | '\t') {
                    k += 1;
                }
                if k < chars.len() && matches!(chars[k], '"' | '\'') {
                    k += 1;
                }
                let start = k;
                while k < chars.len() && !is_value_end(chars[k]) {
                    k += 1;
                }
                if k > start {
                    out.extend_from_slice(&chars[i..start]);
                    out.extend_from_slice(&hidden);
                    i = k;
                    continue;
                }
            }
        }
        if starts_with_ignore_case(chars, i, "bearer ") {
            let start = i + "bearer ".len();
            let mut k = start;
            while k < chars.len()
                && (chars[k].is_ascii_alphanumeric() || "._~+/=-".contains(chars[k]))
            {
                k += 1;
            }
            if k - start >= 8 {
                out.extend_from_slice(&chars[i..start]);
                out.extend_from_slice(&hidden);
                i = k;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn is_key_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.')
}

fn hide_prefixed_keys(chars: &[char]) -> Vec<char> {
    let hidden: Vec<char> = HIDDEN.chars().collect();
    let mut out = Vec::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        let at_word_start = i == 0 || !chars[i - 1].is_alphanumeric();
        let prefix = at_word_start
            .then(|| {
                KEY_PREFIXES.iter().find(|p| {
                    let mut it = chars[i..].iter();
                    p.chars().all(|w| it.next() == Some(&w))
                })
            })
            .flatten();
        if let Some(p) = prefix {
            let mut k = i + p.len();
            while k < chars.len() && is_key_char(chars[k]) {
                k += 1;
            }
            if k - (i + p.len()) >= 4 {
                out.extend_from_slice(&hidden);
                i = k;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn is_run_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=' | '-' | '_')
}

fn hide_long_runs(chars: &[char]) -> Vec<char> {
    let hidden: Vec<char> = HIDDEN.chars().collect();
    let mut out = Vec::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        if is_run_char(chars[i]) {
            let mut k = i;
            while k < chars.len() && is_run_char(chars[k]) {
                k += 1;
            }
            let run = &chars[i..k];
            // Long hyphenated or slashed words and paths are not keys: when a run has
            // separators, it must also look random (a digit, or mixed case).
            let separated = run.iter().any(|c| matches!(c, '-' | '_' | '/'));
            let random = run.iter().any(char::is_ascii_digit)
                || (run.iter().any(char::is_ascii_uppercase)
                    && run.iter().any(char::is_ascii_lowercase));
            if run.len() >= LONG_RUN && (!separated || random) {
                out.extend_from_slice(&hidden);
            } else {
                out.extend_from_slice(run);
            }
            i = k;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hidden(input: &str) -> String {
        clean(input, 2000)
    }

    #[test]
    fn plain_text_is_kept_with_newlines() {
        assert_eq!(
            hidden("Fix the login bug.\nThen add tests."),
            "Fix the login bug.\nThen add tests."
        );
    }

    #[test]
    fn control_bidi_and_invisible_characters_are_removed() {
        let input = "a\u{0}b\u{7}c\u{202e}d\u{200b}e\u{feff}f\u{2066}g\r\u{85}h\u{e0041}i";
        assert_eq!(hidden(input), "abcdefghi");
    }

    #[test]
    fn truncates_by_characters_not_bytes() {
        let s = "é".repeat(50);
        assert_eq!(clean(&s, 10).chars().count(), 10);
        assert_eq!(clean("short", 300), "short");
        assert_eq!(clean("  padded  ", 300), "padded");
    }

    #[test]
    fn private_key_blocks_are_hidden() {
        let key = "-----BEGIN RSA PRIVATE KEY-----\nMIIEow\nabc\n-----END RSA PRIVATE KEY-----";
        assert_eq!(
            hidden(&format!("use this {key} please")),
            "use this [hidden] please"
        );
        // Cut off before the END line: everything after BEGIN goes.
        assert_eq!(
            hidden("x -----BEGIN PRIVATE KEY-----\nMIIE and more"),
            "x [hidden]"
        );
        // A certificate is not a private key.
        assert!(
            hidden("-----BEGIN CERTIFICATE-----\nabc\n-----END CERTIFICATE-----")
                .contains("CERTIFICATE")
        );
    }

    #[test]
    fn common_key_prefixes_are_hidden() {
        for secret in [
            "sk-ant-api03-AbCdEf123456",
            "ghp_1234567890abcdef",
            "gho_abcDEF123456",
            "github_pat_11ABCDEFG0abcdef",
            // Split so the repository secret scan does not flag this fake fixture.
            concat!("xo", "xb-1234-5678-abcdef"),
            // Split so the repository secret scan does not flag this fake fixture.
            concat!("xo", "xp-1234-5678-abcdef"),
            "AKIAIOSFODNN7EXAMPLE",
            "AIzaSyA-abcdefghijk",
        ] {
            let out = hidden(&format!("my key is {secret} ok"));
            assert_eq!(out, "my key is [hidden] ok", "{secret}");
            let out = hidden(&format!("key={secret}"));
            assert!(!out.contains(secret), "{out}");
        }
    }

    #[test]
    fn prefixes_inside_words_and_short_matches_are_kept() {
        assert_eq!(
            hidden("the task-list and desk-lamp"),
            "the task-list and desk-lamp"
        );
        assert_eq!(hidden("use sk-x"), "use sk-x");
    }

    #[test]
    fn assignments_hide_the_value_and_keep_the_name() {
        assert_eq!(hidden("password=hunter2 done"), "password=[hidden] done");
        assert_eq!(hidden("PASSWORD: hunter2"), "PASSWORD: [hidden]");
        assert_eq!(
            hidden("export GITHUB_TOKEN=abc123"),
            "export GITHUB_TOKEN=[hidden]"
        );
        assert_eq!(
            hidden(r#"{"secret": "s3cr3t", "x": 1}"#),
            r#"{"secret": "[hidden]", "x": 1}"#
        );
        assert_eq!(hidden("api_key = 'k-1'"), "api_key = '[hidden]'");
        assert_eq!(
            hidden("Authorization: Bearer abcdefgh12345"),
            "Authorization: Bearer [hidden]"
        );
        // No assignment, no change.
        assert_eq!(
            hidden("rotate the token and the password policy"),
            "rotate the token and the password policy"
        );
    }

    #[test]
    fn long_unbroken_runs_are_hidden() {
        let run39 = "a1".repeat(19) + "z";
        let run40 = "a1".repeat(20);
        assert_eq!(hidden(&format!("x {run39} y")), format!("x {run39} y"));
        assert_eq!(hidden(&format!("x {run40} y")), "x [hidden] y");
        // Base64 and URL-safe alphabets count: + / = - _ stay inside the run.
        let b64 = "Ab3+/xYz9=".repeat(5);
        assert_eq!(hidden(&format!("k {b64} z")), "k [hidden] z");
        let url_safe = "Ab3-_xYz9_".repeat(5);
        assert_eq!(hidden(&format!("k {url_safe} z")), "k [hidden] z");
        // A standard UUID stays readable, alone or in text.
        let uuid = "6f1c2a52-8a63-4f53-9a0e-1d1f6b7e2c11";
        assert_eq!(hidden(uuid), uuid);
        assert_eq!(hidden(&format!("id {uuid}.")), format!("id {uuid}."));
        // Long words and paths without anything random-looking are text, not keys.
        let slug = "this-is-a-very-long-kebab-case-slug-for-a-branch-name";
        assert_eq!(hidden(slug), slug);
        let path = "src/components/settings/panels/advanced/options";
        assert_eq!(hidden(path), path);
    }

    #[test]
    fn secret_names_with_any_prefix_or_suffix_are_hidden() {
        for (input, want) in [
            (
                "aws_secret_access_key=abc123",
                "aws_secret_access_key=[hidden]",
            ),
            ("DB_PASSWORD_PROD=hunter2 x", "DB_PASSWORD_PROD=[hidden] x"),
            ("my-api-token-v2: abc", "my-api-token-v2: [hidden]"),
            ("GITHUB_TOKEN_2=abc", "GITHUB_TOKEN_2=[hidden]"),
            (
                "client_secret_key = 'abc'",
                "client_secret_key = '[hidden]'",
            ),
        ] {
            assert_eq!(hidden(input), want, "{input}");
        }
        // A key word with no assignment after it is plain text.
        assert_eq!(
            hidden("the secret_sauce is good"),
            "the secret_sauce is good"
        );
    }

    #[test]
    fn url_userinfo_is_hidden() {
        for (input, want) in [
            (
                concat!("post", "gres://admin:s3cr3t@db.example.com:5432/app"),
                "postgres://[hidden]@db.example.com:5432/app",
            ),
            (
                "see https://user:pw@host/path now",
                "see https://[hidden]@host/path now",
            ),
            (
                "redis://:onlypass@cache:6379",
                "redis://[hidden]@cache:6379",
            ),
        ] {
            assert_eq!(hidden(input), want, "{input}");
        }
        // No credentials: unchanged.
        for plain in [
            "https://example.com/a@b",
            "https://host:8080/path",
            "ssh://git@github.com/x/y.git",
            "mailto://a@b.c",
        ] {
            assert_eq!(hidden(plain), plain, "{plain}");
        }
    }

    #[test]
    fn newer_key_prefixes_are_hidden() {
        // Fake fixtures, split so the repository secret scan does not flag them.
        for secret in [
            concat!("sk_", "live_4eC39HqLyjWDarjtT1zdp7dc"),
            concat!("sk_", "test_4eC39HqLyjWDarjtT1zdp7dc"),
            concat!("rk_", "live_4eC39HqLyjWDarjtT1zdp7dc"),
            concat!("np", "m_abcdefghijklmnopqrstuvwxyz0123"),
            concat!("glp", "at-abcdefghij1234567890"),
        ] {
            assert_eq!(
                hidden(&format!("use {secret} ok")),
                "use [hidden] ok",
                "{secret}"
            );
        }
    }

    #[test]
    fn redaction_is_linear_on_adversarial_input() {
        let started = std::time::Instant::now();
        for input in [
            "token".repeat(4000),
            "secret_".repeat(2800),
            "://".repeat(6000),
            "a@".repeat(9000),
            format!("http://{}", "x:".repeat(9000)),
            "Bearer ".repeat(2800),
            "-----BEGIN ".repeat(1800),
            "A1/".repeat(6000),
        ] {
            let _ = clean(&input, 2000);
        }
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
    }

    #[test]
    fn cleaning_is_idempotent() {
        let input = "password=abc sk-ant-1234567 \u{202e}hi\n-----BEGIN PRIVATE KEY-----\nx\n-----END PRIVATE KEY----- token: zzz Bearer abcdefghij0123";
        let once = hidden(input);
        assert_eq!(hidden(&once), once);
        assert!(!once.contains("abcdefghij0123") && !once.contains("zzz"));
    }

    #[test]
    fn huge_input_is_bounded() {
        let big = "word ".repeat(1_000_000);
        assert!(clean(&big, 2000).chars().count() <= 2000);
    }
}
