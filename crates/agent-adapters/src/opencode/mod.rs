//! OpenCode adapter (EXPERIMENTAL, OpenCode 2.x only; ADR-022). Observe-only: one plugin file
//! that Lullshift fully owns, in OpenCode's user-level plugin directory.

pub mod install;
pub mod map;

pub use map::{CAPABILITIES, EVENTS, Mapped, PROVIDER, map_context, map_hook};

/// Adapter id used in hook arguments and install records.
pub const ADAPTER_ID: &str = "opencode";
