//! Lullshift Companion core (Sprint 09, ADR-020): provider requests, keychain access, budget and
//! context scopes. No UI, no database; the desktop crate wires this to the store and to Tauri.
//!
//! Rules enforced here:
//! - A key is a [`Secret`]: redacted `Debug`/`Display`, zeroised on drop, never serialised.
//! - Keys live only in the OS keychain ([`keystore`]). If it is unavailable there is no fallback.
//! - Requests carry tools only when the user turned lookups on (Sprint 12), and only the three
//!   read-only lookups in [`tools`]; one of two fixed neutral system texts.
//! - Errors are fixed sentences. A refused request may add one sanitised excerpt of the provider's
//!   own standard JSON error message ([`reason`]); raw bodies, headers and URLs are never put in them.
//! - The models.dev model list ([`models_dev`]) is fetched only on request and kept in memory.
//! - Nothing in this crate logs.

// `CompanionError` carries a provider reason and a usage figure beside its fixed message, and it
// is returned by value throughout; the errors are rare and boxing them would only add noise.
#![allow(clippy::result_large_err)]

pub mod budget;
pub mod chat_stream;
pub mod error;
pub mod http;
pub mod keystore;
pub mod models_dev;
pub mod provider;
pub mod reason;
pub mod scope;
pub mod scrub;
pub mod secret;
pub mod sse;
pub mod stream;
pub mod tools;
pub mod url_check;

pub use chat_stream::{Learned, MAX_THINKING_BYTES};
pub use chat_stream::{StreamFailure, StreamReply, chat_stream};
pub use error::ProviderReason;
pub use error::{CompanionError, ErrorKind};
pub use http::{CancelToken, HttpConfig, LateUsage, chat, chat_tracked};
#[cfg(any(test, feature = "test-support"))]
pub use keystore::MemoryKeyStore;
pub use keystore::{KEYCHAIN_SERVICE, KeyStore, KeyStoreError, KeyringStore};
pub use provider::{
    ChatMessage, ProviderKind, ProviderSpec, Reply, RequestPlan, Role, SYSTEM_PROMPT,
    SYSTEM_PROMPT_WITH_TOOLS, ToolExchange,
};
pub use scrub::scrub;
pub use secret::Secret;
