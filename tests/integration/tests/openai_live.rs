//! Opt-in tests that call the real OpenAI API.
//!
//! They are skipped unless the operator sets the opt-in variables below, so
//! `cargo test`, `scripts/verify.ps1`, and CI never spend money or reach the
//! network. Each test is deliberately tiny: the point is to prove that the
//! credential works and that the request/response contracts still hold, not to
//! transcribe a real meeting.
//!
//! Enable with:
//!
//! ```powershell
//! $env:MEETING_INTELLIGENCE_OPENAI_TEST_KEY = "sk-..."
//! # Optional: a short (<60 s) synthetic or public-domain speech recording.
//! $env:MEETING_INTELLIGENCE_OPENAI_TEST_AUDIO = "C:\path\to\short-sample.m4a"
//! cargo test --package meeting-intelligence-integration-tests --features openai-live --test openai_live -- --nocapture
//! ```

#![cfg(feature = "openai-live")]

use std::{path::PathBuf, sync::Arc};

use anyhow::{anyhow, Result};
use meeting_intelligence_audio::{AudioPreparation, AudioPreparationConfig, SystemFfmpeg};
use meeting_intelligence_core::{
    AnalysisMode, AudioSource, IntelligenceProvider, IntelligenceRequest, Meeting,
    MeetingProcessingState, RunContext, SpeakerScope, Transcript, TranscriptSegment,
    TranscriptionProvider, TranscriptionRequest,
};
use meeting_intelligence_credentials::SecretString;
use meeting_intelligence_providers::openai::{
    live_transport, verify_credential, OpenAiConfig, OpenAiIntelligenceProvider,
    OpenAiTranscriptionProvider,
};

const KEY_VARIABLE: &str = "MEETING_INTELLIGENCE_OPENAI_TEST_KEY";
const AUDIO_VARIABLE: &str = "MEETING_INTELLIGENCE_OPENAI_TEST_AUDIO";

/// Returns the opt-in credential, or `None` when the test should be skipped.
/// The value is never printed, asserted on, or included in a failure message.
fn opt_in_key() -> Option<SecretString> {
    let raw = std::env::var(KEY_VARIABLE).ok()?;
    SecretString::new(raw).ok()
}

fn opt_in_audio() -> Option<PathBuf> {
    let raw = std::env::var(AUDIO_VARIABLE).ok()?;
    let path = PathBuf::from(raw);
    path.is_file().then_some(path)
}

fn meeting() -> Meeting {
    Meeting {
        id: "meeting_live_check".to_owned(),
        title: "Live contract check".to_owned(),
        started_at: None,
        processing_state: MeetingProcessingState::Processing,
        artifact_ids: Vec::new(),
        participants: Vec::new(),
        duration_ms: None,
        metadata: Default::default(),
    }
}

#[tokio::test]
async fn live_credential_is_accepted() -> Result<()> {
    let Some(key) = opt_in_key() else {
        eprintln!("skipped: {KEY_VARIABLE} is not set");
        return Ok(());
    };

    let config = OpenAiConfig::default();
    verify_credential(live_transport(&config, key)?.as_ref()).await?;

    println!("live credential check: OK");
    Ok(())
}

#[tokio::test]
async fn live_structured_analysis_matches_the_canonical_contract() -> Result<()> {
    let Some(key) = opt_in_key() else {
        eprintln!("skipped: {KEY_VARIABLE} is not set");
        return Ok(());
    };

    let config = OpenAiConfig::default();
    let provider = OpenAiIntelligenceProvider::new(live_transport(&config, key)?, config);
    // A three-line synthetic transcript keeps the live call cheap and contains
    // no real meeting content.
    let transcript = Transcript {
        meeting_id: "meeting_live_check".to_owned(),
        language: Some("en".to_owned()),
        speaker_scope: SpeakerScope::Meeting,
        segments: vec![
            TranscriptSegment {
                id: "seg_00001".to_owned(),
                speaker_id: "speaker_01".to_owned(),
                start_ms: 0,
                end_ms: 4_000,
                text: "We agreed to ship the beta on Friday.".to_owned(),
            },
            TranscriptSegment {
                id: "seg_00002".to_owned(),
                speaker_id: "speaker_02".to_owned(),
                start_ms: 4_000,
                end_ms: 9_000,
                text: "Someone should double-check the installer first.".to_owned(),
            },
            TranscriptSegment {
                id: "seg_00003".to_owned(),
                speaker_id: "speaker_01".to_owned(),
                start_ms: 9_000,
                end_ms: 13_000,
                text: "Do we still need a signed build for the beta?".to_owned(),
            },
        ],
    };

    let analysis = provider
        .analyze(
            IntelligenceRequest {
                meeting: meeting(),
                transcript: transcript.clone(),
                speakers: Vec::new(),
                analysis_mode: AnalysisMode::Fast,
            },
            &RunContext::detached(),
        )
        .await?;

    assert_eq!(analysis.meeting_id, "meeting_live_check");
    assert!(!analysis.summary.trim().is_empty());
    let known: Vec<&str> = transcript
        .segments
        .iter()
        .map(|segment| segment.id.as_str())
        .collect();
    for evidence in analysis
        .decisions
        .iter()
        .flat_map(|item| &item.evidence_segment_ids)
        .chain(
            analysis
                .action_items
                .iter()
                .flat_map(|item| &item.evidence_segment_ids),
        )
        .chain(
            analysis
                .open_questions
                .iter()
                .flat_map(|item| &item.evidence_segment_ids),
        )
    {
        assert!(
            known.contains(&evidence.as_str()),
            "invalid evidence survived"
        );
    }
    // Nobody was named as an owner, so nothing may be assigned.
    for item in &analysis.action_items {
        assert_eq!(item.assignee_profile_id, None);
    }

    println!(
        "live analysis check: OK ({} decisions, {} action items, {} open questions)",
        analysis.decisions.len(),
        analysis.action_items.len(),
        analysis.open_questions.len()
    );
    Ok(())
}

#[tokio::test]
async fn live_diarized_transcription_maps_to_canonical_segments() -> Result<()> {
    let Some(key) = opt_in_key() else {
        eprintln!("skipped: {KEY_VARIABLE} is not set");
        return Ok(());
    };
    let Some(audio) = opt_in_audio() else {
        eprintln!("skipped: {AUDIO_VARIABLE} is not set to a readable short recording");
        return Ok(());
    };

    let config = OpenAiConfig::default();
    let preparation = Arc::new(AudioPreparation::new(
        Arc::new(SystemFfmpeg::from_path()),
        AudioPreparationConfig::default(),
    ));
    let provider =
        OpenAiTranscriptionProvider::new(live_transport(&config, key)?, preparation, config);

    let transcript = provider
        .transcribe(
            TranscriptionRequest {
                meeting: meeting(),
                audio_source: AudioSource::LocalFile {
                    path: audio.to_string_lossy().into_owned(),
                },
                language_hints: Vec::new(),
                known_context: None,
            },
            &RunContext::detached(),
        )
        .await?;

    assert_eq!(transcript.meeting_id, "meeting_live_check");
    let first = transcript
        .segments
        .first()
        .ok_or_else(|| anyhow!("the live response produced no segments"))?;
    assert!(!first.speaker_id.is_empty());
    for pair in transcript.segments.windows(2) {
        assert!(
            pair[1].start_ms >= pair[0].end_ms,
            "the canonical timeline must stay ordered"
        );
    }

    println!(
        "live transcription check: OK ({} segments, scope {:?})",
        transcript.segments.len(),
        transcript.speaker_scope
    );
    Ok(())
}
