//! Lullshift agent adapters (ADR-010): provider hook payloads → normalized agent events, the
//! hook forwarder that sends them to the app, and consent-based hook installation.

pub mod claude_code;
pub mod forward;
pub mod redact;
