use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, PoisonError, RwLock},
};

use meeting_intelligence_audio::SUPPORTED_EXTENSIONS;
use meeting_intelligence_core::{
    AnalysisMode, AudioSource, CancellationToken, Meeting, MeetingCore, MeetingProcessingState,
    ProcessMeetingRequest, ProcessingJobErrorCategory, ProgressReporter, StorageProvider,
    METADATA_ANALYSIS_MODE, METADATA_INTELLIGENCE_PROVIDER, METADATA_SOURCE_AUDIO_FILE_NAME,
    METADATA_SOURCE_AUDIO_PATH, METADATA_TRANSCRIPTION_PROVIDER,
};
use meeting_intelligence_credentials::{
    CredentialSlot, CredentialStatus, CredentialStore, SecretString,
};
use meeting_intelligence_exporters::{
    ArtifactExporter, DocxExporter, ExportBundle, JsonExporter, MarkdownExporter,
};
use tracing::info;

use crate::{
    clock::{format_rfc3339, unix_time_ms},
    dto::{meeting_detail, ExportFormat, MeetingDetailDto, MeetingSummaryDto, SettingsDto},
    AppError, AppSettings, AppStatusDto, CredentialStatusDto, ProviderChoice, ProviderFactory,
};

/// What an interface supplies when importing a recording.
#[derive(Clone, Debug)]
pub struct ImportRequest {
    pub source_path: PathBuf,
    /// Falls back to the recording's file name when absent.
    pub title: Option<String>,
    /// Falls back to the configured default analysis mode.
    pub analysis_mode: Option<AnalysisMode>,
}

pub struct MeetingService {
    storage: Arc<dyn StorageProvider>,
    credentials: Arc<dyn CredentialStore>,
    factory: Arc<dyn ProviderFactory>,
    settings: RwLock<AppSettings>,
    settings_path: PathBuf,
}

impl MeetingService {
    pub fn new(
        storage: Arc<dyn StorageProvider>,
        credentials: Arc<dyn CredentialStore>,
        factory: Arc<dyn ProviderFactory>,
        settings_path: PathBuf,
    ) -> Self {
        let settings = AppSettings::load(&settings_path);
        Self {
            storage,
            credentials,
            factory,
            settings: RwLock::new(settings),
            settings_path,
        }
    }

