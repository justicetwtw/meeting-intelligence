use std::{collections::HashMap, sync::Arc};

use meeting_intelligence_core::{
    IntelligenceProvider, MeetingAnalysis, SpeakerIdentityProvider, Transcript,
    TranscriptionProvider,
};
#[cfg(feature = "openai")]
use meeting_intelligence_credentials::CredentialSlot;
use meeting_intelligence_credentials::CredentialStore;
use meeting_intelligence_providers::{
    FixtureTranscriptionProvider, MockIntelligenceProvider, MockSpeakerIdentityProvider,
};

#[cfg(feature = "openai")]
use crate::ProviderChoice;
use crate::{AppError, AppSettings};

/// Synthetic fixtures compiled into the binary.
///
/// Mock mode therefore works in a packaged application, in CI, and in a clean
/// clone without any file path, credential, model download, or network access.
const MOCK_TRANSCRIPT: &str =
    include_str!("../../../fixtures/transcription/bootstrap-planning.json");
const MOCK_ANALYSIS: &str = include_str!("../../../fixtures/analysis/bootstrap-planning.json");

pub struct ProviderBundle {
    pub transcription: Arc<dyn TranscriptionProvider>,
    pub speaker_identity: Arc<dyn SpeakerIdentityProvider>,
    pub intelligence: Arc<dyn IntelligenceProvider>,
}

/// Builds the providers a run needs from the current configuration.
///
/// Interfaces never construct a provider; they choose a mode and this factory
/// resolves it, which is what keeps OpenAI out of the UI layer.
pub trait ProviderFactory: Send + Sync {
    fn build(&self, settings: &AppSettings) -> Result<ProviderBundle, AppError>;
}

pub fn mock_transcription() -> Result<Arc<dyn TranscriptionProvider>, AppError> {
    let transcript: Transcript = serde_json::from_str(MOCK_TRANSCRIPT).map_err(|_| {
        AppError::Configuration("the bundled mock transcript is invalid".to_owned())
    })?;
    Ok(Arc::new(
        FixtureTranscriptionProvider::new(transcript).retargeting_meeting_id(),
    ))
}

pub fn mock_intelligence() -> Result<Arc<dyn IntelligenceProvider>, AppError> {
    let analysis: MeetingAnalysis = serde_json::from_str(MOCK_ANALYSIS)
        .map_err(|_| AppError::Configuration("the bundled mock analysis is invalid".to_owned()))?;
    Ok(Arc::new(
        MockIntelligenceProvider::new(analysis).retargeting_meeting_id(),
    ))
}

/// M1 has no voiceprint enrollment, so identity always resolves to `unknown`
/// and an operator names speakers by hand. Nothing here guesses.
pub fn manual_speaker_identity() -> Arc<dyn SpeakerIdentityProvider> {
    Arc::new(MockSpeakerIdentityProvider::new(HashMap::new()))
}

/// Production factory.
///
/// Without the `openai` feature only mock providers can be built, so a build
/// intended for offline use cannot reach a paid API even if configured to.
pub struct RuntimeProviderFactory {
    #[cfg_attr(not(feature = "openai"), allow(dead_code))]
    credentials: Arc<dyn CredentialStore>,
    #[cfg(feature = "openai")]
    openai: meeting_intelligence_providers::openai::OpenAiConfig,
}

impl RuntimeProviderFactory {
    pub fn new(credentials: Arc<dyn CredentialStore>) -> Self {
        Self {
            credentials,
            #[cfg(feature = "openai")]
            openai: meeting_intelligence_providers::openai::OpenAiConfig::default(),
        }
    }

    #[cfg(feature = "openai")]
    pub fn with_openai_config(
        mut self,
        config: meeting_intelligence_providers::openai::OpenAiConfig,
    ) -> Self {
        self.openai = config;
        self
    }

    #[cfg(feature = "openai")]
    fn openai_transport(
        &self,
    ) -> Result<Arc<dyn meeting_intelligence_providers::openai::HttpTransport>, AppError> {
        let secret = self
            .credentials
            .read(CredentialSlot::OpenAiApiKey)?
            .ok_or_else(|| {
                AppError::Configuration(
                    "No OpenAI API key is configured. Add one in Settings.".to_owned(),
                )
            })?;
        Ok(meeting_intelligence_providers::openai::live_transport(
            &self.openai,
            secret,
        )?)
    }
}

