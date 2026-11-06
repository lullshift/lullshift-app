//! Validation of everything that comes from an index or a repository: Git URLs, refs, relative
//! paths, names and commit ids (ADR-015). Every function fails closed. The same rules are in
//! `reference/MARKETPLACE_V1.schema.json`; both are driven by the fixtures in
//! `tests/fixtures/marketplace`.

use crate::error::{NameError, PathError, RefError, UrlError};

pub const URL_MAX: usize = 300;
pub const REF_MAX: usize = 200;
pub const PATH_MAX: usize = 300;
/// Most `/`-separated parts in an index path.
pub const PATH_PARTS_MAX: usize = 12;
const SEGMENT_MAX: usize = 64;
/// Host name suffixes that mean "this computer or a local network".
const LOCAL_SUFFIXES: &[&str] = &[
    "localhost",
    "local",
    "internal",
    "lan",
    "localdomain",
    "intranet",
    "corp",
    "home",
    "private",
    "arpa",
];

/// The name only the built-in marketplace may use.
pub const RESERVED_NAME: &str = "lullshift";

/// How a host name reads, before anything is resolved (decision 17).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HostClass {
    /// A DNS name with at least two labels and a public-looking top-level label.
    Public,
    /// An IPv4 address, `localhost`, a single-label name or a local-network suffix such as
    /// `.local`, `.lan`, `.internal`, `.corp` or `.home`. Usable only after the user confirms
    /// that the marketplace is on their own computer or network.
    Private,
}

/// The transport an address uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scheme {
    Https,
    Ssh,
    /// Tests only.
    File,
}

/// A Git repository address that passed validation. `as_str` is what `git` receives;
/// `display` is what the UI and logs show (no userinfo).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GitUrl {
    url: String,
    display: String,
    scheme: Scheme,
    host: String,
    port: Option<u16>,
    class: HostClass,
    /// The user ticked "This marketplace is on my own computer or network" for this address.
    /// Never set by parsing; only by [`GitUrl::confirm_private`].
    private_confirmed: bool,
}

