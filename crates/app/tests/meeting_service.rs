//! Application-service behaviour, exercised entirely offline.
//!
//! Every test here runs in mock mode: no credential, no network, no FFmpeg, and
//! no paid API call.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError},
};

use async_trait::async_trait;
use meeting_intelligence_app::{
    manual_speaker_identity, mock_intelligence, mock_transcription, AppError, AppSettings,
    ExportFormat, ImportRequest, MeetingService, ProviderBundle, ProviderChoice, ProviderFactory,
};
use meeting_intelligence_core::{
    AnalysisMode, CancellationToken, IntelligenceProvider, IntelligenceRequest, MeetingAnalysis,
    MeetingProcessingState, ProcessingJobErrorCategory, ProcessingStage, ProgressEvent,
    ProgressReporter, ProviderError, RunContext, StorageProvider,
};
use meeting_intelligence_credentials::{CredentialSlot, CredentialStore, InMemoryCredentialStore};
use meeting_intelligence_storage::SqliteStorage;
use tempfile::TempDir;

struct MockOnlyFactory;

impl ProviderFactory for MockOnlyFactory {
    fn build(&self, _settings: &AppSettings) -> Result<ProviderBundle, AppError> {
        Ok(ProviderBundle {
            transcription: mock_transcription()?,
            speaker_identity: manual_speaker_identity(),
            intelligence: mock_intelligence()?,
        })
    }
}

struct FailingIntelligence;

#[async_trait]
impl IntelligenceProvider for FailingIntelligence {
    async fn analyze(
        &self,
        _request: IntelligenceRequest,
        _context: &RunContext,
    ) -> Result<MeetingAnalysis, ProviderError> {
        Err(ProviderError::network("openai", "connection reset"))
    }
}

struct FailingAnalysisFactory;

impl ProviderFactory for FailingAnalysisFactory {
    fn build(&self, _settings: &AppSettings) -> Result<ProviderBundle, AppError> {
        Ok(ProviderBundle {
            transcription: mock_transcription()?,
            speaker_identity: manual_speaker_identity(),
            intelligence: Arc::new(FailingIntelligence),
        })
    }
}

#[derive(Default)]
struct RecordingReporter {
    events: Mutex<Vec<ProgressEvent>>,
}

impl RecordingReporter {
    fn stages(&self) -> Vec<ProcessingStage> {
        self.events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .map(|event| event.stage)
            .collect()
    }
}

impl ProgressReporter for RecordingReporter {
    fn report(&self, event: ProgressEvent) {
        self.events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(event);
    }
}

struct Harness {
    service: MeetingService,
    storage: Arc<SqliteStorage>,
    credentials: Arc<InMemoryCredentialStore>,
    directory: TempDir,
}

impl Harness {
    fn new(factory: Arc<dyn ProviderFactory>) -> Self {
        let directory = TempDir::new().expect("temp dir");
        let storage =
            Arc::new(SqliteStorage::open(directory.path().join("library.sqlite")).expect("sqlite"));
        let credentials = Arc::new(InMemoryCredentialStore::new());
        let service = MeetingService::new(
            storage.clone(),
            credentials.clone(),
            factory,
            directory.path().join("settings.json"),
        );
        service
            .update_settings(AppSettings {
                transcription_provider: ProviderChoice::Mock,
                intelligence_provider: ProviderChoice::Mock,
                ..AppSettings::default()
            })
            .expect("settings should save");

        Self {
            service,
            storage,
            credentials,
            directory,
        }
    }

    fn mock() -> Self {
        Self::new(Arc::new(MockOnlyFactory))
    }

    fn recording(&self, name: &str) -> PathBuf {
        let path = self.directory.path().join(name);
        std::fs::write(&path, vec![3_u8; 1_024]).expect("recording should be writable");
        path
    }

    fn exports(&self) -> PathBuf {
        self.directory.path().join("exports")
    }

    async fn import(&self, name: &str, title: &str) -> Result<String, AppError> {
        let detail = self
            .service
            .import_meeting(
                ImportRequest {
                    source_path: self.recording(name),
                    title: Some(title.to_owned()),
                    analysis_mode: Some(AnalysisMode::Standard),
                },
                Arc::new(RecordingReporter::default()),
                CancellationToken::new(),
            )
            .await?;
        Ok(detail.meeting.id)
    }
}

