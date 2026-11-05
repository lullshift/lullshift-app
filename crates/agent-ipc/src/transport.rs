//! Platform endpoints (ADR-009): a Windows named pipe or a Unix domain socket, plus the
//! `endpoint.json` discovery file and a minimal client used by forwarders and tests.

use std::fs;
use std::io;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;

use crate::protocol::{Config, EventSink, Handler, PROTOCOL_VERSION, Reply, StatsSnapshot};
use crate::token::{Token, create_private_dir};

pub const IPC_DIR: &str = "ipc";
pub const ENDPOINT_FILE: &str = "endpoint.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportKind {
    WindowsNamedPipe,
    UnixSocket,
}

/// Contents of `endpoint.json`. Contains no secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Endpoint {
    pub protocol: u64,
    pub transport: TransportKind,
    pub address: String,
}

impl Endpoint {
    /// A fresh endpoint for one app launch. On Windows the pipe name carries a random
    /// per-launch nonce: named pipes are machine-global, so a predictable name would let another
    /// local user squat it and receive the token from forwarders. Only `endpoint.json`, in the
    /// user's own data directory, reveals the name.
    pub fn for_launch(data_dir: &Path) -> io::Result<Self> {
        #[cfg(windows)]
        {
            let _ = data_dir;
            let mut nonce = [0u8; 16];
            getrandom::fill(&mut nonce).map_err(|_| io::Error::other("no OS randomness"))?;
            let hex: String = nonce.iter().map(|b| format!("{b:02x}")).collect();
            Ok(Self {
                protocol: PROTOCOL_VERSION,
                transport: TransportKind::WindowsNamedPipe,
                address: format!(r"\\.\pipe\lullshift-ipc-v1-{hex}"),
            })
        }
        #[cfg(not(windows))]
        {
            // The socket lives in the user's 0700 directory, so its path needn't be secret.
            Ok(Self {
                protocol: PROTOCOL_VERSION,
                transport: TransportKind::UnixSocket,
                address: data_dir
                    .join(IPC_DIR)
                    .join("ipc.sock")
                    .to_string_lossy()
                    .into_owned(),
            })
        }
    }

    pub fn read(data_dir: &Path) -> io::Result<Self> {
        let text = fs::read_to_string(data_dir.join(IPC_DIR).join(ENDPOINT_FILE))?;
        serde_json::from_str(&text)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "endpoint.json"))
    }

    fn write(&self, data_dir: &Path) -> io::Result<()> {
        let dir = data_dir.join(IPC_DIR);
        let tmp = dir.join(format!("{ENDPOINT_FILE}.tmp"));
        fs::write(
            &tmp,
            serde_json::to_vec_pretty(self).map_err(io::Error::other)?,
        )?;
        fs::rename(tmp, dir.join(ENDPOINT_FILE))
    }
}

#[derive(Debug)]
pub enum ServeError {
    /// Another process already owns the endpoint (second instance or squatting).
    AddressInUse,
    Io(io::Error),
}

impl std::fmt::Display for ServeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AddressInUse => {
                f.write_str("the IPC endpoint is already in use by another process")
            }
            Self::Io(e) => write!(f, "IPC endpoint error: {e}"),
        }
    }
}

impl std::error::Error for ServeError {}

impl From<io::Error> for ServeError {
    fn from(e: io::Error) -> Self {
        if e.kind() == io::ErrorKind::AddrInUse || e.kind() == io::ErrorKind::PermissionDenied {
            Self::AddressInUse
        } else {
            Self::Io(e)
        }
    }
}

/// A running endpoint. Dropping it stops accepting connections.
pub struct Running {
    pub endpoint: Endpoint,
    handler: Arc<Handler>,
    task: JoinHandle<()>,
    accepting: Arc<AtomicBool>,
}

impl Running {
    pub fn stats(&self) -> StatsSnapshot {
        self.handler.stats.snapshot()
    }

