//! Finding out where a moved repository went (decision 18.1, ADR-015 amendment).
//!
//! Git runs with `http.followRedirects=false`, because Git can't limit redirects to one host.
//! When an https fetch fails in a way a redirect would cause (Git reports "not found" or a
//! generic failure), Lullshift asks the server once, itself, where the repository is:
//!
//! - one `GET <url>/info/refs?service=git-upload-pack`, the request Git would make;
//! - redirects off, no credentials, no cookies, a 10-second limit, the body never read;
//! - the destination is checked first exactly as for a fetch (private hosts, resolved
//!   addresses), and only https is ever probed; the request connects only to the addresses
//!   that check approved ([`PinnedResolver`]), and a name that didn't resolve isn't probed;
//! - a `Location` is used only if it is https and names the same Git endpoint
//!   (`.../info/refs?service=git-upload-pack`); the rest must pass [`GitUrl::parse`].
//!
//! The answer is never followed automatically. The fetch fails with [`Error::Moved`], and the
//! app asks the user. A move to the same host and port can be accepted in one step; a move to a
//! different host is shown so the user can add or change the address deliberately.

use std::net::SocketAddr;
use std::time::Duration;

use ureq::unversioned::resolver::{DefaultResolver, ResolvedSocketAddrs, Resolver};
use ureq::unversioned::transport::{DefaultConnector, NextTimeout};

use crate::error::Error;
use crate::net::{Destination, Resolve, check_destination_with, system_resolve};
use crate::validate::{GitUrl, Scheme};

/// How long the probe may take in all.
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
/// The path Git asks for first; a redirect of anything else is not a repository move.
const ENDPOINT: &str = "/info/refs?service=git-upload-pack";

/// Where a repository moved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Moved {
    pub to: GitUrl,
    /// Same host name (case-insensitive) and same port.
    pub same_host: bool,
}

/// Reads a redirect answer: `status` and `location` from the probe of `from`. `None` unless it
/// is a 301, 302, 303, 307 or 308 to a usable https Git address that differs from `from`.
pub fn moved_to(from: &GitUrl, status: u16, location: Option<&str>) -> Option<Moved> {
    if !matches!(status, 301 | 302 | 303 | 307 | 308) {
        return None;
    }
    let location = location?.trim();
    let absolute = if location.starts_with('/') && !location.starts_with("//") {
        let port = from.port().map_or_else(String::new, |p| format!(":{p}"));
        format!("https://{}{port}{location}", from.host())
    } else if location.starts_with("https://") {
        location.to_owned()
    } else {
        // http://, scheme-relative, relative paths, anything else.
        return None;
    };
    let base = absolute.strip_suffix(ENDPOINT)?;
    let to = GitUrl::parse(base).ok()?;
    if to.scheme() != Scheme::Https || to.display() == from.display() {
        return None;
    }
    let same_host = to.host() == from.host() && to.effective_port() == from.effective_port();
    Some(Moved {
        to: if same_host && from.private_confirmed() {
            // Same host, so the user's private-network answer still describes it.
            to.confirm_private()
        } else {
            to
        },
        same_host,
    })
}

/// Asks the server behind `url` whether the repository moved. `None` for anything but https,
/// for a destination the fetch rules refuse or that didn't resolve, and for any answer that
/// isn't a usable redirect. The request goes only to the addresses that were checked
/// ([`PinnedResolver`]), never to whatever a second lookup of the name returns.
pub fn probe(url: &GitUrl) -> Option<Moved> {
    probe_with(url, system_resolve)
}

/// [`probe`] with a given resolver for the check.
pub(crate) fn probe_with(url: &GitUrl, resolve: Resolve) -> Option<Moved> {
    if url.scheme() != Scheme::Https {
        return None;
    }
    let Ok(Destination::Resolved(addrs)) = check_destination_with(url, resolve) else {
        return None;
    };
    let pin = PinnedResolver::new(url.host(), url.effective_port(), &addrs)?;
    let (status, location) = ask(&agent(true, pin), &format!("{}{ENDPOINT}", url.as_str()))?;
    moved_to(url, status, location.as_deref())
}

