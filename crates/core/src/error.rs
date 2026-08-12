use thiserror::Error;

use crate::ProcessingJobErrorCategory;

/// Typed provider failures.
///
/// Every variant carries a short, already-redacted message. Providers must not
/// place credentials, authorization headers, request bodies, or raw upstream
/// response bodies into these messages; interfaces render the category and the
/// short message only.
#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("{provider} provider failed: {message}")]
    Failure {
        provider: &'static str,
        message: String,
    },
    #[error("{provider} credential was rejected: {message}")]
    Credential {
        provider: &'static str,
        message: String,
    },
    #[error("{provider} quota or rate limit was exhausted: {message}")]
    Quota {
        provider: &'static str,
        message: String,
    },
    #[error("{provider} could not be reached: {message}")]
    Network {
        provider: &'static str,
        message: String,
    },
    #[error("{provider} could not use the supplied audio: {message}")]
    Audio {
        provider: &'static str,
        message: String,
    },
    #[error("{provider} is not configured correctly: {message}")]
    Configuration {
        provider: &'static str,
        message: String,
    },
    #[error("{provider} returned an unusable response: {message}")]
    InvalidResponse {
        provider: &'static str,
        message: String,
    },
    #[error("{provider} operation was cancelled")]
    Canceled { provider: &'static str },
}

impl ProviderError {
    pub fn failure(provider: &'static str, message: impl Into<String>) -> Self {
        Self::Failure {
            provider,
            message: message.into(),
        }
    }

    pub fn credential(provider: &'static str, message: impl Into<String>) -> Self {
        Self::Credential {
            provider,
            message: message.into(),
        }
    }

    pub fn quota(provider: &'static str, message: impl Into<String>) -> Self {
        Self::Quota {
            provider,
            message: message.into(),
        }
    }

    pub fn network(provider: &'static str, message: impl Into<String>) -> Self {
        Self::Network {
            provider,
            message: message.into(),
        }
    }

    pub fn audio(provider: &'static str, message: impl Into<String>) -> Self {
        Self::Audio {
            provider,
            message: message.into(),
        }
    }

    pub fn configuration(provider: &'static str, message: impl Into<String>) -> Self {
        Self::Configuration {
            provider,
            message: message.into(),
        }
    }

    pub fn invalid_response(provider: &'static str, message: impl Into<String>) -> Self {
        Self::InvalidResponse {
            provider,
            message: message.into(),
        }
    }

    pub fn canceled(provider: &'static str) -> Self {
        Self::Canceled { provider }
    }

    pub fn provider(&self) -> &'static str {
        match self {
            Self::Failure { provider, .. }
            | Self::Credential { provider, .. }
            | Self::Quota { provider, .. }
            | Self::Network { provider, .. }
            | Self::Audio { provider, .. }
            | Self::Configuration { provider, .. }
            | Self::InvalidResponse { provider, .. }
            | Self::Canceled { provider } => provider,
        }
    }

    /// Stable, redacted category safe to persist and to return to an interface.
    pub fn category(&self) -> ProcessingJobErrorCategory {
        match self {
            Self::Credential { .. } => ProcessingJobErrorCategory::Credential,
            Self::Quota { .. } => ProcessingJobErrorCategory::Quota,
            Self::Network { .. } => ProcessingJobErrorCategory::Network,
            Self::Audio { .. } => ProcessingJobErrorCategory::Audio,
            Self::Configuration { .. } => ProcessingJobErrorCategory::Configuration,
            Self::InvalidResponse { .. } => ProcessingJobErrorCategory::InvalidProviderOutput,
            Self::Canceled { .. } => ProcessingJobErrorCategory::Canceled,
            Self::Failure { provider, .. } if matches!(*provider, "sqlite" | "storage") => {
                ProcessingJobErrorCategory::Storage
            }
            Self::Failure { .. } => ProcessingJobErrorCategory::Provider,
        }
    }
}

#[derive(Debug, Error)]
pub enum CoreError {
    #[error(transparent)]
    Provider(#[from] ProviderError),
    #[error("invalid provider output: {0}")]
    InvalidProviderOutput(String),
    #[error("processing was cancelled")]
    Canceled,
}

impl CoreError {
    /// Stable, redacted category safe to persist and to return to an interface.
    pub fn category(&self) -> ProcessingJobErrorCategory {
        match self {
            Self::InvalidProviderOutput(_) => ProcessingJobErrorCategory::InvalidProviderOutput,
            Self::Canceled => ProcessingJobErrorCategory::Canceled,
            Self::Provider(error) => error.category(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_failures_keep_their_dedicated_category() {
        assert_eq!(
            ProviderError::failure("sqlite", "disk is full").category(),
            ProcessingJobErrorCategory::Storage
        );
        assert_eq!(
            ProviderError::failure("openai", "unexpected").category(),
            ProcessingJobErrorCategory::Provider
        );
    }

    #[test]
    fn typed_failures_map_to_their_own_categories() {
        let cases = [
            (
                ProviderError::credential("openai", "rejected"),
                ProcessingJobErrorCategory::Credential,
            ),
            (
                ProviderError::quota("openai", "no credit"),
                ProcessingJobErrorCategory::Quota,
            ),
            (
                ProviderError::network("openai", "connect failed"),
                ProcessingJobErrorCategory::Network,
            ),
            (
                ProviderError::audio("ffmpeg", "unsupported container"),
                ProcessingJobErrorCategory::Audio,
            ),
            (
                ProviderError::configuration("openai", "missing model"),
                ProcessingJobErrorCategory::Configuration,
            ),
            (
                ProviderError::invalid_response("openai", "missing segments"),
                ProcessingJobErrorCategory::InvalidProviderOutput,
            ),
            (
                ProviderError::canceled("openai"),
                ProcessingJobErrorCategory::Canceled,
            ),
        ];

        for (error, expected) in cases {
            assert_eq!(error.category(), expected, "{error}");
        }
    }

    #[test]
    fn core_cancellation_is_not_reported_as_a_provider_failure() {
        assert_eq!(
            CoreError::Canceled.category(),
            ProcessingJobErrorCategory::Canceled
        );
    }
}
