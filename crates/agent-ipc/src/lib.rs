//! Lullshift authenticated local IPC (ADR-009). Agent hook forwarders send one normalized agent
//! event per connection over a per-user named pipe (Windows) or Unix socket, authenticated with
//! a per-install token. Requests are size-capped before parsing, time-limited and rate-limited.

pub mod protocol;
pub mod token;
pub mod transport;

pub use protocol::{
    Config, ErrorCode, EventContext, EventSink, MAX_ASK_CHARS, MAX_INTENT_CHARS, MAX_TITLE_CHARS,
    ProcessRef, Reply, SinkError, StatsSnapshot, WithProcess,
};
pub use token::Token;
pub use transport::{
    ClientError, Endpoint, Running, ServeError, TransportKind, request_line,
    request_line_with_process, send_event, send_event_full, send_event_with_context, serve,
};
