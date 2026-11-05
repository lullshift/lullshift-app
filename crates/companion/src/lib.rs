//! Lullshift Companion core (Sprint 09, ADR-020): provider requests, keychain access, budget and
//! context scopes. No UI, no database; the desktop crate wires this to the store and to Tauri.
//!
//! Rules enforced here:
//! - A key is a [`Secret`]: redacted `Debug`/`Display`, zeroised on drop, never serialised.
//! - Keys live only in the OS keychain ([`keystore`]). If it is unavailable there is no fallback.
//! - Requests carry no tools, functions or tool choice, and one fixed neutral system sentence.
//! - Errors are fixed strings. Provider bodies, headers and URLs are never put in them.
//! - Nothing in this crate logs.

pub mod budget;
pub mod error;
pub mod http;
pub mod keystore;
pub mod provider;
pub mod scope;
pub mod scrub;
pub mod secret;
pub mod url_check;

pub use error::{CompanionError, ErrorKind};
pub use http::{CancelToken, HttpConfig, LateUsage, chat, chat_tracked};
#[cfg(any(test, feature = "test-support"))]
pub use keystore::MemoryKeyStore;
pub use keystore::{KEYCHAIN_SERVICE, KeyStore, KeyStoreError, KeyringStore};
pub use provider::{ChatMessage, ProviderKind, ProviderSpec, Reply, Role, SYSTEM_PROMPT};
pub use scrub::scrub;
pub use secret::Secret;
