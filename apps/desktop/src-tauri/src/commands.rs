//! The native command boundary.
//!
//! Every command is a thin adapter: it converts interface input, delegates to
//! the application service, and converts the result. No provider, credential,
//! SQLite connection, or FFmpeg detail is visible above this layer, and no
//! command returns a stored secret.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex, PoisonError},
};

use meeting_intelligence_app::{
    AppError, AppErrorDto, AppSettings, AppStatusDto, CredentialStatusDto, ExportFormat,
    ImportRequest, MeetingDetailDto, MeetingService, MeetingSummaryDto, SettingsDto,
};
use meeting_intelligence_core::{AnalysisMode, CancellationToken, ProgressEvent};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager, State};

/// Event name the interface subscribes to for processing stages.
pub const PROGRESS_EVENT: &str = "meeting://progress";

pub struct AppState {
    service: Arc<MeetingService>,
    /// Cancellation tokens for runs the interface can still stop, keyed by the
    /// run identifier the interface generated.
    runs: Mutex<HashMap<String, CancellationToken>>,
}

impl AppState {
    pub fn new(service: Arc<MeetingService>) -> Self {
        Self {
            service,
            runs: Mutex::new(HashMap::new()),
        }
    }

    fn register(&self, run_id: &str) -> CancellationToken {
        let token = CancellationToken::new();
        self.runs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(run_id.to_owned(), token.clone());
        token
    }

    fn finish(&self, run_id: &str) {
        self.runs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(run_id);
    }

    fn cancel(&self, run_id: &str) -> bool {
        self.runs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(run_id)
            .map(|token| {
                token.cancel();
                true
            })
            .unwrap_or(false)
    }
}

/// Progress as the interface sees it: a run identifier, the meeting, a stage,
/// and bounded counts. Never a file name, transcript text, or provider payload.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressPayload {
    pub run_id: String,
    pub meeting_id: String,
    pub stage: String,
    pub current_part: Option<u32>,
    pub total_parts: Option<u32>,
    pub error_category: Option<String>,
}

impl ProgressPayload {
    pub fn new(run_id: &str, event: &ProgressEvent) -> Self {
        Self {
            run_id: run_id.to_owned(),
            meeting_id: event.meeting_id.clone(),
            stage: event.stage.as_str().to_owned(),
            current_part: event.detail.map(|detail| detail.current_part),
            total_parts: event.detail.map(|detail| detail.total_parts),
            error_category: event
                .error_category
                .map(|category| category_name(category).to_owned()),
        }
    }
}

fn category_name(category: meeting_intelligence_core::ProcessingJobErrorCategory) -> &'static str {
    use meeting_intelligence_core::ProcessingJobErrorCategory as Category;
    match category {
        Category::Provider => "provider",
        Category::Storage => "storage",
        Category::InvalidProviderOutput => "invalid_provider_output",
        Category::Credential => "credential",
        Category::Quota => "quota",
        Category::Network => "network",
        Category::Audio => "audio",
        Category::Configuration => "configuration",
        Category::Canceled => "canceled",
    }
}

type CommandResult<T> = Result<T, AppErrorDto>;

