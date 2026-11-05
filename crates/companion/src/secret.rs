//! The key type. Never printed, never serialised, zeroised on drop.

use std::fmt;

use zeroize::Zeroizing;

use crate::error::CompanionError;

pub const MIN_KEY_CHARS: usize = 8;
pub const MAX_KEY_CHARS: usize = 512;

pub struct Secret(Zeroizing<String>);

impl Secret {
    /// Trims, then accepts 8 to 512 visible ASCII characters (no spaces or control characters).
    /// The error never contains any part of the input.
    pub fn parse(input: &str) -> Result<Self, CompanionError> {
        let t = input.trim();
        let ok = (MIN_KEY_CHARS..=MAX_KEY_CHARS).contains(&t.len())
            && t.bytes().all(|b| (0x21..=0x7e).contains(&b));
        if !ok {
            return Err(CompanionError::invalid_input(
                "A key must be 8 to 512 visible ASCII characters, with no spaces.",
            ));
        }
        Ok(Self(Zeroizing::new(t.to_owned())))
    }

    /// For the provider auth header, the keychain write and `scrub`; nowhere else.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl Clone for Secret {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_and_display_are_redacted() {
        let s = Secret::parse("sk-test-ABCDEFGH12345").expect("ok");
        assert!(!format!("{s:?}{s}").contains("ABCDEFGH"));
        assert_eq!(s.expose(), "sk-test-ABCDEFGH12345");
    }

    #[test]
    fn parse_trims_and_validates_without_echoing() {
        assert!(Secret::parse("  sk-test-ABCDEFGH  ").is_ok());
        for bad in [
            "short",
            "has space inside",
            "tab\tinside1234",
            "unicode-\u{fc}-key-1234",
        ] {
            let e = Secret::parse(bad).expect_err("rejected");
            assert!(!e.message.contains(bad));
        }
        assert!(Secret::parse("").is_err());
        assert!(Secret::parse(&"a".repeat(513)).is_err());
        assert!(Secret::parse(&"a".repeat(512)).is_ok());
    }
}
