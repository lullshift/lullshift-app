//! Claude Code adapter (docs/adapters/CLAUDE_CODE.md, ADR-010).

pub mod install;
pub mod map;
pub mod title;

pub use map::{CAPABILITIES, HOOKS, Mapped, PROVIDER, map_context, map_hook, transcript_ref};

/// Adapter id used in hook arguments and install records.
pub const ADAPTER_ID: &str = "claude-code";