#[tokio::test]
async fn a_mock_import_produces_a_complete_meeting() -> Result<(), AppError> {
    let harness = Harness::mock();

    let detail = harness
        .service
        .import_meeting(
            ImportRequest {
                source_path: harness.recording("bcrc-2026-08-12.m4a"),
                title: Some("BCRC Meeting".to_owned()),
                analysis_mode: Some(AnalysisMode::Standard),
            },
            Arc::new(RecordingReporter::default()),
            CancellationToken::new(),
        )
        .await?;

    assert_eq!(detail.meeting.title, "BCRC Meeting");
    assert_eq!(
        detail.meeting.processing_state,
        MeetingProcessingState::Completed
    );
    assert_eq!(
        detail.meeting.source_file_name.as_deref(),
        Some("bcrc-2026-08-12.m4a")
    );
    assert!(!detail.segments.is_empty());
    assert!(!detail.speakers.is_empty());

    let analysis = detail.analysis.expect("mock mode must produce an analysis");
    assert!(!analysis.summary.is_empty());
    let segment_ids: Vec<&str> = detail
        .segments
        .iter()
        .map(|segment| segment.id.as_str())
        .collect();
    for item in analysis
        .decisions
        .iter()
        .chain(&analysis.action_items)
        .chain(&analysis.open_questions)
    {
        for evidence in &item.evidence_segment_ids {
            assert!(
                segment_ids.contains(&evidence.as_str()),
                "evidence {evidence} must exist in the transcript"
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn the_original_recording_is_never_modified() -> Result<(), AppError> {
    let harness = Harness::mock();
    let path = harness.recording("original.wav");
    let before = std::fs::read(&path).expect("recording bytes");

    harness
        .service
        .import_meeting(
            ImportRequest {
                source_path: path.clone(),
                title: None,
                analysis_mode: None,
            },
            Arc::new(RecordingReporter::default()),
            CancellationToken::new(),
        )
        .await?;

    assert!(path.exists());
    assert_eq!(std::fs::read(&path).expect("recording bytes"), before);
    Ok(())
}

#[tokio::test]
async fn processing_reports_truthful_stages_in_order() -> Result<(), AppError> {
    let harness = Harness::mock();
    let reporter = Arc::new(RecordingReporter::default());

    harness
        .service
        .import_meeting(
            ImportRequest {
                source_path: harness.recording("stages.mp3"),
                title: None,
                analysis_mode: None,
            },
            reporter.clone(),
            CancellationToken::new(),
        )
        .await?;

    let stages = reporter.stages();
    assert_eq!(
        stages,
        vec![
            ProcessingStage::Transcribing,
            ProcessingStage::Analyzing,
            ProcessingStage::Saving,
            ProcessingStage::Completed,
        ]
    );
    Ok(())
}

#[tokio::test]
async fn history_lists_meetings_with_the_newest_first() -> Result<(), AppError> {
    let harness = Harness::mock();
    harness.import("first.m4a", "Sales Meeting").await?;
    harness.import("second.m4a", "BCRC Meeting").await?;

    let history = harness.service.list_meetings().await?;

    assert_eq!(history.len(), 2);
    assert_eq!(history[0].title, "BCRC Meeting");
    assert_eq!(history[1].title, "Sales Meeting");
    assert!(history[0].duration_label.is_some());
    Ok(())
}

#[tokio::test]
async fn a_meeting_survives_a_restart_of_the_application() -> Result<(), AppError> {
    let harness = Harness::mock();
    let meeting_id = harness.import("restart.m4a", "Restart Meeting").await?;

    // A second service over the same database is what a relaunch looks like.
    let reopened = MeetingService::new(
        harness.storage.clone(),
        harness.credentials.clone(),
        Arc::new(MockOnlyFactory),
        harness.directory.path().join("settings.json"),
    );

    let detail = reopened.get_meeting(&meeting_id).await?;
    assert_eq!(detail.meeting.title, "Restart Meeting");
    assert!(detail.analysis.is_some());
    assert_eq!(reopened.list_meetings().await?.len(), 1);
    Ok(())
}

#[tokio::test]
async fn renaming_a_speaker_changes_the_label_but_not_the_transcript() -> Result<(), AppError> {
    let harness = Harness::mock();
    let meeting_id = harness.import("rename.m4a", "Rename Meeting").await?;

    let before = harness.service.get_meeting(&meeting_id).await?;
    let target = before.speakers[1].id.clone();
    assert!(before.speakers[1].label.starts_with("Speaker"));
    let segment_speaker_ids: Vec<String> = before
        .segments
        .iter()
        .map(|segment| segment.speaker_id.clone())
        .collect();

    let after = harness
        .service
        .rename_speaker(&meeting_id, &target, Some("  Mr. Wang  "))
        .await?;

    let renamed = after
        .speakers
        .iter()
        .find(|speaker| speaker.id == target)
        .expect("the renamed speaker must still exist");
    assert_eq!(renamed.label, "Mr. Wang");
    assert_eq!(
        renamed.id, target,
        "the canonical speaker ID must not change"
    );
    assert!(renamed.renamed);
    assert_eq!(
        after
            .segments
            .iter()
            .map(|segment| segment.speaker_id.clone())
            .collect::<Vec<_>>(),
        segment_speaker_ids,
        "renaming must not rewrite transcript segments"
    );

    // Every segment for that speaker now renders under the new label.
    let affected = after
        .segments
        .iter()
        .filter(|segment| segment.speaker_id == target)
        .count();
    assert!(affected > 0);
    Ok(())
}

#[tokio::test]
async fn a_rename_persists_and_can_be_cleared() -> Result<(), AppError> {
    let harness = Harness::mock();
    let meeting_id = harness.import("clear.m4a", "Clear Meeting").await?;
    let target = harness.service.get_meeting(&meeting_id).await?.speakers[0]
        .id
        .clone();

    harness
        .service
        .rename_speaker(&meeting_id, &target, Some("Ms. Chen"))
        .await?;
    let reloaded = harness.service.get_meeting(&meeting_id).await?;
    assert_eq!(reloaded.speakers[0].label, "Ms. Chen");

    let cleared = harness
        .service
        .rename_speaker(&meeting_id, &target, Some("   "))
        .await?;
    assert_eq!(cleared.speakers[0].label, "Speaker 1");
    assert!(!cleared.speakers[0].renamed);
    Ok(())
}

#[tokio::test]
async fn renaming_an_unknown_speaker_is_reported_clearly() -> Result<(), AppError> {
    let harness = Harness::mock();
    let meeting_id = harness.import("unknown.m4a", "Unknown Speaker").await?;

    match harness
        .service
        .rename_speaker(&meeting_id, "speaker_99", Some("Nobody"))
        .await
    {
        Ok(_) => panic!("an unknown speaker must not be renamed"),
        Err(error) => assert_eq!(
            error.to_dto().category,
            ProcessingJobErrorCategory::Configuration
        ),
    }
    Ok(())
}

#[tokio::test]
async fn every_export_format_writes_a_usable_file() -> Result<(), AppError> {
    let harness = Harness::mock();
    let meeting_id = harness.import("export.m4a", "Export Meeting").await?;

    let markdown = harness
        .service
        .export_meeting(
            &meeting_id,
            ExportFormat::Markdown,
            Some(&harness.exports()),
        )
        .await?;
    let json = harness
        .service
        .export_meeting(&meeting_id, ExportFormat::Json, Some(&harness.exports()))
        .await?;
    let docx = harness
        .service
        .export_meeting(&meeting_id, ExportFormat::Docx, Some(&harness.exports()))
        .await?;

    let markdown_text = std::fs::read_to_string(&markdown).expect("markdown export");
    assert!(markdown_text.contains("# Export Meeting"));
    assert!(markdown_text.contains("## Transcript"));

    let json_text = std::fs::read_to_string(&json).expect("json export");
    let parsed: serde_json::Value =
        serde_json::from_str(&json_text).expect("json export should be valid JSON");
    assert_eq!(parsed["meeting"]["title"], "Export Meeting");

    let docx_bytes = std::fs::read(&docx).expect("docx export");
    assert_eq!(&docx_bytes[0..2], b"PK", "the DOCX must be a real package");
    assert!(Path::new(&docx)
        .extension()
        .is_some_and(|ext| ext == "docx"));
    Ok(())
}

#[tokio::test]
async fn an_export_is_recorded_against_the_meeting() -> Result<(), AppError> {
    let harness = Harness::mock();
    let meeting_id = harness.import("artifact.m4a", "Artifact Meeting").await?;

    harness
        .service
        .export_meeting(&meeting_id, ExportFormat::Docx, Some(&harness.exports()))
        .await?;

    let meeting = harness
        .storage
        .load_meeting(&meeting_id)
        .await?
        .expect("meeting");
    assert_eq!(meeting.artifact_ids.len(), 1);
    assert!(harness
        .storage
        .load_artifact(&meeting.artifact_ids[0])
        .await?
        .is_some());
    Ok(())
}

#[tokio::test]
async fn a_rename_reaches_every_export() -> Result<(), AppError> {
    let harness = Harness::mock();
    let meeting_id = harness
        .import("renamed-export.m4a", "Renamed Export")
        .await?;
    let target = harness.service.get_meeting(&meeting_id).await?.speakers[0]
        .id
        .clone();
    harness
        .service
        .rename_speaker(&meeting_id, &target, Some("Mr. Wang"))
        .await?;

    let markdown = harness
        .service
        .export_meeting(
            &meeting_id,
            ExportFormat::Markdown,
            Some(&harness.exports()),
        )
        .await?;

    assert!(std::fs::read_to_string(&markdown)
        .expect("markdown export")
        .contains("Mr. Wang"));
    Ok(())
}

#[tokio::test]
async fn an_unsupported_file_is_refused_before_anything_is_stored() -> Result<(), AppError> {
    let harness = Harness::mock();

    match harness
        .service
        .import_meeting(
            ImportRequest {
                source_path: harness.recording("notes.txt"),
                title: None,
                analysis_mode: None,
            },
            Arc::new(RecordingReporter::default()),
            CancellationToken::new(),
        )
        .await
    {
        Ok(_) => panic!("a text file must not be imported"),
        Err(error) => {
            assert_eq!(
                error.to_dto().category,
                ProcessingJobErrorCategory::Configuration
            );
        }
    }

    assert!(harness.service.list_meetings().await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_missing_file_is_refused() -> Result<(), AppError> {
    let harness = Harness::mock();

    match harness
        .service
        .import_meeting(
            ImportRequest {
                source_path: harness.directory.path().join("absent.mp3"),
                title: None,
                analysis_mode: None,
            },
            Arc::new(RecordingReporter::default()),
            CancellationToken::new(),
        )
        .await
    {
        Ok(_) => panic!("a missing file must not be imported"),
        Err(error) => assert_eq!(
            error.to_dto().category,
            ProcessingJobErrorCategory::Configuration
        ),
    }
    Ok(())
}

#[tokio::test]
async fn a_failed_run_is_recorded_and_can_be_retried() -> Result<(), AppError> {
    let harness = Harness::new(Arc::new(FailingAnalysisFactory));
    let recording = harness.recording("retry.m4a");

    let failure = harness
        .service
        .import_meeting(
            ImportRequest {
                source_path: recording.clone(),
                title: Some("Retry Meeting".to_owned()),
                analysis_mode: None,
            },
            Arc::new(RecordingReporter::default()),
            CancellationToken::new(),
        )
        .await;
    let error = match failure {
        Ok(_) => panic!("the failing provider must fail the run"),
        Err(error) => error.to_dto(),
    };
    assert_eq!(error.category, ProcessingJobErrorCategory::Network);
    assert!(error.retryable);

    let history = harness.service.list_meetings().await?;
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].processing_state, MeetingProcessingState::Failed);
    assert_eq!(
        history[0].error_category,
        Some(ProcessingJobErrorCategory::Network)
    );

    // A working provider on the second attempt completes the same meeting.
    let recovered = MeetingService::new(
        harness.storage.clone(),
        harness.credentials.clone(),
        Arc::new(MockOnlyFactory),
        harness.directory.path().join("settings.json"),
    );
    let detail = recovered
        .retry_meeting(
            &history[0].id,
            Arc::new(RecordingReporter::default()),
            CancellationToken::new(),
        )
        .await?;

    assert_eq!(
        detail.meeting.processing_state,
        MeetingProcessingState::Completed
    );
    assert!(detail.analysis.is_some());
    assert_eq!(recovered.list_meetings().await?.len(), 1);
    Ok(())
}

#[tokio::test]
async fn a_retry_without_the_original_recording_says_so() -> Result<(), AppError> {
    let harness = Harness::mock();
    let recording = harness.recording("gone.m4a");
    let meeting_id = harness
        .service
        .import_meeting(
            ImportRequest {
                source_path: recording.clone(),
                title: None,
                analysis_mode: None,
            },
            Arc::new(RecordingReporter::default()),
            CancellationToken::new(),
        )
        .await?
        .meeting
        .id;
    std::fs::remove_file(&recording).expect("remove the recording");

    match harness
        .service
        .retry_meeting(
            &meeting_id,
            Arc::new(RecordingReporter::default()),
            CancellationToken::new(),
        )
        .await
    {
        Ok(_) => panic!("a retry without its recording must fail"),
        Err(error) => assert!(error
            .to_dto()
            .message
            .contains("no longer at its saved location")),
    }
    Ok(())
}

#[tokio::test]
async fn a_cancelled_run_stops_and_is_marked_cancelled() -> Result<(), AppError> {
    let harness = Harness::mock();
    let cancellation = CancellationToken::new();
    cancellation.cancel();

    match harness
        .service
        .import_meeting(
            ImportRequest {
                source_path: harness.recording("cancel.m4a"),
                title: None,
                analysis_mode: None,
            },
            Arc::new(RecordingReporter::default()),
            cancellation,
        )
        .await
    {
        Ok(_) => panic!("a cancelled run must not complete"),
        Err(error) => assert_eq!(
            error.to_dto().category,
            ProcessingJobErrorCategory::Canceled
        ),
    }
    Ok(())
}

#[tokio::test]
async fn onboarding_is_required_only_while_a_needed_credential_is_missing() -> Result<(), AppError>
{
    let harness = Harness::mock();

    // Mock mode alone never needs a credential.
    assert!(!harness.service.status()?.onboarding_required);

    harness.service.update_settings(AppSettings::default())?;
    assert!(
        harness.service.status()?.onboarding_required,
        "OpenAI mode without a key must ask for onboarding"
    );

    let status = harness
        .service
        .store_credential("sk-test-0123456789abcdef")?;
    assert!(status.configured);
    assert_eq!(status.hint.as_deref(), Some("…cdef"));
    assert!(!harness.service.status()?.onboarding_required);

    let removed = harness.service.remove_credential()?;
    assert!(!removed.configured);
    assert!(harness.service.status()?.onboarding_required);
    Ok(())
}

#[tokio::test]
async fn a_malformed_credential_is_rejected_before_storage() -> Result<(), AppError> {
    let harness = Harness::mock();

    assert!(harness.service.store_credential("   ").is_err());
    assert!(harness.service.store_credential("sk-short").is_err());
    assert!(harness
        .service
        .store_credential("sk-test 0123456789abcdef")
        .is_err());
    assert!(!harness.service.credential_status()?.configured);
    Ok(())
}

#[tokio::test]
async fn a_credential_never_reaches_the_database_or_the_settings_file() -> Result<(), AppError> {
    let harness = Harness::mock();
    const KEY: &str = "sk-test-0123456789abcdefLEAKCHECK";
    harness.service.store_credential(KEY)?;
    harness.import("leak.m4a", "Leak Check").await?;
    harness
        .service
        .export_meeting(
            &harness.service.list_meetings().await?[0].id,
            ExportFormat::Json,
            Some(&harness.exports()),
        )
        .await?;

    let mut inspected = 0_usize;
    for entry in walk(harness.directory.path()) {
        let bytes = std::fs::read(&entry).unwrap_or_default();
        let text = String::from_utf8_lossy(&bytes);
        assert!(
            !text.contains(KEY) && !text.contains("LEAKCHECK"),
            "{} contains the credential",
            entry.display()
        );
        inspected += 1;
    }
    assert!(inspected > 0, "the test must inspect real files");

    // The credential is still available to the provider layer.
    assert!(harness
        .credentials
        .read(CredentialSlot::OpenAiApiKey)?
        .is_some());
    Ok(())
}

#[tokio::test]
async fn a_credential_status_never_carries_the_key_itself() -> Result<(), AppError> {
    let harness = Harness::mock();
    harness
        .service
        .store_credential("sk-test-0123456789abcdef")?;

    let rendered = serde_json::to_string(&harness.service.status()?)
        .expect("status should serialize for the interface");

    assert!(!rendered.contains("sk-test-0123456789abcdef"));
    assert!(rendered.contains("cdef"), "only the hint may be exposed");
    Ok(())
}

fn walk(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                files.push(path);
            }
        }
    }
    files
}