    pub fn settings(&self) -> AppSettings {
        self.settings
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn update_settings(&self, settings: AppSettings) -> Result<SettingsDto, AppError> {
        settings.save(&self.settings_path)?;
        let dto = SettingsDto::from(&settings);
        *self
            .settings
            .write()
            .unwrap_or_else(PoisonError::into_inner) = settings;
        Ok(dto)
    }

    pub fn credential_status(&self) -> Result<CredentialStatusDto, AppError> {
        Ok(self
            .credentials
            .status(CredentialSlot::OpenAiApiKey)
            .unwrap_or_else(|_| CredentialStatus::not_configured())
            .into())
    }

    pub fn status(&self) -> Result<AppStatusDto, AppError> {
        let settings = self.settings();
        let credential = self.credential_status()?;
        Ok(AppStatusDto {
            onboarding_required: settings.uses_openai() && !credential.configured,
            credential,
            settings: SettingsDto::from(&settings),
            supported_extensions: SUPPORTED_EXTENSIONS
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            openai_available: cfg!(feature = "openai"),
        })
    }

    /// Stores a credential. The raw value is validated, moved into the OS
    /// credential store, and never returned, logged, or persisted elsewhere.
    pub fn store_credential(&self, raw: &str) -> Result<CredentialStatusDto, AppError> {
        let secret = SecretString::new(raw)?;
        self.credentials
            .store(CredentialSlot::OpenAiApiKey, &secret)?;
        info!("stored the OpenAI credential in the OS credential store");
        self.credential_status()
    }

    pub fn remove_credential(&self) -> Result<CredentialStatusDto, AppError> {
        self.credentials.delete(CredentialSlot::OpenAiApiKey)?;
        info!("removed the OpenAI credential from the OS credential store");
        self.credential_status()
    }

    /// Verifies a credential against the provider with one cheap call.
    ///
    /// `raw` lets onboarding test a key before storing it; when it is `None`
    /// the stored credential is used.
    #[cfg(feature = "openai")]
    pub async fn test_connection(&self, raw: Option<&str>) -> Result<(), AppError> {
        let secret = match raw {
            Some(value) => SecretString::new(value)?,
            None => self
                .credentials
                .read(CredentialSlot::OpenAiApiKey)?
                .ok_or_else(|| {
                    AppError::Configuration(
                        "No OpenAI API key is configured. Add one first.".to_owned(),
                    )
                })?,
        };
        let config = meeting_intelligence_providers::openai::OpenAiConfig::default();
        let transport = meeting_intelligence_providers::openai::live_transport(&config, secret)?;
        meeting_intelligence_providers::openai::verify_credential(transport.as_ref()).await?;
        Ok(())
    }

    #[cfg(not(feature = "openai"))]
    pub async fn test_connection(&self, _raw: Option<&str>) -> Result<(), AppError> {
        Err(AppError::Configuration(
            "This build contains mock providers only and cannot reach OpenAI.".to_owned(),
        ))
    }

    pub async fn list_meetings(&self) -> Result<Vec<MeetingSummaryDto>, AppError> {
        let meetings = self.storage.list_meetings().await?;
        let mut summaries = Vec::with_capacity(meetings.len());
        for meeting in meetings {
            let error_category = self.error_category(&meeting.id).await?;
            summaries.push(MeetingSummaryDto::from_meeting(&meeting, error_category));
        }
        Ok(summaries)
    }

    pub async fn get_meeting(&self, meeting_id: &str) -> Result<MeetingDetailDto, AppError> {
        let meeting = self.load_meeting(meeting_id).await?;
        let transcript = self.storage.load_transcript(meeting_id).await?;
        let speakers = self.storage.load_speakers(meeting_id).await?;
        let analysis = self.storage.load_analysis(meeting_id).await?;
        let error_category = self.error_category(meeting_id).await?;

        Ok(meeting_detail(
            &meeting,
            transcript.as_ref(),
            &speakers,
            analysis.as_ref(),
            error_category,
        ))
    }

    /// Imports a recording and runs it through Meeting Core.
    ///
    /// The original file is never modified or moved; only its path is recorded
    /// so a failed run can be retried.
    pub async fn import_meeting(
        &self,
        request: ImportRequest,
        progress: Arc<dyn ProgressReporter>,
        cancellation: CancellationToken,
    ) -> Result<MeetingDetailDto, AppError> {
        let settings = self.settings();
        validate_extension(&request.source_path)?;
        if !request.source_path.is_file() {
            return Err(AppError::NotFound(
                "That recording could not be found.".to_owned(),
            ));
        }

        let analysis_mode = request.analysis_mode.unwrap_or(settings.analysis_mode);
        let meeting = self.new_meeting(&request, &settings, analysis_mode).await?;
        self.storage.save_meeting(&meeting).await?;

        self.run(meeting, settings, analysis_mode, progress, cancellation)
            .await
    }

    /// Re-runs a stored meeting from its original recording.
    pub async fn retry_meeting(
        &self,
        meeting_id: &str,
        progress: Arc<dyn ProgressReporter>,
        cancellation: CancellationToken,
    ) -> Result<MeetingDetailDto, AppError> {
        let mut meeting = self.load_meeting(meeting_id).await?;
        let settings = self.settings();
        let analysis_mode = meeting
            .metadata
            .get(METADATA_ANALYSIS_MODE)
            .and_then(|value| AnalysisMode::from_str_opt(value))
            .unwrap_or(settings.analysis_mode);

        let source = meeting
            .metadata
            .get(METADATA_SOURCE_AUDIO_PATH)
            .map(PathBuf::from)
            .ok_or_else(|| {
                AppError::NotFound(
                    "This meeting has no stored recording path, so it cannot be retried."
                        .to_owned(),
                )
            })?;
        if !source.is_file() {
            return Err(AppError::NotFound(
                "The original recording is no longer at its saved location.".to_owned(),
            ));
        }

        meeting.processing_state = MeetingProcessingState::Pending;
        meeting.metadata.insert(
            METADATA_TRANSCRIPTION_PROVIDER.to_owned(),
            provider_name(settings.transcription_provider).to_owned(),
        );
        meeting.metadata.insert(
            METADATA_INTELLIGENCE_PROVIDER.to_owned(),
            provider_name(settings.intelligence_provider).to_owned(),
        );

        self.run(meeting, settings, analysis_mode, progress, cancellation)
            .await
    }

    async fn run(
        &self,
        meeting: Meeting,
        settings: AppSettings,
        analysis_mode: AnalysisMode,
        progress: Arc<dyn ProgressReporter>,
        cancellation: CancellationToken,
    ) -> Result<MeetingDetailDto, AppError> {
        let meeting_id = meeting.id.clone();
        let source_path = meeting
            .metadata
            .get(METADATA_SOURCE_AUDIO_PATH)
            .cloned()
            .unwrap_or_default();

        let bundle = self.factory.build(&settings)?;
        let core = MeetingCore::new(
            bundle.transcription,
            bundle.speaker_identity,
            bundle.intelligence,
            self.storage.clone(),
        )
        .with_progress_reporter(progress);

        let result = core
            .process(
                ProcessMeetingRequest::new(meeting, AudioSource::LocalFile { path: source_path })
                    .with_language_hints(settings.language_hints())
                    .with_analysis_mode(analysis_mode)
                    .with_cancellation(cancellation),
            )
            .await;

        match result {
            Ok(_) => self.get_meeting(&meeting_id).await,
            Err(error) => Err(AppError::Core(error)),
        }
    }

    /// Applies an operator-supplied label to a meeting-local speaker.
    ///
    /// The transcript is never rewritten: the canonical speaker ID stays exactly
    /// as the transcription provider produced it, which is what a later
    /// persistent-identity milestone will attach profiles to.
    pub async fn rename_speaker(
        &self,
        meeting_id: &str,
        transcript_speaker_id: &str,
        display_name: Option<&str>,
    ) -> Result<MeetingDetailDto, AppError> {
        let mut speakers = self.storage.load_speakers(meeting_id).await?;
        if speakers.is_empty() {
            return Err(AppError::NotFound(
                "That meeting has no speakers to rename yet.".to_owned(),
            ));
        }

        let speaker = speakers
            .iter_mut()
            .find(|speaker| speaker.transcript_speaker_id == transcript_speaker_id)
            .ok_or_else(|| AppError::NotFound("That speaker is not in this meeting.".to_owned()))?;
        speaker.manual_display_name = display_name
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);

        self.storage.save_speakers(meeting_id, &speakers).await?;
        self.get_meeting(meeting_id).await
    }