impl ProviderFactory for RuntimeProviderFactory {
    fn build(&self, settings: &AppSettings) -> Result<ProviderBundle, AppError> {
        #[cfg(not(feature = "openai"))]
        {
            if settings.uses_openai() {
                return Err(AppError::Configuration(
                    "This build contains mock providers only. Select the mock \
                     transcription and intelligence providers in Settings."
                        .to_owned(),
                ));
            }
            Ok(ProviderBundle {
                transcription: mock_transcription()?,
                speaker_identity: manual_speaker_identity(),
                intelligence: mock_intelligence()?,
            })
        }

        #[cfg(feature = "openai")]
        {
            use meeting_intelligence_audio::{
                AudioPreparation, AudioPreparationConfig, SystemFfmpeg,
            };
            use meeting_intelligence_providers::openai::{
                OpenAiIntelligenceProvider, OpenAiTranscriptionProvider,
            };

            let transport = if settings.uses_openai() {
                Some(self.openai_transport()?)
            } else {
                None
            };

            let transcription: Arc<dyn TranscriptionProvider> =
                match settings.transcription_provider {
                    ProviderChoice::Mock => mock_transcription()?,
                    ProviderChoice::OpenAi => {
                        let runner = match &settings.ffmpeg_directory {
                            Some(directory) => SystemFfmpeg::from_directory(directory),
                            None => SystemFfmpeg::from_path(),
                        };
                        let preparation = Arc::new(AudioPreparation::new(
                            Arc::new(runner),
                            AudioPreparationConfig::default(),
                        ));
                        let transport = transport.clone().ok_or_else(|| {
                            AppError::Configuration("No OpenAI transport".to_owned())
                        })?;
                        Arc::new(OpenAiTranscriptionProvider::new(
                            transport,
                            preparation,
                            self.openai.clone(),
                        ))
                    }
                };

            let intelligence: Arc<dyn IntelligenceProvider> = match settings.intelligence_provider {
                ProviderChoice::Mock => mock_intelligence()?,
                ProviderChoice::OpenAi => {
                    let transport = transport
                        .ok_or_else(|| AppError::Configuration("No OpenAI transport".to_owned()))?;
                    Arc::new(OpenAiIntelligenceProvider::new(
                        transport,
                        self.openai.clone(),
                    ))
                }
            };

            Ok(ProviderBundle {
                transcription,
                speaker_identity: manual_speaker_identity(),
                intelligence,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use meeting_intelligence_credentials::InMemoryCredentialStore;
    #[cfg(feature = "openai")]
    use meeting_intelligence_credentials::SecretString;

    use super::*;

    #[test]
    fn the_bundled_mock_fixtures_are_valid() -> Result<(), AppError> {
        mock_transcription()?;
        mock_intelligence()?;
        Ok(())
    }

    #[test]
    fn mock_mode_needs_no_credential() -> Result<(), AppError> {
        let factory = RuntimeProviderFactory::new(Arc::new(InMemoryCredentialStore::new()));
        let settings = AppSettings {
            transcription_provider: ProviderChoice::Mock,
            intelligence_provider: ProviderChoice::Mock,
            ..AppSettings::default()
        };

        factory.build(&settings)?;
        Ok(())
    }

    #[cfg(feature = "openai")]
    #[test]
    fn openai_mode_without_a_stored_key_reports_a_configuration_error() {
        let factory = RuntimeProviderFactory::new(Arc::new(InMemoryCredentialStore::new()));

        match factory.build(&AppSettings::default()) {
            Ok(_) => panic!("OpenAI mode must require a credential"),
            Err(error) => assert!(error.to_dto().message.contains("Settings")),
        }
    }

    #[cfg(feature = "openai")]
    #[test]
    fn a_mixed_configuration_builds_both_providers() -> Result<(), AppError> {
        let credentials = Arc::new(InMemoryCredentialStore::new());
        credentials.store(
            CredentialSlot::OpenAiApiKey,
            &SecretString::new("sk-test-0123456789abcdef").expect("test key"),
        )?;
        let factory = RuntimeProviderFactory::new(credentials);

        factory.build(&AppSettings {
            transcription_provider: ProviderChoice::Mock,
            intelligence_provider: ProviderChoice::OpenAi,
            ..AppSettings::default()
        })?;
        Ok(())
    }
}
