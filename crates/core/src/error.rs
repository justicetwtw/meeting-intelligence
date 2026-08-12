use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("{provider} provider failed: {message}")]
    Failure {
        provider: &'static str,
        message: String,
    },
}

impl ProviderError {
    pub fn failure(provider: &'static str, message: impl Into<String>) -> Self {
        Self::Failure {
            provider,
            message: message.into(),
        }
    }
}

#[derive(Debug, Error)]
pub enum CoreError {
    #[error(transparent)]
    Provider(#[from] ProviderError),
    #[error("invalid provider output: {0}")]
    InvalidProviderOutput(String),
}
