//! Codex CLI adapter (docs/ADAPTER_CAPABILITIES.md, ADR-021). Observe-only: user-level
//! `hooks.json` hooks, registered async, writing nothing back.

pub mod install;
pub mod map;

pub use map::{CAPABILITIES, HOOKS, Mapped, PROVIDER, map_hook};

/// Adapter id used in hook arguments and install records.
pub const ADAPTER_ID: &str = "codex";
