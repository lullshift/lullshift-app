//! Opening a link from a Companion reply (decision 36, ADR-020 amendment).
//!
//! The UI never opens a link by itself: a click shows Lullshift's own dialog with the full address
//! in ASCII (punycode for international names), and only "Open in browser" calls
//! `companion_open_link`. This module checks the address again, because the reply it came from is
//! untrusted: only `http` and `https` with a host, no user name or password, no control
//! characters or spaces, at most 2,048 characters. The address handed to the system is the
//! parser's own ASCII serialisation, never the raw text.

use super::{CommandError, Res};

/// Longest address that may be opened.
pub const MAX_LINK_CHARS: usize = 2048;

fn refused() -> CommandError {
    CommandError::invalid("Only web addresses that start with http:// or https:// can be opened.")
}

/// The address to open, or why not. Pure: opens nothing.
pub fn check_link(raw: &str) -> Res<String> {
    if raw.is_empty()
        || raw.chars().count() > MAX_LINK_CHARS
        || raw.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return Err(refused());
    }
    // The text itself must start with the scheme and a host: no scheme-relative or
    // backslash forms that a lenient parser would repair into something else.
    let lower = raw.get(..8).unwrap_or(raw).to_ascii_lowercase();
    let rest = if lower.starts_with("https://") {
        &raw[8..]
    } else if lower.starts_with("http://") {
        &raw[7..]
    } else {
        return Err(refused());
    };
    if rest.is_empty() || rest.starts_with(['/', '\\']) {
        return Err(refused());
    }
    let url = url::Url::parse(raw).map_err(|_| refused())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(refused());
    }
    if url.host_str().is_none_or(str::is_empty) {
        return Err(refused());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(CommandError::invalid(
            "Addresses with a user name or password in them aren't opened.",
        ));
    }
    let out = url.as_str().to_owned();
    if !out.is_ascii() || out.chars().count() > MAX_LINK_CHARS {
        return Err(refused());
    }
    Ok(out)
}

/// Checks the address and hands it to the default browser.
pub fn open(raw: &str) -> Res<()> {
    let url = check_link(raw)?;
    tauri_plugin_opener::open_url(&url, None::<&str>).map_err(|_| CommandError {
        kind: "internal",
        message: "The browser couldn't be opened. Copy the address instead.".to_owned(),
        provider_reason: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_http_and_https_addresses_pass() {
        assert_eq!(
            check_link("https://example.com/a?b=1#c").expect("ok"),
            "https://example.com/a?b=1#c"
        );
        assert_eq!(
            check_link("HTTP://Example.COM").expect("ok"),
            "http://example.com/"
        );
        for bad in [
            "",
            "javascript:alert(1)",
            "JaVaScRiPt:alert(1)",
            "data:text/html,<script>alert(1)</script>",
            "file:///C:/Windows/System32/calc.exe",
            "vbscript:msgbox(1)",
            "mailto:a@example.com",
            "ftp://example.com/",
            "ms-settings:privacy",
            "https://",
            "https:///path",
            "https://exa mple.com/",
            "https://example.com/\n",
            "//example.com/",
            "example.com",
            "https://user:pw@example.com/",
            "https://user@example.com/",
        ] {
            assert_eq!(
                check_link(bad).map_err(|e| e.kind),
                Err("invalidInput"),
                "{bad:?}"
            );
        }
        assert!(check_link(&format!("https://example.com/{}", "a".repeat(2100))).is_err());
    }

    #[test]
    fn international_names_are_opened_as_punycode() {
        let out = check_link("https://exämple.com/ä").expect("ok");
        assert_eq!(out, "https://xn--exmple-cua.com/%C3%A4");
        assert!(out.is_ascii());
    }
}
