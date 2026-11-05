//! Lullshift agent core: the normalized agent event contract (`reference/AGENT_EVENT_V1.schema.json`),
//! session state, and the Sprint 00 mock feed. The reducer and persistence arrive in Sprint 01.

pub mod event;
pub mod mock;
pub mod session;

pub use event::{AgentEvent, EventError, EventType, parse_event};
pub use session::{SessionSnapshot, SessionState};
