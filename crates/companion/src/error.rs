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

/// What the provider itself said about a refused request: one sanitised, capped sentence (see
/// [`crate::reason`]). Kept apart from [`CompanionError::message`], which is always Lullshift's
/// own fixed sentence, so the UI can show it as the provider's words, in its own element, and
/// never mistake it for Lullshift's wording or run it through link detection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderReason {
    /// "OpenAI", "Anthropic" or "The provider".
    pub speaker: &'static str,
    pub text: String,
}

#[derive(Clone, PartialEq, Eq)]
pub struct CompanionError {
    pub kind: ErrorKind,
    /// A fixed sentence. Never provider text.
    pub message: String,
    /// The provider's sanitised reason, if it gave a readable one.
    pub reason: Option<ProviderReason>,
    /// Tokens (input, output) to book for a call that failed after the provider began work on it
    /// (a reported failure with its usage, or the output cap when none was reported). Not shown.
    pub usage: Option<(i64, i64)>,
}

impl CompanionError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            reason: None,
            usage: None,
        }
    }

    #[must_use]
    pub fn with_reason(mut self, reason: Option<ProviderReason>) -> Self {
        self.reason = reason;
        self
    }

    #[must_use]
    pub fn with_usage(mut self, input: i64, output: i64) -> Self {
        self.usage = Some((input, output));
        self
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

    /// Removes `key` from the message and the provider's reason (defence in depth: the reason
    /// was already scrubbed when it was made).
    #[must_use]
    pub fn scrubbed(mut self, key: &str) -> Self {
        self.message = crate::scrub(&self.message, key);
        if let Some(r) = &mut self.reason {
            r.text = crate::scrub(&r.text, key);
        }
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
