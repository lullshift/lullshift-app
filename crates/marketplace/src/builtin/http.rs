//! The built-in Git's only network path: gix's smart-HTTP protocol over Lullshift's own HTTP
//! client (decision 28, ADR-023).
//!
//! gix-transport's `Http` trait lets the caller supply the HTTP layer. This one is ureq (the
//! same client and TLS setup as the redirect probe), configured so that:
//!
//! - it connects only to the addresses `net::check_destination` approved
//!   ([`PinnedResolver`]): no second DNS lookup, no other host;
//! - it never uses a proxy (not even one from the environment) and never follows a redirect;
//!   a redirect is recorded so the caller can report it as `moved` (decision 18.1);
//! - TLS is verified with the platform verifier; gix's `configure` call (which would carry
//!   `http.sslVerify`, proxies, extra headers and so on from Git configuration) is ignored;
//! - it never sends credentials. A 401, 403 or 407 is recorded and ends the fetch; the caller
//!   reports "needs Git installed" (private marketplaces need the user's own Git sign-in);
//! - it counts every response byte against the fetch cap, and stops at the fetch deadline.
//!
//! Requests other than `https://` can't be made, except by the test-only loopback switch, which
//! sends the same request over plain http to a local test server.

use std::io::{BufRead, BufReader, Cursor, Read, Write};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gix::error::ErrorExt as _;
use gix_transport::client::blocking_io::http::{GetResponse, Http, PostBodyDataKind, PostResponse};
use ureq::unversioned::transport::DefaultConnector;

use crate::redirect::PinnedResolver;

/// Most a request body (the client's wants and haves) may be. A shallow fetch sends a few lines.
const POST_MAX: usize = 4 * 1024 * 1024;
/// A `Location` longer than this is ignored, as in the probe.
const LOCATION_MAX: usize = 2048;

/// What the server said that the caller needs to classify a failed fetch. The raw text of gix
/// errors is never shown; this is how the honest category is found.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Seen {
    /// A 3xx with this status and `Location` (not followed).
    pub redirect: Option<(u16, Option<String>)>,
    /// 401, 403 or 407: the repository wants a sign-in.
    pub auth: bool,
    /// 404 or 410.
    pub not_found: bool,
    /// Could not connect, or the connection broke.
    pub offline: bool,
    /// The deadline passed.
    pub timeout: bool,
    /// More bytes arrived than the fetch cap allows.
    pub too_large: bool,
    /// Any other non-success status.
    pub other_status: bool,
    /// How many requests reached the point of being sent. Tests only use it.
    pub requests: u32,
}

/// Limits shared by every request of one fetch.
#[derive(Debug)]
pub(crate) struct Budget {
    pub deadline: Instant,
    pub cap: u64,
    received: AtomicU64,
    /// Set when the deadline passes or the cap is exceeded, so gix stops pack processing too.
    pub interrupt: AtomicBool,
    seen: Mutex<Seen>,
}

impl Budget {
    pub(crate) fn new(timeout: Duration, cap: u64) -> Arc<Self> {
        Arc::new(Self {
            deadline: Instant::now() + timeout,
            cap,
            received: AtomicU64::new(0),
            interrupt: AtomicBool::new(false),
            seen: Mutex::new(Seen::default()),
        })
    }

    pub(crate) fn seen(&self) -> Seen {
        self.seen.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn note(&self, f: impl FnOnce(&mut Seen)) {
        f(&mut self.seen.lock().unwrap_or_else(|e| e.into_inner()));
    }

    fn remaining(&self) -> Option<Duration> {
        self.deadline.checked_duration_since(Instant::now())
    }

    pub(crate) fn time_out(&self) {
        self.note(|s| s.timeout = true);
        self.interrupt.store(true, Ordering::Relaxed);
    }
}

/// The HTTP layer handed to gix. One per fetch.
#[derive(Clone)]
pub(crate) struct PinnedHttp {
    agent: ureq::Agent,
    budget: Arc<Budget>,
    /// Tests only: send to `http://` instead of `https://` (a loopback test server).
    plain_http: bool,
}

impl PinnedHttp {
    /// `None` without any checked address: there is nothing allowed to connect to.
    pub(crate) fn new(
        host: &str,
        port: u16,
        addrs: &[SocketAddr],
        budget: Arc<Budget>,
        plain_http: bool,
    ) -> Option<Self> {
        let resolver = PinnedResolver::new(host, port, addrs)?;
        Some(Self {
            agent: agent(resolver, plain_http),
            budget,
            plain_http,
        })
    }

