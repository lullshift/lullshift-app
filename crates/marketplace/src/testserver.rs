//! A loopback smart-HTTP Git server for the built-in Git's tests (ADR-023). Tests only.
//!
//! Each request is answered by `git http-backend` (the CGI program that ships with Git) for a
//! repository registered under a short key, or by a fixed status for the redirect and sign-in
//! tests. It speaks plain http on 127.0.0.1, because a test can't present a trusted
//! certificate; the built-in Git uses plain http only through its test-only switch.
//!
//! URLs use [`HOST`], a name under `.invalid` that never resolves (RFC 6761). Only the test
//! resolver ([`resolve`]) maps it to the server, so a fetch that reaches the server proves the
//! pinned address was used and no second lookup happened.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use crate::validate::GitUrl;

/// The test host name. Never resolves anywhere but in [`resolve`].
pub const HOST: &str = "builtin-git.test.invalid";

#[derive(Clone)]
enum Route {
    Repo(PathBuf),
    Status { code: u16, location: Option<String> },
    Hang(std::time::Duration),
}

/// One request the server received.
#[derive(Debug, Clone)]
pub struct Logged {
    pub key: String,
    pub method: String,
    pub target: String,
    pub authorization: bool,
}

struct Server {
    addr: SocketAddr,
    routes: Mutex<HashMap<String, Route>>,
    log: Mutex<Vec<Logged>>,
    next: AtomicU64,
}

fn server() -> &'static Server {
    static SERVER: OnceLock<Server> = OnceLock::new();
    SERVER.get_or_init(|| {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind the test server");
        let addr = listener.local_addr().expect("address");
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                std::thread::spawn(move || {
                    let _ = handle(stream);
                });
            }
        });
        Server {
            addr,
            routes: Mutex::new(HashMap::new()),
            log: Mutex::new(Vec::new()),
            next: AtomicU64::new(1),
        }
    })
}

/// The test resolver: [`HOST`] is the server; anything else goes to the system resolver.
pub fn resolve(host: &str, port: u16) -> std::io::Result<Vec<SocketAddr>> {
    let s = server();
    if host.eq_ignore_ascii_case(HOST) && port == s.addr.port() {
        return Ok(vec![s.addr]);
    }
    crate::net::system_resolve(host, port)
}

/// The server's port.
pub fn port() -> u16 {
    server().addr.port()
}

fn register(route: Route) -> (String, GitUrl) {
    let s = server();
    let key = format!("k{}", s.next.fetch_add(1, Ordering::Relaxed));
    s.routes.lock().expect("routes").insert(key.clone(), route);
    let url = GitUrl::for_test_https(HOST, s.addr.port(), &format!("/{key}/repo.git"));
    (key, url)
}

/// An https address (served as plain http) for the repository at `dir`.
pub fn repo_url(dir: &Path) -> GitUrl {
    register(Route::Repo(dir.to_path_buf())).1
}

/// An address that always answers `code`, with `location` if given. Returns its key too, for
/// [`requests`].
pub fn status_url(code: u16, location: Option<&str>) -> (String, GitUrl) {
    register(Route::Status {
        code,
        location: location.map(str::to_owned),
    })
}

/// An address whose server waits `wait` before closing without an answer.
pub fn hang_url(wait: std::time::Duration) -> (String, GitUrl) {
    register(Route::Hang(wait))
}

/// The address of the repository at `dir`, plus the key to look its requests up by.
pub fn repo_url_with_key(dir: &Path) -> (String, GitUrl) {
    register(Route::Repo(dir.to_path_buf()))
}

/// Every request received under `key`.
pub fn requests(key: &str) -> Vec<Logged> {
    server()
        .log
        .lock()
        .expect("log")
        .iter()
        .filter(|l| l.key == key)
        .cloned()
        .collect()
}