    /// Writes an export next to the operator's chosen directory and records the
    /// artifact against the meeting.
    pub async fn export_meeting(
        &self,
        meeting_id: &str,
        format: ExportFormat,
        directory: Option<&Path>,
    ) -> Result<String, AppError> {
        let settings = self.settings();
        let directory = directory
            .map(Path::to_path_buf)
            .or_else(|| settings.output_directory.clone())
            .ok_or_else(|| {
                AppError::Configuration(
                    "Choose an export folder in Settings, or pick one for this export.".to_owned(),
                )
            })?;
        std::fs::create_dir_all(&directory).map_err(|_| AppError::FileSystem)?;

        let mut meeting = self.load_meeting(meeting_id).await?;
        let transcript = self
            .storage
            .load_transcript(meeting_id)
            .await?
            .ok_or_else(|| {
                AppError::NotFound("That meeting has no transcript to export.".to_owned())
            })?;
        let analysis = self
            .storage
            .load_analysis(meeting_id)
            .await?
            .ok_or_else(|| {
                AppError::NotFound("That meeting has no analysis to export.".to_owned())
            })?;
        let speakers = self.storage.load_speakers(meeting_id).await?;

        let bundle = ExportBundle {
            meeting: meeting.clone(),
            transcript,
            speakers,
            analysis,
        };
        let artifact = match format {
            ExportFormat::Markdown => MarkdownExporter.export(&bundle)?,
            ExportFormat::Json => JsonExporter.export(&bundle)?,
            ExportFormat::Docx => DocxExporter.export(&bundle)?,
        };

        let path = directory.join(format!(
            "{}.{}",
            file_stem(&bundle.meeting.title, meeting_id),
            artifact.kind.file_extension()
        ));
        write_artifact(&path, &artifact)?;

        self.storage.save_artifact(&artifact).await?;
        if !meeting.artifact_ids.contains(&artifact.id) {
            meeting.artifact_ids.push(artifact.id.clone());
            self.storage.save_meeting(&meeting).await?;
        }

        info!(meeting_id, kind = ?artifact.kind, "wrote an export");
        Ok(path.to_string_lossy().into_owned())
    }