    fn target(&self, url: &str) -> std::io::Result<String> {
        let Some(rest) = url.strip_prefix("https://") else {
            // gix only builds URLs from the https address it was given.
            return Err(std::io::Error::other("only https is allowed"));
        };
        Ok(if self.plain_http {
            format!("http://{rest}")
        } else {
            url.to_owned()
        })
    }

    /// Sends one request and turns anything but a 2xx into an error, recording why.
    fn send(
        &self,
        url: &str,
        headers: &[(String, String)],
        body: Option<&[u8]>,
    ) -> std::io::Result<Exchange> {
        let url = self.target(url)?;

        let Some(remaining) = self.budget.remaining() else {
            self.budget.time_out();
            return Err(std::io::ErrorKind::TimedOut.into());
        };
        self.budget.note(|s| s.requests += 1);
        let result = match body {
            None => {
                let mut req = self.agent.get(&url);
                for (k, v) in headers {
                    req = req.header(k.as_str(), v.as_str());
                }
                req.config().timeout_global(Some(remaining)).build().call()
            }
            Some(bytes) => {
                let mut req = self.agent.post(&url);
                for (k, v) in headers {
                    req = req.header(k.as_str(), v.as_str());
                }
                req.config()
                    .timeout_global(Some(remaining))
                    .build()
                    .send(bytes)
            }
        };
        let resp = match result {
            Ok(r) => r,
            Err(e) => {
                let kind = match e {
                    ureq::Error::Timeout(_) => {
                        self.budget.time_out();
                        std::io::ErrorKind::TimedOut
                    }
                    ureq::Error::HostNotFound
                    | ureq::Error::ConnectionFailed
                    | ureq::Error::Io(_) => {
                        self.budget.note(|s| s.offline = true);
                        std::io::ErrorKind::ConnectionRefused
                    }
                    _ => std::io::ErrorKind::Other,
                };
                return Err(kind.into());
            }
        };
        let status = resp.status().as_u16();
        match status {
            200..=299 => {}
            301 | 302 | 303 | 307 | 308 => {
                let location = resp
                    .headers()
                    .get("location")
                    .and_then(|v| v.to_str().ok())
                    .filter(|v| v.len() <= LOCATION_MAX)
                    .map(str::to_owned);
                self.budget.note(|s| s.redirect = Some((status, location)));
                return Err(std::io::Error::other("redirect not followed"));
            }
            401 | 403 | 407 => {
                self.budget.note(|s| s.auth = true);
                return Err(std::io::ErrorKind::PermissionDenied.into());
            }
            404 | 410 => {
                self.budget.note(|s| s.not_found = true);
                return Err(std::io::ErrorKind::NotFound.into());
            }
            _ => {
                self.budget.note(|s| s.other_status = true);
                return Err(std::io::Error::other("unexpected status"));
            }
        }
        let mut head = String::new();
        for (name, value) in resp.headers() {
            if let Ok(v) = value.to_str() {
                head.push_str(name.as_str());
                head.push_str(": ");
                head.push_str(v);
                head.push('\n');
            }
        }
        // The cap also bounds what one response may hold; the counter below covers the total.
        let left = self
            .budget
            .cap
            .saturating_sub(self.budget.received.load(Ordering::Relaxed))
            .saturating_add(1);
        let reader = resp.into_body().into_with_config().limit(left).reader();
        Ok(Exchange {
            head: head.into_bytes(),
            body: Some(Box::new(Counted {
                inner: reader,
                budget: Arc::clone(&self.budget),
            })),
        })
    }
}

fn agent(resolver: PinnedResolver, plain_http: bool) -> ureq::Agent {
    use ureq::config::Config;
    use ureq::tls::{RootCerts, TlsConfig};
    let config = Config::builder()
        .http_status_as_error(false)
        .max_redirects(0)
        .max_redirects_will_error(false)
        // No proxy, whatever the environment says: a proxy would resolve the name itself and
        // the pin could not apply (ADR-023).
        .proxy(None)
        .https_only(!plain_http)
        .user_agent(concat!(
            "git/2 (Lullshift built-in Git ",
            env!("CARGO_PKG_VERSION"),
            ")"
        ))
        .tls_config(
            TlsConfig::builder()
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        )
        .build();
    ureq::Agent::with_parts(config, DefaultConnector::default(), resolver)
}

/// Parses gix's `"Name: value"` header lines.
fn header_pairs(headers: impl IntoIterator<Item = impl AsRef<str>>) -> Vec<(String, String)> {
    headers
        .into_iter()
        .filter_map(|h| {
            let (k, v) = h.as_ref().split_once(':')?;
            Some((k.trim().to_owned(), v.trim().to_owned()))
        })
        // Credentials are never sent, whatever asks for them.
        .filter(|(k, _)| !k.eq_ignore_ascii_case("authorization"))
        .collect()
}

/// A response body that counts bytes against the cap and stops at the deadline.
struct Counted<R> {
    inner: R,
    budget: Arc<Budget>,
}

impl<R: Read> Read for Counted<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.budget.remaining().is_none() {
            self.budget.time_out();
            return Err(std::io::ErrorKind::TimedOut.into());
        }
        let n = match self.inner.read(buf) {
            Ok(n) => n,
            Err(e) => {
                let over = self.budget.received.load(Ordering::Relaxed) >= self.budget.cap;
                if over {
                    self.budget.note(|s| s.too_large = true);
                    self.budget.interrupt.store(true, Ordering::Relaxed);
                } else if e.kind() == std::io::ErrorKind::TimedOut {
                    self.budget.time_out();
                } else {
                    self.budget.note(|s| s.offline = true);
                }
                return Err(e);
            }
        };
        let total = self
            .budget
            .received
            .fetch_add(n as u64, Ordering::Relaxed)
            .saturating_add(n as u64);
        if total > self.budget.cap {
            self.budget.note(|s| s.too_large = true);
            self.budget.interrupt.store(true, Ordering::Relaxed);
            return Err(std::io::Error::other("fetch cap exceeded"));
        }
        Ok(n)
    }
}