impl GitUrl {
    /// Parses `https://host/path`, `ssh://[user@]host[:port]/path`, `git@host:path` or the
    /// GitHub shorthand `owner/repo`. Nothing else is accepted, in particular not `file://`,
    /// `http://`, `ext::`, credentials or non-canonical IP spellings. Private hosts (IPv4
    /// addresses, `localhost`, single-label names, local-network suffixes) parse, but are
    /// marked [`HostClass::Private`] and can't be fetched until [`GitUrl::confirm_private`].
    pub fn parse(input: &str) -> Result<Self, UrlError> {
        if input.is_empty() {
            return Err(UrlError::Empty);
        }
        if input.len() > URL_MAX {
            return Err(UrlError::TooLong);
        }
        if !input.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'/' | b':' | b'@')
        }) {
            return Err(UrlError::ForbiddenCharacter);
        }
        if let Some(rest) = input.strip_prefix("https://") {
            let (authority, path) = split_authority(rest)?;
            if authority.contains('@') {
                return Err(UrlError::Credentials);
            }
            let (host, port, class) = host_port(authority)?;
            check_url_path(path)?;
            let display = format!("https://{host}{}{path}", port_text(port));
            return Ok(Self::build(
                input.to_owned(),
                display,
                Scheme::Https,
                host,
                port,
                class,
            ));
        }
        if let Some(rest) = input.strip_prefix("ssh://") {
            let (authority, path) = split_authority(rest)?;
            let (user, hostport) = match authority.split_once('@') {
                Some((user, hostport)) => {
                    if hostport.contains('@') || user.contains(':') {
                        return Err(UrlError::Credentials);
                    }
                    check_ssh_user(user)?;
                    (Some(user), hostport)
                }
                None => (None, authority),
            };
            let (host, port, class) = host_port(hostport)?;
            check_url_path(path)?;
            let display = format!("ssh://{host}{}{path}", port_text(port));
            let url = match user {
                Some(user) => format!("ssh://{user}@{host}{}{path}", port_text(port)),
                None => display.clone(),
            };
            return Ok(Self::build(url, display, Scheme::Ssh, host, port, class));
        }
        if let Some(rest) = input.strip_prefix("git@") {
            let (host, path) = rest.split_once(':').ok_or(UrlError::BadPath)?;
            let class = check_host(host)?;
            check_segments(path, 10).map_err(|()| UrlError::BadPath)?;
            return Ok(Self::build(
                format!("ssh://git@{host}/{path}"),
                format!("{host}:{path}"),
                Scheme::Ssh,
                host,
                None,
                class,
            ));
        }
        if input.contains("://") {
            return Err(UrlError::UnsupportedScheme);
        }
        let (owner, repo) = input.split_once('/').ok_or(UrlError::BadShorthand)?;
        let owner_ok = (1..=39).contains(&owner.len())
            && owner.as_bytes()[0].is_ascii_alphanumeric()
            && owner
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-');
        let repo_ok = (1..=100).contains(&repo.len())
            && (repo.as_bytes()[0].is_ascii_alphanumeric() || repo.as_bytes()[0] == b'_')
            && repo
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
        if !(owner_ok && repo_ok) {
            return Err(UrlError::BadShorthand);
        }
        let url = format!("https://github.com/{owner}/{repo}");
        Ok(Self::build(
            url.clone(),
            url,
            Scheme::Https,
            "github.com",
            None,
            HostClass::Public,
        ))
    }

    fn build(
        url: String,
        display: String,
        scheme: Scheme,
        host: &str,
        port: Option<u16>,
        class: HostClass,
    ) -> Self {
        Self {
            url,
            display,
            scheme,
            host: host.to_ascii_lowercase(),
            port,
            class,
            private_confirmed: false,
        }
    }

    /// The address passed to `git`. May contain an ssh user name (never a password).
    pub fn as_str(&self) -> &str {
        &self.url
    }

    /// The address to show and log: no userinfo.
    pub fn display(&self) -> &str {
        &self.display
    }

    pub fn scheme(&self) -> Scheme {
        self.scheme
    }

    /// The host name or IPv4 address, lowercased. Empty for `file://`.
    pub fn host(&self) -> &str {
        &self.host
    }

    /// The port written in the address, if any.
    pub fn port(&self) -> Option<u16> {
        self.port
    }

    /// The port a connection uses: the written one, or 443 for https and 22 for ssh.
    pub fn effective_port(&self) -> u16 {
        self.port.unwrap_or(match self.scheme {
            Scheme::Https | Scheme::File => 443,
            Scheme::Ssh => 22,
        })
    }

    /// How the host name reads (before resolving it).
    pub fn host_class(&self) -> HostClass {
        self.class
    }

    /// True if the address itself names a private host (decision 17).
    pub fn is_private_host(&self) -> bool {
        self.class == HostClass::Private
    }

    /// The same address, marked as confirmed by the user to be on their own computer or
    /// network. Only the add-marketplace and change-address flows call this, and only when the
    /// user ticked the confirmation; the confirmation is stored with the marketplace.
    #[must_use]
    pub fn confirm_private(mut self) -> Self {
        self.private_confirmed = true;
        self
    }

    /// True if the user confirmed this address as private.
    pub fn private_confirmed(&self) -> bool {
        self.private_confirmed
    }

    /// A `file://` address, for tests only. Production code cannot build one: this function
    /// does not exist outside `cfg(test)`, and [`GitUrl::parse`] refuses `file://`.
    #[cfg(any(test, all(feature = "unsafe-test-file-urls", debug_assertions)))]
    pub fn for_test_file(path: &std::path::Path) -> Self {
        let p = path.to_string_lossy().replace('\\', "/");
        let url = if p.starts_with('/') {
            format!("file://{p}")
        } else {
            format!("file:///{p}")
        };
        Self::build(url.clone(), url, Scheme::File, "", None, HostClass::Public)
    }

    /// An https address on the loopback test server (`testserver`), already confirmed as
    /// private because it resolves to 127.0.0.1. Tests only.
    #[cfg(test)]
    pub(crate) fn for_test_https(host: &str, port: u16, path: &str) -> Self {
        let url = format!("https://{host}:{port}{path}");
        Self::build(
            url.clone(),
            url,
            Scheme::Https,
            host,
            Some(port),
            HostClass::Private,
        )
        .confirm_private()
    }
}

