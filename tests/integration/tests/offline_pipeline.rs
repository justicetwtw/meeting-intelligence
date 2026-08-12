use std::{collections::HashMap, fs, path::Path, path::PathBuf, sync::Arc};

use anyhow::{anyhow, Context, Result};
use meeting_intelligence_core::{
    AudioSource, CoreError, Meeting, MeetingAnalysis, MeetingCore, ProcessMeetingRequest,
    ProcessedMeeting, ProcessingJobStatus, SpeakerIdentityStatus, SpeakerProfile, StorageProvider,
    Transcript,
};
use meeting_intelligence_exporters::{
    ArtifactExporter, ExportBundle, JsonExporter, MarkdownExporter,
};
use meeting_intelligence_providers::{
    FixtureTranscriptionProvider, InMemoryStorageProvider, MockIntelligenceProvider,
    MockSpeakerIdentityProvider,
};
use meeting_intelligence_storage::SqliteStorage;
use serde::de::DeserializeOwned;

#[tokio::test]
async fn fixture_pipeline_exports_markdown_and_json_without_guessing_speakers() -> Result<()> {
    let fixture_root = fixture_root();
    let meeting: Meeting = read_json(&fixture_root.join("meeting.json"))?;
    let speaker_profiles: Vec<SpeakerProfile> =
        read_json(&fixture_root.join("speakers/profiles.json"))?;
    let transcription = FixtureTranscriptionProvider::from_json_path(
        fixture_root.join("transcription/bootstrap-planning.json"),
    )?;
    let identities = MockSpeakerIdentityProvider::from_json_path(
        fixture_root.join("speakers/identity-candidates.json"),
    )?;
    let intelligence = MockIntelligenceProvider::from_json_path(
        fixture_root.join("analysis/bootstrap-planning.json"),
    )?;
    let storage = Arc::new(InMemoryStorageProvider::default());
    let core = MeetingCore::new(
        Arc::new(transcription),
        Arc::new(identities),
        Arc::new(intelligence),
        storage.clone(),
    );

    let processed = core
        .process(ProcessMeetingRequest {
            meeting,
            audio_source: AudioSource::Fixture {
                reference: "transcription/bootstrap-planning.json".to_owned(),
            },
            language_hints: vec!["en".to_owned()],
            known_context: Some("offline integration test".to_owned()),
            speaker_profiles,
        })
        .await?;

    let known = processed
        .speakers
        .iter()
        .find(|speaker| speaker.transcript_speaker_id == "speaker_01")
        .ok_or_else(|| anyhow!("fixture did not return speaker_01"))?;
    assert_eq!(known.identity, SpeakerIdentityStatus::Resolved);
    assert_eq!(known.display_name.as_deref(), Some("Alice"));

    let low_confidence = processed
        .speakers
        .iter()
        .find(|speaker| speaker.transcript_speaker_id == "speaker_02")
        .ok_or_else(|| anyhow!("fixture did not return speaker_02"))?;
    assert_eq!(low_confidence.identity, SpeakerIdentityStatus::Unknown);
    assert_eq!(low_confidence.profile_id, None);
    assert_eq!(low_confidence.display_name, None);
    assert_eq!(low_confidence.confidence, Some(0.42));
    assert_eq!(processed.job.status, ProcessingJobStatus::Completed);
    assert!(processed.job.completed_at_ms.is_some());
    assert!(processed.job.updated_at_ms >= processed.job.created_at_ms);
    assert_eq!(processed.job.error_category, None);

    let bundle = ExportBundle::from(&processed);
    let markdown = MarkdownExporter.export(&bundle)?;
    let json = JsonExporter.export(&bundle)?;
    let decoded: ExportBundle = serde_json::from_str(&json.content)?;

    assert!(markdown.content.contains("## Decisions"));
    assert!(markdown.content.contains("Unknown speaker"));
    assert!(markdown.content.contains("seg_003"));
    assert_eq!(decoded, bundle);
    assert_eq!(
        storage.load_analysis("meeting_fixture_001").await?,
        Some(processed.analysis)
    );
    Ok(())
}