    /// False while the endpoint can't accept connections (accept loop failing or ended), so the
    /// UI can say agent status is unknown instead of implying it is live.
    pub fn is_accepting(&self) -> bool {
        !self.task.is_finished() && self.accepting.load(Ordering::Relaxed)
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Loads or creates the token, binds the endpoint, writes `endpoint.json` and starts accepting.
/// Must be called inside a Tokio runtime.
pub fn serve(
    data_dir: &Path,
    sink: Arc<dyn EventSink>,
    cfg: Config,
) -> Result<Running, ServeError> {
    let ipc_dir = data_dir.join(IPC_DIR);
    create_private_dir(&ipc_dir)?;
    let token = Token::load_or_create(&ipc_dir)?;
    let endpoint = Endpoint::for_launch(data_dir)?;
    let limit = Arc::new(Semaphore::new(cfg.max_connections));
    let handler = Arc::new(Handler::new(token, sink, cfg));
    let accepting = Arc::new(AtomicBool::new(true));
    let task = platform::start(
        &endpoint,
        Arc::clone(&handler),
        limit,
        Arc::clone(&accepting),
    )?;
    endpoint.write(data_dir)?;
    Ok(Running {
        endpoint,
        handler,
        task,
        accepting,
    })
}

/// Retry delay after an accept-side failure: 50 ms doubling to 5 s.
async fn backoff(failures: &mut u32, accepting: &AtomicBool) {
    accepting.store(false, Ordering::Relaxed);
    let ms = 50u64.saturating_mul(1 << (*failures).min(7)).min(5_000);
    *failures = failures.saturating_add(1);
    tokio::time::sleep(Duration::from_millis(ms)).await;
}

fn dispatch<S>(handler: &Arc<Handler>, limit: &Arc<Semaphore>, stream: S)
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let handler = Arc::clone(handler);
    match Arc::clone(limit).try_acquire_owned() {
        Ok(permit) => {
            tokio::spawn(async move {
                handler.serve_connection(stream).await;
                drop(permit);
            });
        }
        Err(_) => {
            tokio::spawn(async move { handler.reject_busy(stream).await });
        }
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use tokio::net::windows::named_pipe::ServerOptions;

    pub(super) fn start(
        endpoint: &Endpoint,
        handler: Arc<Handler>,
        limit: Arc<Semaphore>,
        accepting: Arc<AtomicBool>,
    ) -> Result<JoinHandle<()>, ServeError> {
        let name = endpoint.address.clone();
        // first_pipe_instance: fail if anyone else already created this name (squatting).
        let first = ServerOptions::new()
            .first_pipe_instance(true)
            .reject_remote_clients(true)
            .create(&name)
            .map_err(|e| {
                if e.kind() == io::ErrorKind::PermissionDenied {
                    ServeError::AddressInUse
                } else {
                    ServeError::from(e)
                }
            })?;
        let create = move || {
            ServerOptions::new()
                .reject_remote_clients(true)
                .create(&name)
        };
        Ok(tokio::spawn(async move {
            let mut server = first;
            let mut failures = 0u32;
            loop {
                let connected = server.connect().await;
                // Always have the next instance listening before handing this one off. On
                // failure, retry with backoff (reported via `accepting`) instead of exiting.
                let next = loop {
                    match create() {
                        Ok(s) => break s,
                        Err(_) => backoff(&mut failures, &accepting).await,
                    }
                };
                failures = 0;
                accepting.store(true, Ordering::Relaxed);
                let current = std::mem::replace(&mut server, next);
                if connected.is_ok() {
                    dispatch(&handler, &limit, current);
                }
            }
        }))
    }
}

#[cfg(unix)]
mod platform {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use tokio::net::UnixListener;

    pub(super) fn start(
        endpoint: &Endpoint,
        handler: Arc<Handler>,
        limit: Arc<Semaphore>,
        accepting: Arc<AtomicBool>,
    ) -> Result<JoinHandle<()>, ServeError> {
        let path = PathBuf::from(&endpoint.address);
        if path.exists() {
            // Remove a stale socket only if nobody is listening on it.
            if std::os::unix::net::UnixStream::connect(&path).is_ok() {
                return Err(ServeError::AddressInUse);
            }
            fs::remove_file(&path)?;
        }
        let listener = UnixListener::bind(&path)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        Ok(tokio::spawn(async move {
            let mut failures = 0u32;
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        failures = 0;
                        accepting.store(true, Ordering::Relaxed);
                        dispatch(&handler, &limit, stream);
                    }
                    Err(_) => backoff(&mut failures, &accepting).await,
                }
            }
        }))
    }
}

#[derive(Debug)]
pub enum ClientError {
    /// The app isn't running or the endpoint file is missing.
    NotRunning,
    Io(io::Error),
    BadReply,
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotRunning => f.write_str("Lullshift is not running"),
            Self::Io(e) => write!(f, "IPC error: {e}"),
            Self::BadReply => f.write_str("unexpected reply"),
        }
    }
}

impl std::error::Error for ClientError {}

/// Sends one raw event JSON to the app at `data_dir`, failing fast within `deadline`.
pub async fn send_event(
    data_dir: &Path,
    event_json: &str,
    deadline: Duration,
) -> Result<Reply, ClientError> {
    let ipc_dir = data_dir.join(IPC_DIR);
    let endpoint = Endpoint::read(data_dir).map_err(|_| ClientError::NotRunning)?;
    let token = fs::read_to_string(ipc_dir.join(crate::token::TOKEN_FILE))
        .map_err(|_| ClientError::NotRunning)?;
    let event: &serde_json::value::RawValue = serde_json::from_str(event_json).map_err(|_| {
        ClientError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            "event is not JSON",
        ))
    })?;
    let mut line = serde_json::to_vec(&serde_json::json!({
        "v": PROTOCOL_VERSION,
        "token": token.trim(),
        "event": event,
    }))
    .map_err(|_| ClientError::BadReply)?;
    line.push(b'\n');
    tokio::time::timeout(deadline, exchange(&endpoint, &line))
        .await
        .map_err(|_| ClientError::Io(io::ErrorKind::TimedOut.into()))?
}

async fn exchange(endpoint: &Endpoint, line: &[u8]) -> Result<Reply, ClientError> {
    let not_running = |e: io::Error| match e.kind() {
        io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused => ClientError::NotRunning,
        _ => ClientError::Io(e),
    };
    #[cfg(windows)]
    let mut stream = {
        use tokio::net::windows::named_pipe::ClientOptions;
        const ERROR_PIPE_BUSY: i32 = 231;
        loop {
            match ClientOptions::new().open(&endpoint.address) {
                Ok(c) => break c,
                Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY) => {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                Err(e) => return Err(not_running(e)),
            }
        }
    };
    #[cfg(unix)]
    let mut stream = tokio::net::UnixStream::connect(&endpoint.address)
        .await
        .map_err(not_running)?;

    stream.write_all(line).await.map_err(ClientError::Io)?;
    let mut reply = String::new();
    BufReader::new(stream)
        .take(4096)
        .read_line(&mut reply)
        .await
        .map_err(ClientError::Io)?;
    serde_json::from_str(&reply).map_err(|_| ClientError::BadReply)
}

use tokio::io::AsyncReadExt as _;