fn map(error: AppError) -> AppErrorDto {
    error.to_dto()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsInput {
    pub transcription_provider: meeting_intelligence_app::ProviderChoice,
    pub intelligence_provider: meeting_intelligence_app::ProviderChoice,
    pub analysis_mode: AnalysisMode,
    pub theme: meeting_intelligence_app::ThemeChoice,
    pub output_directory: Option<String>,
    pub ffmpeg_directory: Option<String>,
    pub language_hint: Option<String>,
}

impl From<SettingsInput> for AppSettings {
    fn from(input: SettingsInput) -> Self {
        Self {
            transcription_provider: input.transcription_provider,
            intelligence_provider: input.intelligence_provider,
            analysis_mode: input.analysis_mode,
            theme: input.theme,
            output_directory: input.output_directory.map(PathBuf::from),
            ffmpeg_directory: input.ffmpeg_directory.map(PathBuf::from),
            language_hint: input.language_hint,
        }
    }
}

#[tauri::command]
pub fn app_status(state: State<'_, AppState>) -> CommandResult<AppStatusDto> {
    state.service.status().map_err(map)
}

#[tauri::command]
pub fn update_settings(
    state: State<'_, AppState>,
    settings: SettingsInput,
) -> CommandResult<SettingsDto> {
    state
        .service
        .update_settings(AppSettings::from(settings))
        .map_err(map)
}

/// Accepts a pasted key. The raw value is handed straight to the OS credential
/// store and is never echoed back, logged, or written to a settings file.
#[tauri::command]
pub fn save_openai_key(
    state: State<'_, AppState>,
    key: String,
) -> CommandResult<CredentialStatusDto> {
    state.service.store_credential(&key).map_err(map)
}

#[tauri::command]
pub fn remove_openai_key(state: State<'_, AppState>) -> CommandResult<CredentialStatusDto> {
    state.service.remove_credential().map_err(map)
}

#[tauri::command]
pub async fn test_openai_connection(
    state: State<'_, AppState>,
    key: Option<String>,
) -> CommandResult<()> {
    state
        .service
        .test_connection(key.as_deref())
        .await
        .map_err(map)
}

#[tauri::command]
pub async fn list_meetings(state: State<'_, AppState>) -> CommandResult<Vec<MeetingSummaryDto>> {
    state.service.list_meetings().await.map_err(map)
}

#[tauri::command]
pub async fn get_meeting(
    state: State<'_, AppState>,
    meeting_id: String,
) -> CommandResult<MeetingDetailDto> {
    state.service.get_meeting(&meeting_id).await.map_err(map)
}

#[tauri::command]
pub async fn import_meeting(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    run_id: String,
    path: String,
    title: Option<String>,
    analysis_mode: Option<AnalysisMode>,
) -> CommandResult<MeetingDetailDto> {
    let cancellation = state.register(&run_id);
    let reporter = progress_reporter(&app, &run_id);
    let result = state
        .service
        .import_meeting(
            ImportRequest {
                source_path: PathBuf::from(path),
                title,
                analysis_mode,
            },
            reporter,
            cancellation,
        )
        .await;
    state.finish(&run_id);
    result.map_err(map)
}

#[tauri::command]
pub async fn retry_meeting(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    run_id: String,
    meeting_id: String,
) -> CommandResult<MeetingDetailDto> {
    let cancellation = state.register(&run_id);
    let reporter = progress_reporter(&app, &run_id);
    let result = state
        .service
        .retry_meeting(&meeting_id, reporter, cancellation)
        .await;
    state.finish(&run_id);
    result.map_err(map)
}

#[tauri::command]
pub fn cancel_processing(state: State<'_, AppState>, run_id: String) -> CommandResult<bool> {
    Ok(state.cancel(&run_id))
}

#[tauri::command]
pub async fn rename_speaker(
    state: State<'_, AppState>,
    meeting_id: String,
    speaker_id: String,
    display_name: Option<String>,
) -> CommandResult<MeetingDetailDto> {
    state
        .service
        .rename_speaker(&meeting_id, &speaker_id, display_name.as_deref())
        .await
        .map_err(map)
}

#[tauri::command]
pub async fn export_meeting(
    state: State<'_, AppState>,
    meeting_id: String,
    format: ExportFormat,
    directory: Option<String>,
) -> CommandResult<String> {
    state
        .service
        .export_meeting(
            &meeting_id,
            format,
            directory.as_deref().map(std::path::Path::new),
        )
        .await
        .map_err(map)
}

/// Bridges Meeting Core progress onto the webview event bus.
fn progress_reporter(
    app: &tauri::AppHandle,
    run_id: &str,
) -> Arc<dyn meeting_intelligence_core::ProgressReporter> {
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel::<ProgressEvent>();
    let app = app.clone();
    let run_id = run_id.to_owned();
    tauri::async_runtime::spawn(async move {
        while let Some(event) = receiver.recv().await {
            // A closed webview is not a processing failure.
            let _ = app.emit(PROGRESS_EVENT, ProgressPayload::new(&run_id, &event));
        }
    });
    Arc::new(meeting_intelligence_app::ChannelProgressReporter::new(
        sender,
    ))
}

/// Directory holding the local library and settings.
pub fn data_directory(app: &tauri::AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use meeting_intelligence_core::{ProcessingJobErrorCategory, ProcessingStage, ProgressDetail};

    use super::*;

    fn event(stage: ProcessingStage) -> ProgressEvent {
        ProgressEvent {
            meeting_id: "m1".to_owned(),
            job_id: "job-m1".to_owned(),
            stage,
            detail: None,
            error_category: None,
        }
    }

    #[test]
    fn a_progress_payload_carries_stages_and_counts_only() {
        let mut source = event(ProcessingStage::Uploading);
        source.detail = Some(ProgressDetail {
            current_part: 2,
            total_parts: 4,
        });

        let payload = ProgressPayload::new("run-1", &source);

        assert_eq!(payload.run_id, "run-1");
        assert_eq!(payload.meeting_id, "m1");
        assert_eq!(payload.stage, "uploading");
        assert_eq!(payload.current_part, Some(2));
        assert_eq!(payload.total_parts, Some(4));
        assert_eq!(payload.error_category, None);
    }

    #[test]
    fn a_failure_payload_carries_a_redacted_category() {
        let mut source = event(ProcessingStage::Failed);
        source.error_category = Some(ProcessingJobErrorCategory::Credential);

        let payload = ProgressPayload::new("run-1", &source);

        assert_eq!(payload.stage, "failed");
        assert_eq!(payload.error_category.as_deref(), Some("credential"));
    }

    #[test]
    fn cancellation_tokens_are_registered_and_released_by_run_id() {
        let service = Arc::new(MeetingService::new(
            Arc::new(meeting_intelligence_providers::InMemoryStorageProvider::default()),
            Arc::new(meeting_intelligence_credentials::InMemoryCredentialStore::new()),
            Arc::new(meeting_intelligence_app::RuntimeProviderFactory::new(
                Arc::new(meeting_intelligence_credentials::InMemoryCredentialStore::new()),
            )),
            std::env::temp_dir().join("meeting-intelligence-command-test-settings.json"),
        ));
        let state = AppState::new(service);

        assert!(!state.cancel("run-1"), "an unknown run cannot be cancelled");

        let token = state.register("run-1");
        assert!(state.cancel("run-1"));
        assert!(token.is_canceled());

        state.finish("run-1");
        assert!(!state.cancel("run-1"));
    }
}