/// A ureq resolver that answers only for the checked host and port, with only the checked
/// addresses (security review LOW-1). Any other name fails, except the user's own configured
/// proxy, which ureq resolves to reach the proxy itself; behind a proxy the proxy resolves the
/// target and no pin can apply (ADR-015 amendment).
#[derive(Debug)]
pub(crate) struct PinnedResolver {
    host: String,
    port: u16,
    addrs: Vec<SocketAddr>,
}

impl PinnedResolver {
    /// `None` without any address: there is nothing checked to connect to.
    pub(crate) fn new(host: &str, port: u16, addrs: &[SocketAddr]) -> Option<Self> {
        if addrs.is_empty() {
            return None;
        }
        Some(Self {
            host: host.to_ascii_lowercase(),
            port,
            addrs: addrs.to_vec(),
        })
    }
}

impl Resolver for PinnedResolver {
    fn resolve(
        &self,
        uri: &ureq::http::Uri,
        config: &ureq::config::Config,
        timeout: NextTimeout,
    ) -> Result<ResolvedSocketAddrs, ureq::Error> {
        let host = uri.host().unwrap_or_default().to_ascii_lowercase();
        let port = uri
            .port_u16()
            .unwrap_or(if uri.scheme_str() == Some("http") {
                80
            } else {
                443
            });
        if host == self.host && port == self.port {
            let mut out = self.empty();
            for a in self.addrs.iter().take(16) {
                out.push(*a);
            }
            return Ok(out);
        }
        if config
            .proxy()
            .is_some_and(|p| p.host().eq_ignore_ascii_case(&host) && p.port() == port)
        {
            return DefaultResolver::default().resolve(uri, config, timeout);
        }
        Err(ureq::Error::HostNotFound)
    }
}

fn agent(https_only: bool, resolver: PinnedResolver) -> ureq::Agent {
    use ureq::config::Config;
    use ureq::tls::{RootCerts, TlsConfig};
    let config = Config::builder()
        .http_status_as_error(false)
        .max_redirects(0)
        .max_redirects_will_error(false)
        .timeout_global(Some(PROBE_TIMEOUT))
        .https_only(https_only)
        .user_agent("git/2 (Lullshift redirect check)")
        .tls_config(
            TlsConfig::builder()
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        )
        .build();
    ureq::Agent::with_parts(config, DefaultConnector::default(), resolver)
}

/// One GET, nothing followed, nothing sent but the request line and ureq's default headers.
fn ask(agent: &ureq::Agent, url: &str) -> Option<(u16, Option<String>)> {
    let resp = agent.get(url).call().ok()?;
    let status = resp.status().as_u16();
    let location = resp
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .filter(|v| v.len() <= 2048)
        .map(str::to_owned);
    // The body is dropped unread.
    Some((status, location))
}

