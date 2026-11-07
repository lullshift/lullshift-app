//! Errors the UI may see. Every message is a fixed sentence (plus a number where noted).

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    Offline,
    Timeout,
    RateLimited,
    KeyRejected,
    NoKey,
    KeychainUnavailable,
    BudgetReached,
    ProviderError,
    InvalidResponse,
    InvalidInput,
    NotFound,
    Cancelled,
    Busy,
    Internal,
}

impl ErrorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Offline => "offline",
            Self::Timeout => "timeout",
            Self::RateLimited => "rateLimited",
            Self::KeyRejected => "keyRejected",
            Self::NoKey => "noKey",
            Self::KeychainUnavailable => "keychainUnavailable",
            Self::BudgetReached => "budgetReached",
            Self::ProviderError => "providerError",
            Self::InvalidResponse => "invalidResponse",
            Self::InvalidInput => "invalidInput",
            Self::NotFound => "notFound",
            Self::Cancelled => "cancelled",
            Self::Busy => "busy",
            Self::Internal => "internal",
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct CompanionError {
    pub kind: ErrorKind,
    pub message: String,
}

impl CompanionError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn offline() -> Self {
        Self::new(
            ErrorKind::Offline,
            "Can't reach the provider securely. Check your network connection and the provider address.",
        )
    }

    pub fn timeout() -> Self {
        Self::new(
            ErrorKind::Timeout,
            "The provider took too long to answer and the request was stopped.",
        )
    }

    pub fn key_rejected() -> Self {
        Self::new(
            ErrorKind::KeyRejected,
            "The provider rejected the key. Check that it is correct, active and allowed to use this model.",
        )
    }

    pub fn no_key() -> Self {
        Self::new(
            ErrorKind::NoKey,
            "No key is saved for this provider. Add one in Companion settings.",
        )
    }

    pub fn keychain_unavailable() -> Self {
        Self::new(
            ErrorKind::KeychainUnavailable,
            "The system keychain isn't available, so Lullshift can't read or save keys. Keys are never stored anywhere else.",
        )
    }

    pub fn budget_reached() -> Self {
        Self::new(
            ErrorKind::BudgetReached,
            "This profile's monthly token budget is used up. Raise the limit or wait until next month (UTC).",
        )
    }

    pub fn invalid_response() -> Self {
        Self::new(
            ErrorKind::InvalidResponse,
            "The provider's answer wasn't in the expected format.",
        )
    }

    pub fn cancelled() -> Self {
        Self::new(
            ErrorKind::Cancelled,
            "Cancelled. The request may already have reached the provider, so it may still count against your usage.",
        )
    }

    pub fn busy() -> Self {
        Self::new(
            ErrorKind::Busy,
            "A message is already being sent in this conversation.",
        )
    }

    pub fn internal() -> Self {
        Self::new(
            ErrorKind::Internal,
            "Something went wrong inside Lullshift.",
        )
    }

    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::InvalidInput, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::NotFound, message)
    }

    /// Removes `key` from the message (defence in depth; messages carry only sanitised, scrubbed provider excerpts).
    #[must_use]
    pub fn scrubbed(mut self, key: &str) -> Self {
        self.message = crate::scrub(&self.message, key);
        self
    }
}

impl fmt::Debug for CompanionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind.as_str(), self.message)
    }
}

impl fmt::Display for CompanionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CompanionError {}
