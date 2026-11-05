//! Lullshift agent core: the normalized agent event contract (`reference/AGENT_EVENT_V1.schema.json`),
//! session state and the ADR-008 reducer. Persistence is the `local-store` crate.

pub mod event;
pub mod reducer;
pub mod session;
pub mod text;

pub use event::{AgentEvent, EventError, EventType, parse_event};
pub use reducer::{SessionRecord, SessionView, ViewReason, apply, view};
pub use session::SessionState;