    async fn load_meeting(&self, meeting_id: &str) -> Result<Meeting, AppError> {
        self.storage
            .load_meeting(meeting_id)
            .await?
            .ok_or_else(|| AppError::NotFound("That meeting is not in this library.".to_owned()))
    }

    async fn error_category(
        &self,
        meeting_id: &str,
    ) -> Result<Option<ProcessingJobErrorCategory>, AppError> {
        Ok(self
            .storage
            .load_job(&format!("job-{meeting_id}"))
            .await?
            .and_then(|job| job.error_category))
    }

    async fn new_meeting(
        &self,
        request: &ImportRequest,
        settings: &AppSettings,
        analysis_mode: AnalysisMode,
    ) -> Result<Meeting, AppError> {
        let file_name = request
            .source_path
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_else(|| "recording".to_owned());
        let title = request
            .title
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| {
                request
                    .source_path
                    .file_stem()
                    .map(|value| value.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Untitled meeting".to_owned())
            });

        let now_ms = unix_time_ms();
        let mut metadata = BTreeMap::new();
        metadata.insert(
            METADATA_SOURCE_AUDIO_PATH.to_owned(),
            request.source_path.to_string_lossy().into_owned(),
        );
        metadata.insert(METADATA_SOURCE_AUDIO_FILE_NAME.to_owned(), file_name);
        metadata.insert(
            METADATA_ANALYSIS_MODE.to_owned(),
            analysis_mode.as_str().to_owned(),
        );
        metadata.insert(
            METADATA_TRANSCRIPTION_PROVIDER.to_owned(),
            provider_name(settings.transcription_provider).to_owned(),
        );
        metadata.insert(
            METADATA_INTELLIGENCE_PROVIDER.to_owned(),
            provider_name(settings.intelligence_provider).to_owned(),
        );

        Ok(Meeting {
            id: self.unique_meeting_id(now_ms, &title).await?,
            title,
            started_at: Some(format_rfc3339(now_ms / 1_000)),
            processing_state: MeetingProcessingState::Pending,
            artifact_ids: Vec::new(),
            participants: Vec::new(),
            duration_ms: None,
            metadata,
        })
    }

    async fn unique_meeting_id(&self, now_ms: u64, title: &str) -> Result<String, AppError> {
        let base = format!("meeting_{now_ms:013}_{:08x}", fnv1a(title));
        if self.storage.load_meeting(&base).await?.is_none() {
            return Ok(base);
        }
        for suffix in 1..1_000_u32 {
            let candidate = format!("{base}_{suffix:03}");
            if self.storage.load_meeting(&candidate).await?.is_none() {
                return Ok(candidate);
            }
        }
        Err(AppError::Configuration(
            "Could not allocate a meeting identifier.".to_owned(),
        ))
    }
}