/// A response: its header lines and its body.
struct Exchange {
    head: Vec<u8>,
    body: Option<Box<dyn Read + Send>>,
}

/// A request not sent yet: the client, the URL and the header pairs.
type Deferred = (PinnedHttp, String, Vec<(String, String)>);

/// One request, sent when its response is first read (a POST's body is written first).
struct Pending {
    http: Option<Deferred>,
    post: Vec<u8>,
    too_big: bool,
    result: Option<std::io::Result<Exchange>>,
}

type Shared = Arc<Mutex<Pending>>;

fn ensure_sent(p: &mut Pending) {
    if p.result.is_some() {
        return;
    }
    let Some((http, url, headers)) = p.http.take() else {
        p.result = Some(Err(std::io::Error::other("no request")));
        return;
    };
    if p.too_big {
        p.result = Some(Err(std::io::Error::other("request too large")));
        return;
    }
    let body = std::mem::take(&mut p.post);
    p.result = Some(http.send(&url, &headers, Some(&body)));
}

fn copy_err(e: &std::io::Error) -> std::io::Error {
    std::io::Error::new(e.kind(), "request failed")
}

/// The header lines of a response, as gix reads them.
pub(crate) struct Headers {
    shared: Shared,
    buf: Option<Cursor<Vec<u8>>>,
}

