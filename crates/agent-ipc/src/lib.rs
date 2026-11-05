//! Lullshift authenticated local IPC (ADR-009). Agent hook forwarders send one normalized agent
//! event per connection over a per-user named pipe (Windows) or Unix socket, authenticated with
//! a per-install token. Requests are size-capped before parsing, time-limited and rate-limited.

pub mod protocol;
pub mod token;
pub mod transport;

pub use protocol::{Config, ErrorCode, EventSink, Reply, SinkError, StatsSnapshot};
pub use token::Token;
pub use transport::{ClientError, Endpoint, Running, ServeError, TransportKind, send_event, serve};