fn port_text(port: Option<u16>) -> String {
    port.map_or_else(String::new, |p| format!(":{p}"))
}

/// `host[:port]/path...` into (`host[:port]`, `/path...`).
fn split_authority(rest: &str) -> Result<(&str, &str), UrlError> {
    let i = rest.find('/').ok_or(UrlError::BadPath)?;
    Ok((&rest[..i], &rest[i..]))
}

fn host_port(authority: &str) -> Result<(&str, Option<u16>, HostClass), UrlError> {
    let mut parts = authority.split(':');
    let host = parts.next().unwrap_or_default();
    let port = parts.next();
    if parts.next().is_some() {
        // More than one colon: an IPv6 literal or garbage.
        return Err(UrlError::BadHost);
    }
    let class = check_host(host)?;
    let port = match port {
        None => None,
        Some(p) => {
            if p.is_empty()
                || p.len() > 5
                || p.starts_with('0')
                || !p.bytes().all(|b| b.is_ascii_digit())
            {
                return Err(UrlError::BadPort);
            }
            match p.parse::<u32>() {
                Ok(n) if (1..=65535).contains(&n) => Some(n as u16),
                _ => return Err(UrlError::BadPort),
            }
        }
    };
    Ok((host, port, class))
}

fn check_ssh_user(user: &str) -> Result<(), UrlError> {
    let ok = (1..=32).contains(&user.len())
        && user.as_bytes()[0].is_ascii_alphanumeric()
        && user
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
    if ok { Ok(()) } else { Err(UrlError::BadHost) }
}

/// Classifies a host: a public DNS name (at least two labels, a top-level label that starts
/// with a letter and is not a local-network suffix), or a private host (a canonical dotted
/// IPv4 address, `localhost`, a one-label name or a local-network suffix). Non-canonical IPv4
/// spellings (`2130706433`, `0x7f.1`, `010.0.0.1`, `127.1`) are refused, because they hide what
/// they point at; so are addresses that can never be a Git server (`0.0.0.0/8`, link-local
/// `169.254.0.0/16` including cloud metadata services, multicast, reserved and broadcast).
fn check_host(host: &str) -> Result<HostClass, UrlError> {
    if host.is_empty() || host.len() > 253 || host.starts_with('-') {
        return Err(UrlError::BadHost);
    }
    let labels: Vec<&str> = host.split('.').collect();
    if !labels.iter().all(|l| {
        (1..=63).contains(&l.len())
            && !l.starts_with('-')
            && !l.ends_with('-')
            && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    }) {
        return Err(UrlError::BadHost);
    }
    let tld = labels
        .last()
        .copied()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if tld.as_bytes()[0].is_ascii_digit() {
        // Only the canonical dotted-quad spelling is an IP address we accept.
        let ip = canonical_ipv4(host).ok_or(UrlError::BadHost)?;
        return match crate::net::classify_ip(std::net::IpAddr::V4(ip)) {
            crate::net::AddrClass::Never => Err(UrlError::SpecialAddress),
            _ => Ok(HostClass::Private),
        };
    }
    if labels.len() < 2 || tld.len() < 2 || LOCAL_SUFFIXES.contains(&tld.as_str()) {
        return Ok(HostClass::Private);
    }
    Ok(HostClass::Public)
}

/// `a.b.c.d` with four decimal parts of 0-255 and no leading zeros.
fn canonical_ipv4(host: &str) -> Option<std::net::Ipv4Addr> {
    let parts: Vec<&str> = host.split('.').collect();
    if parts.len() != 4 {
        return None;
    }
    let mut octets = [0_u8; 4];
    for (slot, p) in octets.iter_mut().zip(&parts) {
        if p.is_empty()
            || p.len() > 3
            || !p.bytes().all(|b| b.is_ascii_digit())
            || (p.len() > 1 && p.starts_with('0'))
        {
            return None;
        }
        *slot = p.parse().ok()?;
    }
    Some(std::net::Ipv4Addr::from(octets))
}