impl Headers {
    fn load(&mut self) -> std::io::Result<&mut Cursor<Vec<u8>>> {
        if self.buf.is_none() {
            let mut p = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            ensure_sent(&mut p);
            match p.result.as_ref() {
                Some(Ok(ex)) => self.buf = Some(Cursor::new(ex.head.clone())),
                Some(Err(e)) => return Err(copy_err(e)),
                None => return Err(std::io::Error::other("no response")),
            }
        }
        Ok(self.buf.as_mut().expect("loaded above"))
    }
}

impl Read for Headers {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.load()?.read(buf)
    }
}

impl BufRead for Headers {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        self.load()?.fill_buf()
    }
    fn consume(&mut self, amt: usize) {
        if let Some(b) = self.buf.as_mut() {
            b.consume(amt);
        }
    }
}

/// A response body, as gix reads it.
pub(crate) struct Body {
    shared: Shared,
    reader: Option<BufReader<Box<dyn Read + Send>>>,
}

impl Body {
    fn load(&mut self) -> std::io::Result<&mut BufReader<Box<dyn Read + Send>>> {
        if self.reader.is_none() {
            let mut p = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            ensure_sent(&mut p);
            let taken = match p.result.as_mut() {
                Some(Ok(ex)) => ex.body.take(),
                Some(Err(e)) => return Err(copy_err(e)),
                None => None,
            };
            let reader: Box<dyn Read + Send> = taken.unwrap_or_else(|| Box::new(std::io::empty()));
            self.reader = Some(BufReader::with_capacity(64 * 1024, reader));
        }
        Ok(self.reader.as_mut().expect("loaded above"))
    }
}

impl Read for Body {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.load()?.read(buf)
    }
}

impl BufRead for Body {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        self.load()?.fill_buf()
    }
    fn consume(&mut self, amt: usize) {
        if let Some(r) = self.reader.as_mut() {
            r.consume(amt);
        }
    }
}

/// Where gix writes a POST body. Kept in memory and sent when the response is first read.
pub(crate) struct PostBody {
    shared: Shared,
}

impl Write for PostBody {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut p = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        if p.post.len() + buf.len() > POST_MAX {
            p.too_big = true;
            return Err(std::io::Error::other("request too large"));
        }
        p.post.extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn gix_err(e: &std::io::Error) -> gix::error::Error {
    std::io::Error::new(e.kind(), "HTTP request failed").raise()
}

impl Http for PinnedHttp {
    type Headers = Headers;
    type ResponseBody = Body;
    type PostBody = PostBody;

    fn get(
        &mut self,
        url: &str,
        _base_url: &str,
        headers: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> gix::error::Result<GetResponse<Self::Headers, Self::ResponseBody>> {
        let ex = self
            .send(url, &header_pairs(headers), None)
            .map_err(|e| gix_err(&e))?;
        let shared = Arc::new(Mutex::new(Pending {
            http: None,
            post: Vec::new(),
            too_big: false,
            result: Some(Ok(ex)),
        }));
        Ok(GetResponse {
            headers: Headers {
                shared: Arc::clone(&shared),
                buf: None,
            },
            body: Body {
                shared,
                reader: None,
            },
        })
    }

    fn post(
        &mut self,
        url: &str,
        _base_url: &str,
        headers: impl IntoIterator<Item = impl AsRef<str>>,
        _body: PostBodyDataKind,
    ) -> gix::error::Result<PostResponse<Self::Headers, Self::ResponseBody, Self::PostBody>> {
        let shared = Arc::new(Mutex::new(Pending {
            http: Some((self.clone(), url.to_owned(), header_pairs(headers))),
            post: Vec::new(),
            too_big: false,
            result: None,
        }));
        Ok(PostResponse {
            post_body: PostBody {
                shared: Arc::clone(&shared),
            },
            headers: Headers {
                shared: Arc::clone(&shared),
                buf: None,
            },
            body: Body {
                shared,
                reader: None,
            },
        })
    }

    /// Ignored on purpose: this is how gix would pass proxies, `http.sslVerify`, extra headers,
    /// cookies and similar settings from Git configuration. None of them apply here.
    fn configure(&mut self, _config: &dyn std::any::Any) -> gix::error::Result {
        Ok(())
    }
}
