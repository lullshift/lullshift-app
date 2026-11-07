//! Last line of defence: removes a key from any string before it is shown or stored.

const HIDDEN: &str = "[hidden]";
/// Any run of this many characters (or more) of the key is hidden, not only the whole key.
const MIN_FRAGMENT: usize = 12;

const STD: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn base64(bytes: &[u8], alphabet: &[u8; 64], pad: bool) -> String {
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        let chars = chunk.len() + 1;
        for i in 0..4 {
            if i < chars {
                out.push(char::from(alphabet[((n >> (18 - 6 * i)) & 63) as usize]));
            } else if pad {
                out.push('=');
            }
        }
    }
    out
}

/// The encoded shapes a key can take in text: base64 (standard and URL-safe, with and without
/// padding) of the key, of `Bearer <key>` (an echoed header), and of the key at the two other
/// byte alignments inside a longer base64 stream (only the groups that do not depend on their
/// neighbours), and the percent-encoded key.
fn encoded_forms(key: &str) -> Vec<String> {
    let mut forms = Vec::new();
    let bearer = format!("Bearer {key}");
    for src in [key, bearer.as_str()] {
        for alphabet in [STD, URL] {
            for pad in [true, false] {
                forms.push(base64(src.as_bytes(), alphabet, pad));
            }
        }
    }
    for lead in 1..3usize {
        let mut bytes = vec![b'x'; lead];
        bytes.extend_from_slice(key.as_bytes());
        for alphabet in [STD, URL] {
            let full = base64(&bytes, alphabet, false);
            // Drop the first group (it mixes in unknown lead bytes) and the last (it may depend
            // on what follows).
            if full.len() > 8 {
                forms.push(full[4..full.len() - 4].to_owned());
            }
        }
    }
    let pct: String = url::form_urlencoded::byte_serialize(key.as_bytes()).collect();
    forms.push(pct.replace('+', "%20"));
    forms.push(pct);
    forms.retain(|f| f.len() >= 8 && f != key);
    // Longest first, so a whole form is replaced before pieces of it.
    forms.sort_by(|a, b| b.len().cmp(&a.len()).then(a.cmp(b)));
    forms.dedup();
    forms
}

/// Replaces every occurrence of `key` in `text` with `[hidden]`: plain, JSON-escaped, base64
/// and percent-encoded, and any run of 12 or more of its characters. An empty or very short key
/// is ignored.
pub fn scrub(text: &str, key: &str) -> String {
    if key.len() < 4 {
        return text.to_owned();
    }
    let mut out = text.replace(key, HIDDEN);
    let json = serde_json::to_string(key).unwrap_or_default();
    if let Some(inner) = json.strip_prefix('"').and_then(|s| s.strip_suffix('"'))
        && inner != key
    {
        out = out.replace(inner, HIDDEN);
    }
    for form in encoded_forms(key) {
        out = out.replace(&form, HIDDEN);
    }
    let chars: Vec<char> = key.chars().collect();
    if chars.len() >= MIN_FRAGMENT {
        for w in chars.windows(MIN_FRAGMENT) {
            let frag: String = w.iter().collect();
            if out.contains(&frag) {
                out = out.replace(&frag, HIDDEN);
            }
        }
    }
    out
}

/// Replaces anything in `text` that looks like an API key, whether or not it is the key in use:
/// a word starting `sk-` / `sk_`, or a long unbroken run of letters, digits, `-` and `_` that
/// mixes letters and digits. Works word by word on text whose whitespace is already collapsed.
/// This is for provider text shown to the user, where a provider may echo a masked key.
pub fn scrub_key_like(text: &str) -> String {
    let word_char = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
    text.split(' ')
        .map(|word| {
            let core = word.trim_matches(|c: char| !word_char(c));
            if core.is_empty() {
                return word.to_owned();
            }
            let lower = core.to_ascii_lowercase();
            let prefixed =
                (lower.starts_with("sk-") || lower.starts_with("sk_")) && core.len() >= 8;
            let long = core.len() >= 32
                && core.chars().all(word_char)
                && core.chars().any(|c| c.is_ascii_digit())
                && core.chars().any(|c| c.is_ascii_alphabetic());
            if prefixed || long {
                word.replacen(core, HIDDEN, 1)
            } else {
                word.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_like_words_are_hidden_and_plain_words_are_not() {
        let t = scrub_key_like(
            "Incorrect API key provided: sk-proj-****abcd. See https://platform.openai.com/account/api-keys and abcdEFGH1234abcdEFGH1234abcdEFGH1234.",
        );
        assert!(!t.contains("sk-proj"), "{t}");
        assert!(!t.contains("EFGH1234"), "{t}");
        assert!(t.contains("Incorrect API key provided"));
        assert_eq!(
            scrub_key_like("The model does not exist"),
            "The model does not exist"
        );
    }

    #[test]
    fn removes_every_occurrence() {
        let k = "sk-live-ABCDEF123456";
        let s = format!("auth: {k}; again {k}. Bearer {k}");
        let out = scrub(&s, k);
        assert!(!out.contains("ABCDEF"));
        assert_eq!(out.matches("[hidden]").count(), 3);
    }

    #[test]
    fn removes_json_escaped_form() {
        let k = r#"ab"cd\ef-12345"#;
        let escaped = serde_json::to_string(k).unwrap();
        let out = scrub(&format!("body={escaped}"), k);
        assert!(!out.contains("ef-12345"));
    }

    #[test]
    fn base64_and_percent_encoded_forms_are_hidden() {
        let k = "sk-live-AB/CD+123456:789";
        assert_eq!(base64(b"Man", STD, true), "TWFu");
        assert_eq!(base64(b"Ma", STD, true), "TWE=");
        assert_eq!(base64(b"M", STD, false), "TQ");
        let std = base64(k.as_bytes(), STD, true);
        let url = base64(k.as_bytes(), URL, false);
        let bearer = base64(format!("Bearer {k}").as_bytes(), STD, true);
        let pct: String = url::form_urlencoded::byte_serialize(k.as_bytes()).collect();
        for form in [&std, &url, &bearer, &pct] {
            let out = scrub(&format!("echo {form} end"), k);
            assert_eq!(out, "echo [hidden] end", "{form}");
        }
        // The key behind one or two unknown leading bytes of a base64 stream.
        for lead in 1..3usize {
            let mut bytes = vec![b'q'; lead];
            bytes.extend_from_slice(k.as_bytes());
            let enc = base64(&bytes, STD, false);
            assert!(
                !scrub(&enc, k).contains(&enc[4..enc.len() - 4]),
                "lead {lead}"
            );
        }
    }

    #[test]
    fn a_twelve_character_piece_of_the_key_is_hidden_and_eleven_is_not() {
        let k = "sk-live-ABCDEFGHIJKLMNOPQRSTUV";
        let piece = &k[6..18];
        assert_eq!(piece.len(), 12);
        assert!(!scrub(&format!("x {piece} y"), k).contains(piece));
        let short = &k[6..17];
        assert!(scrub(&format!("x {short} y"), k).contains(short));
    }

    #[test]
    fn short_or_empty_key_changes_nothing() {
        assert_eq!(scrub("hello", ""), "hello");
        assert_eq!(scrub("hello", "he"), "hello");
    }
}