impl From<Moved> for Error {
    fn from(m: Moved) -> Self {
        Self::Moved {
            to: Box::new(m.to),
            same_host: m.same_host,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    fn url(text: &str) -> GitUrl {
        GitUrl::parse(text).expect(text)
    }

    #[test]
    fn same_host_moves_are_recognised() {
        let from = url("https://github.com/old/repo");
        for (loc, to) in [
            (
                "https://github.com/new/repo/info/refs?service=git-upload-pack",
                "https://github.com/new/repo",
            ),
            (
                "/new/repo.git/info/refs?service=git-upload-pack",
                "https://github.com/new/repo.git",
            ),
            (
                "https://GitHub.com/new/repo/info/refs?service=git-upload-pack",
                "https://GitHub.com/new/repo",
            ),
        ] {
            let m = moved_to(&from, 301, Some(loc)).unwrap_or_else(|| panic!("{loc}"));
            assert_eq!(m.to.as_str(), to, "{loc}");
            assert!(m.same_host, "{loc}");
        }
        for status in [302, 303, 307, 308] {
            assert!(
                moved_to(
                    &from,
                    status,
                    Some("/b/c/info/refs?service=git-upload-pack")
                )
                .is_some()
            );
        }
    }

    #[test]
    fn other_hosts_and_ports_are_reported_as_different() {
        let from = url("https://git.example.com/o/r");
        for loc in [
            "https://evil.example.net/o/r/info/refs?service=git-upload-pack",
            "https://git.example.com:8443/o/r/info/refs?service=git-upload-pack",
            "https://example.com/o/r/info/refs?service=git-upload-pack",
        ] {
            let m = moved_to(&from, 301, Some(loc)).unwrap_or_else(|| panic!("{loc}"));
            assert!(!m.same_host, "{loc}");
            assert!(!m.to.private_confirmed(), "{loc}");
        }
        // A move to a private host is shown, but never carries a confirmation.
        let m = moved_to(
            &from,
            301,
            Some("https://192.168.1.5/o/r/info/refs?service=git-upload-pack"),
        )
        .expect("reported");
        assert!(!m.same_host && m.to.is_private_host() && !m.to.private_confirmed());
    }

    #[test]
    fn a_confirmed_private_host_keeps_its_answer_only_on_the_same_host() {
        let from = url("https://gitserver/o/r").confirm_private();
        let m = moved_to(
            &from,
            301,
            Some("/team/r/info/refs?service=git-upload-pack"),
        )
        .expect("moved");
        assert!(m.same_host && m.to.private_confirmed());
    }

    #[test]
    fn unusable_answers_are_ignored() {
        let from = url("https://github.com/o/r");
        for (status, loc) in [
            (
                200,
                Some("https://github.com/n/r/info/refs?service=git-upload-pack"),
            ),
            (
                404,
                Some("https://github.com/n/r/info/refs?service=git-upload-pack"),
            ),
            (301, None),
            (301, Some("")),
            // Plain http, scheme-relative and relative locations.
            (
                301,
                Some("http://github.com/n/r/info/refs?service=git-upload-pack"),
            ),
            (
                301,
                Some("//github.com/n/r/info/refs?service=git-upload-pack"),
            ),
            (301, Some("n/r/info/refs?service=git-upload-pack")),
            // Not the Git endpoint: a login page, a different service, extra query.
            (302, Some("https://github.com/login")),
            (
                302,
                Some("https://github.com/n/r/info/refs?service=git-receive-pack"),
            ),
            (
                302,
                Some("https://github.com/n/r/info/refs?service=git-upload-pack&x=1"),
            ),
            // Credentials, odd characters, option-looking paths.
            (
                301,
                Some("https://user:pw@github.com/n/r/info/refs?service=git-upload-pack"),
            ),
            (
                301,
                Some("https://github.com/n/r%2e/info/refs?service=git-upload-pack"),
            ),
            (
                301,
                Some("https://github.com/-n/r/info/refs?service=git-upload-pack"),
            ),
            (
                301,
                Some("https://169.254.169.254/n/info/refs?service=git-upload-pack"),
            ),
            // Itself.
            (
                301,
                Some("https://github.com/o/r/info/refs?service=git-upload-pack"),
            ),
        ] {
            assert_eq!(moved_to(&from, status, loc), None, "{status} {loc:?}");
        }
        // Only https is probed at all; ssh has no redirects.
        assert_eq!(probe(&url("git@github.com:o/r")), None);
        // A destination the fetch rules refuse is never probed.
        assert_eq!(probe(&url("https://localhost:1/o/r")), None);
    }

    /// A one-shot plain-http server on loopback that answers a redirect and records what it
    /// received. Plain http only because a test can't present a trusted certificate; the app's
    /// probe is https-only.
    fn serve_once(answer: &'static str) -> (SocketAddr, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&seen);
        std::thread::spawn(move || {
            // Accept at most two connections: a followed redirect would be the second.
            for stream in listener.incoming().take(2) {
                let Ok(mut stream) = stream else { break };
                let mut reader = BufReader::new(stream.try_clone().expect("clone"));
                let mut head = String::new();
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    head.push_str(&line);
                }
                log.lock().expect("log").push(head);
                let _ = stream.write_all(answer.as_bytes());
            }
        });
        (addr, seen)
    }

    const MOVED: &str = "HTTP/1.1 301 Moved Permanently\r\nLocation: /new/r/info/refs?service=git-upload-pack\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

    fn pinned(host: &str, port: u16, addrs: &[SocketAddr]) -> PinnedResolver {
        PinnedResolver::new(host, port, addrs).expect("addresses")
    }

    fn requests(seen: &Arc<Mutex<Vec<String>>>) -> Vec<String> {
        std::thread::sleep(Duration::from_millis(200));
        seen.lock().expect("log").clone()
    }

    #[test]
    fn the_probe_connects_to_the_checked_address_not_a_second_lookup() {
        // `.invalid` never resolves, so reaching the server proves the pinned address was used.
        let (addr, seen) = serve_once(MOVED);
        let port = addr.port();
        let agent = agent(false, pinned("probe-pin.invalid", port, &[addr]));
        let answer = ask(
            &agent,
            &format!("http://probe-pin.invalid:{port}/o/r{ENDPOINT}"),
        );
        assert_eq!(answer.map(|a| a.0), Some(301));
        assert_eq!(requests(&seen).len(), 1);
    }

    #[test]
    fn the_probe_never_reaches_an_address_that_was_not_checked() {
        let (addr, seen) = serve_once(MOVED);
        let port = addr.port();
        // `localhost` would resolve to the listening server. The check approved a different
        // (closed) address, and only that one is tried.
        let closed: SocketAddr = "127.0.0.1:1".parse().expect("addr");
        let agent1 = agent(false, pinned("localhost", port, &[closed]));
        assert_eq!(
            ask(&agent1, &format!("http://localhost:{port}/o/r{ENDPOINT}")),
            None
        );
        // A name the check never looked at is not resolved at all.
        let agent2 = agent(false, pinned("other.example.com", port, &[closed]));
        assert_eq!(
            ask(&agent2, &format!("http://localhost:{port}/o/r{ENDPOINT}")),
            None
        );
        assert!(requests(&seen).is_empty(), "nothing reached the server");
        // There is never a resolver without checked addresses.
        assert!(PinnedResolver::new("x.example.com", 443, &[]).is_none());
    }

    #[test]
    fn a_name_that_resolves_to_an_unconfirmed_private_address_is_never_probed() {
        static SERVER: std::sync::OnceLock<SocketAddr> = std::sync::OnceLock::new();
        let (addr, seen) = serve_once(MOVED);
        SERVER.set(addr).expect("once");
        fn rebinds_to_server(_: &str, _: u16) -> std::io::Result<Vec<SocketAddr>> {
            Ok(vec![*SERVER.get().expect("set")])
        }
        fn unresolvable(_: &str, _: u16) -> std::io::Result<Vec<SocketAddr>> {
            Err(std::io::ErrorKind::NotFound.into())
        }
        let url = url(&format!("https://rebind.example.com:{}/o/r", addr.port()));
        assert_eq!(probe_with(&url, rebinds_to_server), None);
        // A name that doesn't resolve has nothing checked to connect to.
        assert_eq!(probe_with(&url, unresolvable), None);
        assert!(requests(&seen).is_empty(), "nothing reached the server");
    }

    #[test]
    fn the_probe_reads_location_without_following_or_sending_credentials() {
        let (addr, seen) = serve_once(MOVED);
        let agent = agent(false, pinned("127.0.0.1", addr.port(), &[addr]));
        let answer = ask(&agent, &format!("http://{addr}/old/r{ENDPOINT}")).expect("answer");
        assert_eq!(answer.0, 301);
        assert_eq!(
            answer.1.as_deref(),
            Some("/new/r/info/refs?service=git-upload-pack")
        );
        std::thread::sleep(Duration::from_millis(200));
        let requests = seen.lock().expect("log").clone();
        assert_eq!(requests.len(), 1, "the redirect was not followed");
        let head = requests[0].to_ascii_lowercase();
        assert!(head.starts_with("get /old/r/info/refs?service=git-upload-pack "));
        assert!(!head.contains("authorization"), "{head}");
        assert!(!head.contains("cookie"), "{head}");
    }
}