fn provider_name(choice: ProviderChoice) -> &'static str {
    match choice {
        ProviderChoice::Mock => "mock",
        ProviderChoice::OpenAi => "openai",
    }
}

fn validate_extension(path: &Path) -> Result<(), AppError> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| {
            AppError::Configuration("That file has no recognisable audio extension.".to_owned())
        })?;
    if SUPPORTED_EXTENSIONS.contains(&extension.as_str()) {
        Ok(())
    } else {
        Err(AppError::Configuration(format!(
            "`.{extension}` recordings are not supported."
        )))
    }
}

fn write_artifact(
    path: &Path,
    artifact: &meeting_intelligence_core::Artifact,
) -> Result<(), AppError> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use meeting_intelligence_core::ArtifactEncoding;

    match artifact.encoding {
        ArtifactEncoding::Utf8 => {
            std::fs::write(path, artifact.content.as_bytes()).map_err(|_| AppError::FileSystem)
        }
        ArtifactEncoding::Base64 => {
            let bytes = STANDARD
                .decode(&artifact.content)
                .map_err(|_| AppError::FileSystem)?;
            std::fs::write(path, bytes).map_err(|_| AppError::FileSystem)
        }
    }
}

/// Builds a file name that is safe on Windows and still recognisable.
fn file_stem(title: &str, meeting_id: &str) -> String {
    let sanitized: String = title
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || matches!(character, '-' | '_' | ' ') {
                character
            } else {
                '-'
            }
        })
        .collect();
    let sanitized = sanitized.trim().trim_matches('.').trim();
    let short_id = meeting_id.rsplit('_').next().unwrap_or(meeting_id);
    if sanitized.is_empty() {
        format!("meeting-{short_id}")
    } else {
        let mut trimmed: String = sanitized.chars().take(80).collect();
        trimmed = trimmed.trim().to_owned();
        format!("{trimmed}-{short_id}")
    }
}

fn fnv1a(value: &str) -> u32 {
    let mut hash = 0x811c_9dc5_u32;
    for byte in value.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

/// Collects progress events so an interface can forward them.
pub struct ChannelProgressReporter {
    sender: tokio::sync::mpsc::UnboundedSender<meeting_intelligence_core::ProgressEvent>,
}

impl ChannelProgressReporter {
    pub fn new(
        sender: tokio::sync::mpsc::UnboundedSender<meeting_intelligence_core::ProgressEvent>,
    ) -> Self {
        Self { sender }
    }
}

impl ProgressReporter for ChannelProgressReporter {
    fn report(&self, event: meeting_intelligence_core::ProgressEvent) {
        // A closed receiver means the interface stopped listening; the run
        // itself must continue regardless.
        let _ = self.sender.send(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_file_names_are_windows_safe() {
        assert_eq!(
            file_stem("BCRC: Q3/Q4 review?", "meeting_1786611600000_deadbeef"),
            "BCRC- Q3-Q4 review--deadbeef"
        );
        assert_eq!(file_stem("   ", "meeting_1_abc"), "meeting-abc");
    }

    #[test]
    fn every_supported_extension_is_accepted_and_others_are_not() {
        for extension in ["mp3", "m4a", "wav", "mp4", "MP4"] {
            assert!(
                validate_extension(Path::new(&format!("meeting.{extension}"))).is_ok(),
                "{extension} must be accepted"
            );
        }
        assert!(validate_extension(Path::new("notes.txt")).is_err());
        assert!(validate_extension(Path::new("recording")).is_err());
    }

    #[test]
    fn meeting_id_hashing_is_stable() {
        assert_eq!(fnv1a("BCRC Meeting"), fnv1a("BCRC Meeting"));
        assert_ne!(fnv1a("BCRC Meeting"), fnv1a("Sales Meeting"));
    }
}