fn handle(stream: TcpStream) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_owned();
    let target = parts.next().unwrap_or_default().to_owned();
    let mut headers: Vec<(String, String)> = Vec::new();
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h)? == 0 || h == "\r\n" || h == "\n" {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            headers.push((k.trim().to_ascii_lowercase(), v.trim().to_owned()));
        }
    }
    let header = |name: &str| {
        headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
    };
    let length: usize = header("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;

    let (path, query) = target.split_once('?').unwrap_or((target.as_str(), ""));
    let mut segments = path.trim_start_matches('/').splitn(2, '/');
    let key = segments.next().unwrap_or_default().to_owned();
    let rest = segments.next().unwrap_or_default();
    let s = server();
    s.log.lock().expect("log").push(Logged {
        key: key.clone(),
        method: method.clone(),
        target: target.clone(),
        authorization: header("authorization").is_some(),
    });
    let route = s.routes.lock().expect("routes").get(&key).cloned();
    let mut out = stream;
    match route {
        None => respond(&mut out, 404, &[], b""),
        Some(Route::Status { code, location }) => {
            let extra: Vec<(String, String)> = location
                .map(|l| vec![("Location".to_owned(), l)])
                .unwrap_or_default();
            respond(&mut out, code, &extra, b"")
        }
        Some(Route::Hang(wait)) => {
            std::thread::sleep(wait);
            Ok(())
        }
        Some(Route::Repo(dir)) => {
            // `repo.git` in the address stands for the registered folder.
            let rest = rest.strip_prefix("repo.git").unwrap_or(rest);
            let (code, extra, payload) = cgi(&dir, &method, rest, query, &header, &body)?;
            respond(&mut out, code, &extra, &payload)
        }
    }
}

type Answer = (u16, Vec<(String, String)>, Vec<u8>);

fn cgi(
    dir: &Path,
    method: &str,
    rest: &str,
    query: &str,
    header: &dyn Fn(&str) -> Option<String>,
    body: &[u8],
) -> std::io::Result<Answer> {
    let root = dir.parent().expect("a parent folder");
    let name = dir.file_name().expect("a folder name").to_string_lossy();
    let mut cmd = Command::new("git");
    cmd.arg("http-backend")
        .env("GIT_PROJECT_ROOT", root)
        .env("GIT_HTTP_EXPORT_ALL", "1")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("PATH_INFO", format!("/{name}{rest}"))
        .env("QUERY_STRING", query)
        .env("REQUEST_METHOD", method)
        .env("REMOTE_ADDR", "127.0.0.1")
        .env("CONTENT_LENGTH", body.len().to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let gitconfig = dir.with_extension("gitconfig");
    if gitconfig.is_file() {
        cmd.env("GIT_CONFIG_GLOBAL", gitconfig);
    }
    if let Some(ct) = header("content-type") {
        cmd.env("CONTENT_TYPE", ct);
    }
    if let Some(proto) = header("git-protocol") {
        cmd.env("GIT_PROTOCOL", proto);
    }
    let mut child = cmd.spawn()?;
    let mut stdin = child.stdin.take().expect("stdin");
    let input = body.to_vec();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&input);
    });
    let output = child.wait_with_output()?;
    let _ = writer.join();
    let raw = output.stdout;
    let split = find(&raw, b"\r\n\r\n")
        .map(|i| (i, 4))
        .or_else(|| find(&raw, b"\n\n").map(|i| (i, 2)));
    let Some((at, sep)) = split else {
        return Ok((500, Vec::new(), Vec::new()));
    };
    let head = String::from_utf8_lossy(&raw[..at]).into_owned();
    let mut code = 200;
    let mut extra = Vec::new();
    for line in head.lines() {
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        if k.eq_ignore_ascii_case("status") {
            code = v
                .split_whitespace()
                .next()
                .and_then(|c| c.parse().ok())
                .unwrap_or(500);
        } else {
            extra.push((k.trim().to_owned(), v.trim().to_owned()));
        }
    }
    Ok((code, extra, raw[at + sep..].to_vec()))
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn respond(
    out: &mut TcpStream,
    code: u16,
    headers: &[(String, String)],
    body: &[u8],
) -> std::io::Result<()> {
    let mut head = format!("HTTP/1.1 {code} X\r\n");
    for (k, v) in headers {
        if k.eq_ignore_ascii_case("content-length") || k.eq_ignore_ascii_case("connection") {
            continue;
        }
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str(&format!(
        "Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    ));
    out.write_all(head.as_bytes())?;
    out.write_all(body)?;
    out.flush()
}