fn check_url_path(path: &str) -> Result<(), UrlError> {
    let inner = path.strip_prefix('/').ok_or(UrlError::BadPath)?;
    check_segments(inner, 10).map_err(|()| UrlError::BadPath)
}

/// Segments of a repository path: non-empty, start with a letter, digit or underscore (so no
/// `.`, `..`, `.git` or leading `-`), then letters, digits, `.`, `_`, `-`. `repo.git` is fine.
fn check_segments(path: &str, max: usize) -> Result<(), ()> {
    let mut n = 0;
    for seg in path.split('/') {
        n += 1;
        let b = seg.as_bytes();
        let ok = !b.is_empty()
            && b.len() <= 100
            && (b[0].is_ascii_alphanumeric() || b[0] == b'_')
            && b.iter()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-'));
        if !ok {
            return Err(());
        }
    }
    if n == 0 || n > max { Err(()) } else { Ok(()) }
}

/// Removes userinfo, query and fragment from anything that looks like a URL, for showing in
/// messages. Works on unvalidated text: when in doubt it removes more. Control and invisible
/// characters are replaced and the result is at most 300 characters.
pub fn scrub_url(text: &str) -> String {
    let cleaned: String = text
        .chars()
        .take(URL_MAX)
        .map(|c| {
            if c.is_control() || crate::manifest::hidden_char(c) || c.is_whitespace() {
                '?'
            } else {
                c
            }
        })
        .collect();
    let cut = cleaned.find(['?', '#']).unwrap_or(cleaned.len());
    let s = &cleaned[..cut];
    let (scheme, rest) = match s.find("://") {
        Some(i) => (&s[..i + 3], &s[i + 3..]),
        None => ("", s),
    };
    let rest = rest.rsplit_once('@').map_or(rest, |(_, after)| after);
    format!("{scheme}{rest}")
}

/// A pinned commit: 40 lowercase hex digits.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Commit(String);