#[tokio::test]
async fn sqlite_storage_round_trips_canonical_meeting() -> Result<()> {
    let storage = SqliteStorage::open_in_memory()?;
    let meeting: Meeting = read_json(&fixture_root().join("meeting.json"))?;
    let profiles: Vec<SpeakerProfile> = read_json(&fixture_root().join("speakers/profiles.json"))?;
    let profile = profiles
        .first()
        .ok_or_else(|| anyhow!("speaker profile fixture is empty"))?;

    storage.save_meeting(&meeting).await?;
    storage.save_speaker_profile(profile).await?;

    assert_eq!(storage.load_meeting(&meeting.id).await?, Some(meeting));
    assert_eq!(
        storage.load_speaker_profile(&profile.id).await?,
        Some(profile.clone())
    );
    Ok(())
}

#[tokio::test]
async fn rejects_transcript_for_a_different_meeting() -> Result<()> {
    let (mut transcript, analysis) = pipeline_fixtures()?;
    transcript.meeting_id = "wrong-meeting".to_owned();

    let result = run_pipeline(transcript, analysis).await?;

    assert!(
        matches!(result, Err(CoreError::InvalidProviderOutput(message)) if message.contains("meeting_id"))
    );
    Ok(())
}

#[tokio::test]
async fn rejects_duplicate_segment_ids() -> Result<()> {
    let (mut transcript, analysis) = pipeline_fixtures()?;
    transcript.segments[1].id = transcript.segments[0].id.clone();

    let result = run_pipeline(transcript, analysis).await?;

    assert!(
        matches!(result, Err(CoreError::InvalidProviderOutput(message)) if message.contains("unique"))
    );
    Ok(())
}

#[tokio::test]
async fn rejects_overlapping_or_out_of_order_segments() -> Result<()> {
    let (mut transcript, analysis) = pipeline_fixtures()?;
    transcript.segments[1].start_ms = transcript.segments[0].end_ms - 1;

    let result = run_pipeline(transcript, analysis).await?;

    assert!(
        matches!(result, Err(CoreError::InvalidProviderOutput(message)) if message.contains("overlap"))
    );
    Ok(())
}

#[tokio::test]
async fn rejects_analysis_for_a_different_meeting() -> Result<()> {
    let (transcript, mut analysis) = pipeline_fixtures()?;
    analysis.meeting_id = "wrong-meeting".to_owned();

    let result = run_pipeline(transcript, analysis).await?;

    assert!(
        matches!(result, Err(CoreError::InvalidProviderOutput(message)) if message.contains("analysis meeting_id"))
    );
    Ok(())
}

#[tokio::test]
async fn rejects_unknown_analysis_evidence() -> Result<()> {
    let (transcript, mut analysis) = pipeline_fixtures()?;
    analysis.decisions[0].evidence_segment_ids = vec!["seg_missing".to_owned()];

    let result = run_pipeline(transcript, analysis).await?;

    assert!(
        matches!(result, Err(CoreError::InvalidProviderOutput(message)) if message.contains("seg_missing"))
    );
    Ok(())
}

fn pipeline_fixtures() -> Result<(Transcript, MeetingAnalysis)> {
    let root = fixture_root();
    Ok((
        read_json(&root.join("transcription/bootstrap-planning.json"))?,
        read_json(&root.join("analysis/bootstrap-planning.json"))?,
    ))
}

async fn run_pipeline(
    transcript: Transcript,
    analysis: MeetingAnalysis,
) -> Result<std::result::Result<ProcessedMeeting, CoreError>> {
    let root = fixture_root();
    let meeting: Meeting = read_json(&root.join("meeting.json"))?;
    let profiles: Vec<SpeakerProfile> = read_json(&root.join("speakers/profiles.json"))?;
    let core = MeetingCore::new(
        Arc::new(FixtureTranscriptionProvider::new(transcript)),
        Arc::new(MockSpeakerIdentityProvider::new(HashMap::new())),
        Arc::new(MockIntelligenceProvider::new(analysis)),
        Arc::new(InMemoryStorageProvider::default()),
    );

    Ok(core
        .process(ProcessMeetingRequest {
            meeting,
            audio_source: AudioSource::Fixture {
                reference: "negative-test".to_owned(),
            },
            language_hints: vec![],
            known_context: None,
            speaker_profiles: profiles,
        })
        .await)
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let bytes = fs::read(path).with_context(|| format!("could not read {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("could not parse {}", path.display()))
}
