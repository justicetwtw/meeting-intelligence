use meeting_intelligence_audio::AudioError;
use meeting_intelligence_core::{CoreError, ProcessingJobErrorCategory, ProviderError};
use meeting_intelligence_credentials::{CredentialError, SecretValidationError};
use meeting_intelligence_exporters::ExportError;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0}")]
    Provider(#[from] ProviderError),
    #[error("{0}")]
    Core(#[from] CoreError),
    #[error("{0}")]
    Audio(#[from] AudioError),
    #[error("{0}")]
    Credential(#[from] CredentialError),
    #[error("{0}")]
    InvalidCredential(#[from] SecretValidationError),
    #[error("export failed")]
    Export(#[from] ExportError),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Configuration(String),
    #[error("could not write to the export directory")]
    FileSystem,
    #[error("processing was cancelled")]
    Canceled,
}

impl AppError {
    pub fn category(&self) -> ProcessingJobErrorCategory {
        match self {
            Self::Provider(error) => error.category(),
            Self::Core(error) => error.category(),
            Self::Audio(error) => ProviderError::from(clone_audio(error)).category(),
            Self::Credential(_) | Self::InvalidCredential(_) => {
                ProcessingJobErrorCategory::Credential
            }
            Self::Export(_) | Self::FileSystem => ProcessingJobErrorCategory::Storage,
            Self::NotFound(_) | Self::Configuration(_) => ProcessingJobErrorCategory::Configuration,
            Self::Canceled => ProcessingJobErrorCategory::Canceled,
        }
    }

    /// Shape handed to an interface: a stable category, a sentence a person can
    /// act on, and whether retrying is worth offering.
    pub fn to_dto(&self) -> AppErrorDto {
        let category = self.category();
        AppErrorDto {
            category,
            message: self.user_message(),
            retryable: is_retryable(category),
        }
    }

    fn user_message(&self) -> String {
        match self {
            // These carry a message the provider already redacted.
            Self::NotFound(message) | Self::Configuration(message) => message.clone(),
            Self::InvalidCredential(error) => error.to_string(),
            _ => default_message(self.category()).to_owned(),
        }
    }
}

fn clone_audio(error: &AudioError) -> AudioError {
    match error {
        AudioError::NotFound => AudioError::NotFound,
        AudioError::UnsupportedFormat { extension } => AudioError::UnsupportedFormat {
            extension: extension.clone(),
        },
        AudioError::MissingExtension => AudioError::MissingExtension,
        AudioError::Empty => AudioError::Empty,
        AudioError::FfmpegUnavailable => AudioError::FfmpegUnavailable,
        AudioError::FfmpegFailed => AudioError::FfmpegFailed,
        AudioError::Undecodable => AudioError::Undecodable,
        AudioError::Workspace => AudioError::Workspace,
        AudioError::Canceled => AudioError::Canceled,
    }
}

fn is_retryable(category: ProcessingJobErrorCategory) -> bool {
    matches!(
        category,
        ProcessingJobErrorCategory::Provider
            | ProcessingJobErrorCategory::Network
            | ProcessingJobErrorCategory::Quota
            | ProcessingJobErrorCategory::Storage
            | ProcessingJobErrorCategory::InvalidProviderOutput
            | ProcessingJobErrorCategory::Canceled
    )
}

/// Human-readable text for a redacted category.
///
/// Nothing here is derived from an upstream response, so no provider payload,
/// header, or credential can reach an interface through an error.
pub fn default_message(category: ProcessingJobErrorCategory) -> &'static str {
    match category {
        ProcessingJobErrorCategory::Credential => {
            "The OpenAI API key was rejected. Open Settings and replace it."
        }
        ProcessingJobErrorCategory::Quota => {
            "The OpenAI account has no remaining credit or was rate limited. \
             Check your billing, then retry."
        }
        ProcessingJobErrorCategory::Network => {
            "OpenAI could not be reached. Check the network connection and retry."
        }
        ProcessingJobErrorCategory::Audio => {
            "The recording could not be read. Try a different file or export it \
             again from the recorder."
        }
        ProcessingJobErrorCategory::Configuration => {
            "Meeting Intelligence is not configured for this step. Check Settings."
        }
        ProcessingJobErrorCategory::InvalidProviderOutput => {
            "The provider returned a result that did not match the expected \
             structure. Retry the meeting."
        }
        ProcessingJobErrorCategory::Storage => {
            "The local database or export directory could not be written."
        }
        ProcessingJobErrorCategory::Canceled => "Processing was cancelled.",
        ProcessingJobErrorCategory::Provider => {
            "The provider could not complete this request. Retry the meeting."
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppErrorDto {
    pub category: ProcessingJobErrorCategory,
    pub message: String,
    pub retryable: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rejected_credential_tells_the_operator_what_to_do() {
        let error = AppError::Provider(ProviderError::credential("openai", "rejected"));
        let dto = error.to_dto();

        assert_eq!(dto.category, ProcessingJobErrorCategory::Credential);
        assert!(dto.message.contains("Settings"));
        assert!(!dto.retryable, "retrying a rejected key cannot help");
    }

    #[test]
    fn transient_failures_offer_a_retry() {
        for error in [
            AppError::Provider(ProviderError::network("openai", "x")),
            AppError::Provider(ProviderError::quota("openai", "x")),
            AppError::Provider(ProviderError::failure("openai", "x")),
            AppError::Provider(ProviderError::invalid_response("openai", "x")),
        ] {
            assert!(error.to_dto().retryable, "{error} should offer a retry");
        }
    }

    #[test]
    fn audio_failures_are_reported_as_audio_problems() {
        let dto = AppError::Audio(AudioError::Undecodable).to_dto();

        assert_eq!(dto.category, ProcessingJobErrorCategory::Audio);
        assert!(!dto.retryable);
    }

    #[test]
    fn a_missing_ffmpeg_is_a_configuration_problem() {
        let dto = AppError::Audio(AudioError::FfmpegUnavailable).to_dto();

        assert_eq!(dto.category, ProcessingJobErrorCategory::Configuration);
    }

    #[test]
    fn no_interface_message_can_echo_a_provider_payload() {
        let leaky = "Bearer sk-live-SECRET123 {\"prompt\":\"confidential transcript\"}";
        let error = AppError::Provider(ProviderError::credential("openai", leaky));

        let dto = error.to_dto();
        assert!(!dto.message.contains("SECRET123"));
        assert!(!dto.message.contains("confidential transcript"));
        assert!(!dto.message.contains("Bearer"));
    }

    #[test]
    fn every_category_has_an_actionable_sentence() {
        for category in [
            ProcessingJobErrorCategory::Provider,
            ProcessingJobErrorCategory::Storage,
            ProcessingJobErrorCategory::InvalidProviderOutput,
            ProcessingJobErrorCategory::Credential,
            ProcessingJobErrorCategory::Quota,
            ProcessingJobErrorCategory::Network,
            ProcessingJobErrorCategory::Audio,
            ProcessingJobErrorCategory::Configuration,
            ProcessingJobErrorCategory::Canceled,
        ] {
            let message = default_message(category);
            assert!(!message.is_empty());
            assert!(
                message.ends_with('.'),
                "{category:?} message must be a sentence"
            );
        }
    }
}