impl Commit {
    pub fn parse(text: &str) -> Option<Self> {
        (text.len() == 40 && text.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| Self(text.to_ascii_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Commit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A branch, tag or commit to resolve. Only the syntax is checked here; use
/// [`crate::Git::check_ref_format`] (done by every fetch) for Git's own check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitRef {
    Commit(Commit),
    Name(String),
}

impl GitRef {
    pub fn parse(text: &str) -> Result<Self, RefError> {
        if text.is_empty() {
            return Err(RefError::Empty);
        }
        if text.len() > REF_MAX {
            return Err(RefError::TooLong);
        }
        if text.starts_with('-') {
            return Err(RefError::LooksLikeOption);
        }
        if let Some(c) = Commit::parse(text) {
            return Ok(Self::Commit(c));
        }
        if !text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'/' | b'-'))
        {
            return Err(RefError::ForbiddenCharacter);
        }
        let bad_part = |p: &str| p.is_empty() || p.starts_with('.') || p.ends_with(".lock");
        if text.contains("..") || text.ends_with('.') || text.split('/').any(bad_part) {
            return Err(RefError::BadShape);
        }
        Ok(Self::Name(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Commit(c) => c.as_str(),
            Self::Name(n) => n,
        }
    }
}

const WINDOWS_RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// True if Windows treats `name` as a device: the part before the first `.`, any case.
pub fn windows_reserved(name: &str) -> bool {
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    WINDOWS_RESERVED.contains(&stem.as_str())
}

/// One part of a path inside a repository or an installed pack.
pub fn check_segment(seg: &str) -> Result<(), PathError> {
    if seg.is_empty() || seg == "." || seg == ".." {
        return Err(PathError::BadSegment);
    }
    if seg.eq_ignore_ascii_case(".git") {
        return Err(PathError::GitDir);
    }
    if seg.starts_with('-') {
        return Err(PathError::LooksLikeOption);
    }
    if seg.len() > SEGMENT_MAX {
        return Err(PathError::TooLong);
    }
    let b = seg.as_bytes();
    let ok = (b[0].is_ascii_alphanumeric() || b[0] == b'_')
        && b.iter()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-'))
        && !seg.ends_with('.');
    if !ok {
        return Err(PathError::ForbiddenCharacter);
    }
    if windows_reserved(seg) {
        return Err(PathError::WindowsReserved);
    }
    Ok(())
}

/// A `/`-separated path of checked segments, as used in index `path` fields. Never empty,
/// absolute, or containing `.`, `..`, `.git`, a leading `-` or a Windows reserved name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RelPath(String);

impl RelPath {
    pub fn parse(text: &str) -> Result<Self, PathError> {
        if text.is_empty() {
            return Err(PathError::Empty);
        }
        if text.len() > PATH_MAX {
            return Err(PathError::TooLong);
        }
        let mut n = 0;
        for seg in text.split('/') {
            check_segment(seg)?;
            n += 1;
        }
        if n > PATH_PARTS_MAX {
            return Err(PathError::TooDeep);
        }
        Ok(Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// `^[a-z0-9][a-z0-9-]{1,39}$`, and `lullshift` only for the built-in marketplace.
pub fn check_marketplace_name(name: &str, builtin: bool) -> Result<(), NameError> {
    let b = name.as_bytes();
    let ok = (2..=40).contains(&b.len())
        && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit())
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-');
    if !ok {
        return Err(NameError::Invalid);
    }
    if name == RESERVED_NAME && !builtin {
        return Err(NameError::Reserved);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn good_urls() {
        for (input, git, shown) in [
            (
                "https://github.com/o/r",
                "https://github.com/o/r",
                "https://github.com/o/r",
            ),
            (
                "https://gitlab.example.org:8443/a/b/c.git",
                "https://gitlab.example.org:8443/a/b/c.git",
                "https://gitlab.example.org:8443/a/b/c.git",
            ),
            (
                "ssh://git@host.example.com/o/r.git",
                "ssh://git@host.example.com/o/r.git",
                "ssh://host.example.com/o/r.git",
            ),
            (
                "ssh://host.example.com:2222/o/r",
                "ssh://host.example.com:2222/o/r",
                "ssh://host.example.com:2222/o/r",
            ),
            (
                "git@github.com:o/r.git",
                "ssh://git@github.com/o/r.git",
                "github.com:o/r.git",
            ),
            (
                "owner/repo",
                "https://github.com/owner/repo",
                "https://github.com/owner/repo",
            ),
            (
                "Owner-1/re_po.v2",
                "https://github.com/Owner-1/re_po.v2",
                "https://github.com/Owner-1/re_po.v2",
            ),
        ] {
            let u = GitUrl::parse(input).unwrap_or_else(|e| panic!("{input}: {e:?}"));
            assert_eq!(u.as_str(), git, "{input}");
            assert_eq!(u.display(), shown, "{input}");
        }
    }

    #[test]
    fn bad_urls() {
        let long = format!("https://example.com/{}", "a".repeat(300));
        for (input, why) in [
            ("", UrlError::Empty),
            (long.as_str(), UrlError::TooLong),
            // secretlint-disable-next-line -- fake credential: the test checks it is refused
            ("https://user:token@github.com/o/r", UrlError::Credentials),
            ("https://token@github.com/o/r", UrlError::Credentials),
            (
                "https://github.com@evil.example.com/o/r",
                UrlError::Credentials,
            ),
            ("ssh://user:pw@host.example.com/o/r", UrlError::Credentials),
            (
                "ssh://-oProxyCommand=evil/o/r",
                UrlError::ForbiddenCharacter,
            ),
            ("ssh://-oProxyCommand/o/r", UrlError::BadHost),
            (
                "ssh://-oProxyCommand@host.example.com/o/r",
                UrlError::BadHost,
            ),
            (
                "ssh://git@-oProxyCommand.example.com/o/r",
                UrlError::BadHost,
            ),
            ("git@-oProxyCommand.example.com:o/r", UrlError::BadHost),
            (
                "git@-oProxyCommand=evil.example.com:o/r",
                UrlError::ForbiddenCharacter,
            ),
            ("-oProxyCommand=x/repo", UrlError::ForbiddenCharacter),
            ("-owner/repo", UrlError::BadShorthand),
            (".owner/repo", UrlError::BadShorthand),
            ("owner/.repo", UrlError::BadShorthand),
            ("owner/-repo", UrlError::BadShorthand),
            ("owner/repo/extra", UrlError::BadShorthand),
            ("justoneword", UrlError::BadShorthand),
            ("ext::sh -c touch% /tmp/x", UrlError::ForbiddenCharacter),
            ("ext::sh", UrlError::BadShorthand),
            ("http://github.com/o/r", UrlError::UnsupportedScheme),
            ("file:///etc/passwd", UrlError::UnsupportedScheme),
            ("git://github.com/o/r", UrlError::UnsupportedScheme),
            ("https://github.com/../r", UrlError::BadPath),
            ("https://github.com/o/../r", UrlError::BadPath),
            ("https://github.com/o/.git", UrlError::BadPath),
            ("https://github.com/.git/r", UrlError::BadPath),
            ("https://github.com/o/-r", UrlError::BadPath),
            ("https://github.com/o//r", UrlError::BadPath),
            ("https://github.com/o/r/", UrlError::BadPath),
            ("https://github.com", UrlError::BadPath),
            ("git@github.com:../r", UrlError::BadPath),
            ("git@github.com:/o/r", UrlError::BadPath),
            // Addresses that can never be a Git server, and IP spellings that hide what they
            // point at, stay refused even with the private-network confirmation.
            ("https://169.254.169.254/o/r", UrlError::SpecialAddress),
            ("https://0.0.0.0/o/r", UrlError::SpecialAddress),
            ("https://224.0.0.1/o/r", UrlError::SpecialAddress),
            ("https://255.255.255.255/o/r", UrlError::SpecialAddress),
            ("https://2130706433/o/r", UrlError::BadHost),
            ("https://0x7f.1/o/r", UrlError::BadHost),
            ("https://0x7f000001/o/r", UrlError::BadHost),
            ("https://127.1/o/r", UrlError::BadHost),
            ("https://010.0.0.1/o/r", UrlError::BadHost),
            ("https://127.0.0.256/o/r", UrlError::BadHost),
            ("https://1.2.3.4.5/o/r", UrlError::BadHost),
            ("https://[::1]/o/r", UrlError::ForbiddenCharacter),
            // Credentials, `%`, `=` and plain http stay refused for private hosts too.
            ("https://user:pw@10.0.0.5/o/r", UrlError::Credentials),
            ("https://tok@localhost/o/r", UrlError::Credentials),
            ("http://192.168.1.10/o/r", UrlError::UnsupportedScheme),
            ("http://localhost/o/r", UrlError::UnsupportedScheme),
            ("https://gitserver/o/r%2e%2e", UrlError::ForbiddenCharacter),
            ("https://gitserver/o/r?x=1", UrlError::ForbiddenCharacter),
            ("https://gitsérver/o/r", UrlError::ForbiddenCharacter),
            ("https://-evil.example.com/o/r", UrlError::BadHost),
            ("https://exa_mple.com/o/r", UrlError::BadHost),
            ("https://example.com:0/o/r", UrlError::BadPort),
            ("https://example.com:99999/o/r", UrlError::BadPort),
            ("https://example.com:/o/r", UrlError::BadPort),
            (
                "https://example.com/o/r with space",
                UrlError::ForbiddenCharacter,
            ),
            ("https://example.com/o/r\n", UrlError::ForbiddenCharacter),
            ("https://example.com/o/r\u{0}", UrlError::ForbiddenCharacter),
            (
                "https://example.com/o/r%2e%2e",
                UrlError::ForbiddenCharacter,
            ),
            ("https://exämple.com/o/r", UrlError::ForbiddenCharacter),
            ("HTTPS://github.com/o/r", UrlError::UnsupportedScheme),
        ] {
            assert_eq!(GitUrl::parse(input).err(), Some(why), "{input:?}");
        }
    }

    #[test]
    fn private_hosts_parse_but_are_marked_and_unconfirmed() {
        for (input, git, host, port) in [
            (
                "https://localhost/o/r",
                "https://localhost/o/r",
                "localhost",
                443,
            ),
            (
                "https://127.0.0.1:8443/o/r.git",
                "https://127.0.0.1:8443/o/r.git",
                "127.0.0.1",
                8443,
            ),
            (
                "https://10.0.0.5/o/r",
                "https://10.0.0.5/o/r",
                "10.0.0.5",
                443,
            ),
            (
                "https://192.168.1.10/o/r",
                "https://192.168.1.10/o/r",
                "192.168.1.10",
                443,
            ),
            // A public IP literal also needs the confirmation: it is not a name anyone vouches for.
            ("https://8.8.8.8/o/r", "https://8.8.8.8/o/r", "8.8.8.8", 443),
            (
                "https://gitserver/o/r",
                "https://gitserver/o/r",
                "gitserver",
                443,
            ),
            (
                "https://printer.local/o/r",
                "https://printer.local/o/r",
                "printer.local",
                443,
            ),
            (
                "https://git.corp/o/r",
                "https://git.corp/o/r",
                "git.corp",
                443,
            ),
            (
                "https://Server.LAN/o/r",
                "https://Server.LAN/o/r",
                "server.lan",
                443,
            ),
            (
                "https://git.internal/o/r",
                "https://git.internal/o/r",
                "git.internal",
                443,
            ),
            (
                "https://nas.home/o/r",
                "https://nas.home/o/r",
                "nas.home",
                443,
            ),
            (
                "ssh://git@192.168.1.10:2222/o/r",
                "ssh://git@192.168.1.10:2222/o/r",
                "192.168.1.10",
                2222,
            ),
            (
                "git@gitserver:o/r.git",
                "ssh://git@gitserver/o/r.git",
                "gitserver",
                22,
            ),
        ] {
            let u = GitUrl::parse(input).unwrap_or_else(|e| panic!("{input}: {e:?}"));
            assert_eq!(u.as_str(), git, "{input}");
            assert_eq!(u.host(), host, "{input}");
            assert_eq!(u.effective_port(), port, "{input}");
            assert!(u.is_private_host(), "{input} is private");
            assert!(!u.private_confirmed(), "{input}: parsing never confirms");
            assert!(u.clone().confirm_private().private_confirmed());
        }
        for public in [
            "https://github.com/o/r",
            "owner/repo",
            "git@gitlab.example.org:o/r",
            "ssh://git@host.example.com:2222/o/r",
        ] {
            let u = GitUrl::parse(public).expect("public");
            assert_eq!(u.host_class(), HostClass::Public, "{public}");
        }
    }

    #[test]
    fn scrub_removes_userinfo_query_and_fragment() {
        for (input, out) in [
            ("https://user:tok@github.com/o/r", "https://github.com/o/r"),
            ("https://user:p@ss@github.com/o/r", "https://github.com/o/r"),
            (
                "https://user:pa/ss@github.com/o/r",
                "https://github.com/o/r",
            ),
            (
                "https://github.com/o/r?token=abc#frag",
                "https://github.com/o/r",
            ),
            ("git@github.com:o/r.git", "github.com:o/r.git"),
            (
                "ssh://git@host.example.com/o/r",
                "ssh://host.example.com/o/r",
            ),
            ("plain", "plain"),
        ] {
            assert_eq!(scrub_url(input), out, "{input}");
        }
        assert!(!scrub_url("https://a:b@c.example/x\u{7}y\n").contains(['\u{7}', '\n', 'b']));
        assert!(scrub_url(&"x".repeat(1000)).len() <= URL_MAX);
    }

    #[test]
    fn refs() {
        for good in [
            "main",
            "v1.2.3",
            "release/2026-11",
            "feature/a_b.c",
            "a",
            "HEAD",
        ] {
            assert!(GitRef::parse(good).is_ok(), "{good}");
        }
        let sha = "A".repeat(40);
        assert_eq!(
            GitRef::parse(&sha).expect("sha"),
            GitRef::Commit(Commit::parse(&"a".repeat(40)).expect("commit"))
        );
        let long = "a".repeat(201);
        for (bad, why) in [
            ("", RefError::Empty),
            (long.as_str(), RefError::TooLong),
            ("-main", RefError::LooksLikeOption),
            ("--upload-pack=evil", RefError::LooksLikeOption),
            ("a..b", RefError::BadShape),
            ("a b", RefError::ForbiddenCharacter),
            ("a:b", RefError::ForbiddenCharacter),
            ("a^b", RefError::ForbiddenCharacter),
            ("a~1", RefError::ForbiddenCharacter),
            ("a*", RefError::ForbiddenCharacter),
            ("a?", RefError::ForbiddenCharacter),
            ("a[1]", RefError::ForbiddenCharacter),
            ("a\\b", RefError::ForbiddenCharacter),
            ("a@{1}", RefError::ForbiddenCharacter),
            ("a\nb", RefError::ForbiddenCharacter),
            ("a//b", RefError::BadShape),
            ("/a", RefError::BadShape),
            ("a/", RefError::BadShape),
            ("a.", RefError::BadShape),
            ("a.lock", RefError::BadShape),
            ("a/b.lock/c", RefError::BadShape),
            (".hidden", RefError::BadShape),
            ("a/.b", RefError::BadShape),
        ] {
            assert_eq!(GitRef::parse(bad).err(), Some(why), "{bad:?}");
        }
    }

    #[test]
    fn rel_paths() {
        for good in ["a", "activities/content/x", "a-b/c_d/e.f", "_x", "a/b.c.d"] {
            assert!(RelPath::parse(good).is_ok(), "{good}");
        }
        let deep = vec!["a"; 13].join("/");
        let long = "a".repeat(301);
        let seg_long = "a".repeat(65);
        for (bad, why) in [
            ("", PathError::Empty),
            (long.as_str(), PathError::TooLong),
            (deep.as_str(), PathError::TooDeep),
            (seg_long.as_str(), PathError::TooLong),
            ("/abs", PathError::BadSegment),
            ("a//b", PathError::BadSegment),
            ("a/", PathError::BadSegment),
            (".", PathError::BadSegment),
            ("..", PathError::BadSegment),
            ("a/../b", PathError::BadSegment),
            ("a/./b", PathError::BadSegment),
            (".git", PathError::GitDir),
            ("a/.GIT/b", PathError::GitDir),
            ("-rf", PathError::LooksLikeOption),
            ("a/-b", PathError::LooksLikeOption),
            (".hidden", PathError::ForbiddenCharacter),
            ("a.", PathError::ForbiddenCharacter),
            ("a b", PathError::ForbiddenCharacter),
            ("a\\b", PathError::ForbiddenCharacter),
            ("C:/x", PathError::ForbiddenCharacter),
            ("a\u{0}b", PathError::ForbiddenCharacter),
            ("caf\u{e9}", PathError::ForbiddenCharacter),
            ("CON", PathError::WindowsReserved),
            ("a/nul", PathError::WindowsReserved),
            ("aux.txt", PathError::WindowsReserved),
            ("COM1", PathError::WindowsReserved),
            ("x/lpt9.json", PathError::WindowsReserved),
        ] {
            assert_eq!(RelPath::parse(bad).err(), Some(why), "{bad:?}");
        }
    }

    #[test]
    fn names() {
        assert!(check_marketplace_name("acme-tools", false).is_ok());
        assert_eq!(
            check_marketplace_name("lullshift", false),
            Err(NameError::Reserved)
        );
        assert!(check_marketplace_name("lullshift", true).is_ok());
        for bad in ["", "a", "-a", "A-b", "a_b", &"a".repeat(41)] {
            assert_eq!(
                check_marketplace_name(bad, false),
                Err(NameError::Invalid),
                "{bad}"
            );
        }
    }

    #[test]
    fn commits() {
        assert!(Commit::parse(&"0".repeat(40)).is_some());
        assert!(Commit::parse(&"0".repeat(39)).is_none());
        assert!(Commit::parse(&"g".repeat(40)).is_none());
        assert!(Commit::parse(&"0".repeat(64)).is_none());
    }
}
