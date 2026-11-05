//! Per-install IPC token (ADR-009): 32 random bytes, hex-encoded, in `<data>/ipc/token`.

use std::fs;
use std::io::{self, Write};
use std::path::Path;

pub const TOKEN_FILE: &str = "token";
const TOKEN_BYTES: usize = 32;
const TOKEN_HEX_LEN: usize = TOKEN_BYTES * 2;

/// The shared secret. `Debug` never prints it.
#[derive(Clone, PartialEq, Eq)]
pub struct Token(String);

impl std::fmt::Debug for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Token(<redacted>)")
    }
}

impl Token {
    pub fn generate() -> io::Result<Self> {
        let mut bytes = [0u8; TOKEN_BYTES];
        getrandom::fill(&mut bytes).map_err(|_| io::Error::other("no OS randomness"))?;
        Ok(Self(bytes.iter().map(|b| format!("{b:02x}")).collect()))
    }

    fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        (s.len() == TOKEN_HEX_LEN && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
            .then(|| Self(s.to_owned()))
    }

    /// Hex form, for writing into a request. Keep out of logs.
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Constant-time comparison (the length is not secret).
    pub fn matches(&self, candidate: &str) -> bool {
        let (a, b) = (self.0.as_bytes(), candidate.as_bytes());
        if a.len() != b.len() {
            return false;
        }
        a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
    }

    /// Reads the token in `ipc_dir`, creating the directory (0700 on Unix) and a new token
    /// (0600 on Unix) if missing or unreadable as a token. Delete the file to rotate.
    pub fn load_or_create(ipc_dir: &Path) -> io::Result<Self> {
        create_private_dir(ipc_dir)?;
        let path = ipc_dir.join(TOKEN_FILE);
        if let Ok(text) = fs::read_to_string(&path)
            && let Some(t) = Self::parse(&text)
        {
            return Ok(t);
        }
        let token = Self::generate()?;
        let tmp = ipc_dir.join(format!("{TOKEN_FILE}.tmp"));
        let _ = fs::remove_file(&tmp);
        {
            let mut f = private_file(&tmp)?;
            f.write_all(token.0.as_bytes())?;
            f.sync_all()?;
        }
        fs::rename(&tmp, &path)?;
        Ok(token)
    }
}

pub(crate) fn create_private_dir(dir: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
    }
    #[cfg(not(unix))]
    {
        // Under the per-user app data directory; inherits the profile's user-only ACL.
        fs::create_dir_all(dir)
    }
}

fn private_file(path: &Path) -> io::Result<fs::File> {
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    opts.open(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_persists_and_rotates() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ipc = dir.path().join("ipc");
        let a = Token::load_or_create(&ipc).expect("create");
        assert_eq!(a.expose().len(), 64);
        assert_eq!(Token::load_or_create(&ipc).expect("reload"), a);
        fs::remove_file(ipc.join(TOKEN_FILE)).expect("delete");
        assert_ne!(Token::load_or_create(&ipc).expect("rotate"), a);
    }

    #[test]
    fn corrupt_token_file_is_replaced() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join(TOKEN_FILE), "not-a-token").expect("write");
        let t = Token::load_or_create(dir.path()).expect("create");
        assert!(Token::parse(t.expose()).is_some());
    }

    #[test]
    fn compare_and_debug() {
        let t = Token::generate().expect("token");
        assert!(t.matches(t.expose()));
        assert!(!t.matches(&t.expose().replace(&t.expose()[..1], "z")));
        assert!(!t.matches(""));
        assert!(!format!("{t:?}").contains(t.expose()));
    }

    #[cfg(unix)]
    #[test]
    fn unix_permissions_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("tempdir");
        let ipc = dir.path().join("ipc");
        Token::load_or_create(&ipc).expect("create");
        let mode = |p: &Path| fs::metadata(p).expect("meta").permissions().mode() & 0o777;
        assert_eq!(mode(&ipc), 0o700);
        assert_eq!(mode(&ipc.join(TOKEN_FILE)), 0o600);
    }
}
