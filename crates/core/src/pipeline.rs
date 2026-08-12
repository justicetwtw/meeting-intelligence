use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use tracing::{info, warn};

use crate::{
    AnalysisMode, AudioSource, CancellationToken, CoreError, IntelligenceProvider,
    IntelligenceRequest, Meeting, MeetingAnalysis, MeetingProcessingState, NoopProgressReporter,
    ProcessingJob, ProcessingJobKind, ProcessingJobStatus, ProcessingStage, ProgressReporter,
    RunContext, RunProgress, Speaker, SpeakerIdentityProvider, SpeakerIdentityRequest,
    SpeakerIdentityStatus, SpeakerProfile, StorageProvider, Transcript, TranscriptionProvider,
    TranscriptionRequest,
};

const DEFAULT_MINIMUM_IDENTITY_CONFIDENCE: f32 = 0.80;

#[derive(Clone, Debug)]
pub struct ProcessMeetingRequest {
    pub meeting: Meeting,
    pub audio_source: AudioSource,
    pub language_hints: Vec<String>,
    pub known_context: Option<String>,
    pub speaker_profiles: Vec<SpeakerProfile>,
    pub analysis_mode: AnalysisMode,
    pub cancellation: CancellationToken,
}

impl ProcessMeetingRequest {
    /// Minimal request used by fixtures, tests, and the offline runner.
    pub fn new(meeting: Meeting, audio_source: AudioSource) -> Self {
        Self {
            meeting,
            audio_source,
            language_hints: Vec::new(),
            known_context: None,
            speaker_profiles: Vec::new(),
            analysis_mode: AnalysisMode::default(),
            cancellation: CancellationToken::new(),
        }
    }

    pub fn with_language_hints(mut self, language_hints: Vec<String>) -> Self {
        self.language_hints = language_hints;
        self
    }

    pub fn with_known_context(mut self, known_context: Option<String>) -> Self {
        self.known_context = known_context;
        self
    }

    pub fn with_speaker_profiles(mut self, speaker_profiles: Vec<SpeakerProfile>) -> Self {
        self.speaker_profiles = speaker_profiles;
        self
    }

    pub fn with_analysis_mode(mut self, analysis_mode: AnalysisMode) -> Self {
        self.analysis_mode = analysis_mode;
        self
    }

    pub fn with_cancellation(mut self, cancellation: CancellationToken) -> Self {
        self.cancellation = cancellation;
        self
    }
}

#[derive(Clone, Debug)]
pub struct ProcessedMeeting {
    pub meeting: Meeting,
    pub job: ProcessingJob,
    pub transcript: Transcript,
    pub speakers: Vec<Speaker>,
    pub analysis: MeetingAnalysis,
}

pub struct MeetingCore {
    transcription: Arc<dyn TranscriptionProvider>,
    speaker_identity: Arc<dyn SpeakerIdentityProvider>,
    intelligence: Arc<dyn IntelligenceProvider>,
    storage: Arc<dyn StorageProvider>,
    progress: Arc<dyn ProgressReporter>,
    minimum_identity_confidence: f32,
}

impl MeetingCore {
    pub fn new(
        transcription: Arc<dyn TranscriptionProvider>,
        speaker_identity: Arc<dyn SpeakerIdentityProvider>,
        intelligence: Arc<dyn IntelligenceProvider>,
        storage: Arc<dyn StorageProvider>,
    ) -> Self {
        Self {
            transcription,
            speaker_identity,
            intelligence,
            storage,
            progress: Arc::new(NoopProgressReporter),
            minimum_identity_confidence: DEFAULT_MINIMUM_IDENTITY_CONFIDENCE,
        }
    }

    pub fn with_minimum_identity_confidence(mut self, confidence: f32) -> Self {
        self.minimum_identity_confidence = if confidence.is_finite() {
            confidence.clamp(0.0, 1.0)
        } else {
            DEFAULT_MINIMUM_IDENTITY_CONFIDENCE
        };
        self
    }

    pub fn with_progress_reporter(mut self, progress: Arc<dyn ProgressReporter>) -> Self {
        self.progress = progress;
        self
    }

    pub async fn process(
        &self,
        mut request: ProcessMeetingRequest,
    ) -> Result<ProcessedMeeting, CoreError> {
        request.meeting.processing_state = MeetingProcessingState::Processing;
        let meeting_id = request.meeting.id.clone();
        let started_at_ms = unix_time_ms();
        let mut job = ProcessingJob {
            id: format!("job-{meeting_id}"),
            meeting_id: meeting_id.clone(),
            kind: ProcessingJobKind::MeetingPipeline,
            status: ProcessingJobStatus::Running,
            created_at_ms: started_at_ms,
            updated_at_ms: started_at_ms,
            completed_at_ms: None,
            error_category: None,
        };
        let context = RunContext::new(
            RunProgress::new(self.progress.clone(), &meeting_id, &job.id),
            request.cancellation.clone(),
        );

        self.storage.save_meeting(&request.meeting).await?;
        for profile in &request.speaker_profiles {
            self.storage.save_speaker_profile(profile).await?;
        }
        self.storage.save_job(&job).await?;
        info!(meeting_id, "started meeting pipeline");

        let result = self.process_inner(&request, &context).await;
        match result {
            Ok((transcript, speakers, analysis)) => {
                context.progress.stage(ProcessingStage::Saving);
                self.storage.save_transcript(&transcript).await?;
                self.storage.save_speakers(&meeting_id, &speakers).await?;
                self.storage.save_analysis(&analysis).await?;
                job.status = ProcessingJobStatus::Completed;
                let completed_at_ms = unix_time_ms();
                job.updated_at_ms = completed_at_ms;
                job.completed_at_ms = Some(completed_at_ms);
                self.storage.save_job(&job).await?;
                request.meeting.processing_state = MeetingProcessingState::Completed;
                request.meeting.duration_ms = transcript_duration_ms(&transcript);
                self.storage.save_meeting(&request.meeting).await?;
                info!(meeting_id, "completed meeting pipeline");
                context.progress.stage(ProcessingStage::Completed);

                Ok(ProcessedMeeting {
                    meeting: request.meeting,
                    job,
                    transcript,
                    speakers,
                    analysis,
                })
            }
            Err(error) => {
                let category = error.category();
                job.status = ProcessingJobStatus::Failed;
                let completed_at_ms = unix_time_ms();
                job.updated_at_ms = completed_at_ms;
                job.completed_at_ms = Some(completed_at_ms);
                job.error_category = Some(category);
                if let Err(storage_error) = self.storage.save_job(&job).await {
                    warn!(
                        meeting_id,
                        error_category = ?storage_error.category(),
                        "could not persist failed job status"
                    );
                }
                request.meeting.processing_state = MeetingProcessingState::Failed;
                if let Err(storage_error) = self.storage.save_meeting(&request.meeting).await {
                    warn!(
                        meeting_id,
                        error_category = ?storage_error.category(),
                        "could not persist failed meeting state"
                    );
                }
                warn!(meeting_id, error_category = ?category, "meeting pipeline failed");
                context.progress.failed(category);
                Err(error)
            }
        }
    }

    async fn process_inner(
        &self,
        request: &ProcessMeetingRequest,
        context: &RunContext,
    ) -> Result<(Transcript, Vec<Speaker>, MeetingAnalysis), CoreError> {
        context.cancellation.check()?;
        let transcript = self
            .transcription
            .transcribe(
                TranscriptionRequest {
                    meeting: request.meeting.clone(),
                    audio_source: request.audio_source.clone(),
                    language_hints: request.language_hints.clone(),
                    known_context: request.known_context.clone(),
                },
                context,
            )
            .await?;
        validate_transcript(&request.meeting.id, &transcript)?;
        context.cancellation.check()?;

        let speakers = self
            .speaker_identity
            .identify(
                SpeakerIdentityRequest {
                    transcript: transcript.clone(),
                    enrolled_profiles: request.speaker_profiles.clone(),
                },
                context,
            )
            .await?;
        let speakers = normalize_speakers(
            speakers,
            &request.speaker_profiles,
            self.minimum_identity_confidence,
        );
        context.cancellation.check()?;

        context.progress.stage(ProcessingStage::Analyzing);
        let analysis = self
            .intelligence
            .analyze(
                IntelligenceRequest {
                    meeting: request.meeting.clone(),
                    transcript: transcript.clone(),
                    speakers: speakers.clone(),
                    analysis_mode: request.analysis_mode,
                },
                context,
            )
            .await?;
        validate_analysis(&request.meeting.id, &transcript, &analysis)?;
        context.cancellation.check()?;

        Ok((transcript, speakers, analysis))
    }
}

/// Whole-meeting length implied by the transcript, if any segment exists.
pub fn transcript_duration_ms(transcript: &Transcript) -> Option<u64> {
    transcript
        .segments
        .iter()
        .map(|segment| segment.end_ms)
        .max()
}

fn validate_transcript(meeting_id: &str, transcript: &Transcript) -> Result<(), CoreError> {
    if transcript.meeting_id != meeting_id {
        return Err(CoreError::InvalidProviderOutput(
            "transcript meeting_id does not match the requested meeting".to_owned(),
        ));
    }

    let mut segment_ids = HashSet::new();
    for segment in &transcript.segments {
        if segment.id.trim().is_empty() || !segment_ids.insert(segment.id.as_str()) {
            return Err(CoreError::InvalidProviderOutput(
                "transcript segment IDs must be non-empty and unique".to_owned(),
            ));
        }
        if segment.speaker_id.trim().is_empty() || segment.end_ms < segment.start_ms {
            return Err(CoreError::InvalidProviderOutput(format!(
                "transcript segment {} has an invalid speaker or time range",
                segment.id
            )));
        }
    }
    for pair in transcript.segments.windows(2) {
        let previous = &pair[0];
        let current = &pair[1];
        if current.start_ms < previous.end_ms {
            return Err(CoreError::InvalidProviderOutput(format!(
                "transcript segments {} and {} are out of order or overlap",
                previous.id, current.id
            )));
        }
    }
    Ok(())
}

fn normalize_speakers(
    mut speakers: Vec<Speaker>,
    enrolled_profiles: &[SpeakerProfile],
    minimum_confidence: f32,
) -> Vec<Speaker> {
    let enrolled: HashMap<&str, &SpeakerProfile> = enrolled_profiles
        .iter()
        .map(|profile| (profile.id.as_str(), profile))
        .collect();

    for speaker in &mut speakers {
        let canonical_profile = speaker
            .profile_id
            .as_deref()
            .and_then(|profile_id| enrolled.get(profile_id).copied());
        let confidence_is_sufficient = speaker
            .confidence
            .is_some_and(|confidence| confidence >= minimum_confidence);

        if speaker.identity != SpeakerIdentityStatus::Resolved
            || canonical_profile.is_none()
            || !confidence_is_sufficient
        {
            speaker.identity = SpeakerIdentityStatus::Unknown;
            speaker.profile_id = None;
            speaker.display_name = None;
        } else if let Some(profile) = canonical_profile {
            // Identity providers return candidates; the persisted profile is
            // authoritative for user-visible identity metadata.
            speaker.display_name = Some(profile.display_name.clone());
        }
    }
    speakers
}

fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}

fn validate_analysis(
    meeting_id: &str,
    transcript: &Transcript,
    analysis: &MeetingAnalysis,
) -> Result<(), CoreError> {
    if analysis.meeting_id != meeting_id {
        return Err(CoreError::InvalidProviderOutput(
            "analysis meeting_id does not match the requested meeting".to_owned(),
        ));
    }

    let known_segment_ids: HashSet<&str> = transcript
        .segments
        .iter()
        .map(|segment| segment.id.as_str())
        .collect();
    let evidence_ids = analysis
        .decisions
        .iter()
        .flat_map(|item| item.evidence_segment_ids.iter())
        .chain(
            analysis
                .action_items
                .iter()
                .flat_map(|item| item.evidence_segment_ids.iter()),
        )
        .chain(
            analysis
                .open_questions
                .iter()
                .flat_map(|item| item.evidence_segment_ids.iter()),
        );

    for evidence_id in evidence_ids {
        if !known_segment_ids.contains(evidence_id.as_str()) {
            return Err(CoreError::InvalidProviderOutput(format!(
                "analysis references unknown transcript segment {evidence_id}"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ProcessingJobErrorCategory, ProviderError, TranscriptSegment};

    #[test]
    fn duration_uses_the_last_segment_end() {
        let transcript = Transcript {
            meeting_id: "m".to_owned(),
            language: None,
            speaker_scope: crate::SpeakerScope::Meeting,
            segments: vec![
                TranscriptSegment {
                    id: "seg_00001".to_owned(),
                    speaker_id: "speaker_01".to_owned(),
                    start_ms: 0,
                    end_ms: 1_000,
                    text: "a".to_owned(),
                },
                TranscriptSegment {
                    id: "seg_00002".to_owned(),
                    speaker_id: "speaker_02".to_owned(),
                    start_ms: 1_000,
                    end_ms: 5_250,
                    text: "b".to_owned(),
                },
            ],
        };

        assert_eq!(transcript_duration_ms(&transcript), Some(5_250));
    }

    #[test]
    fn duration_is_unknown_for_an_empty_transcript() {
        let transcript = Transcript {
            meeting_id: "m".to_owned(),
            language: None,
            speaker_scope: crate::SpeakerScope::Meeting,
            segments: Vec::new(),
        };

        assert_eq!(transcript_duration_ms(&transcript), None);
    }

    #[test]
    fn error_categories_are_derived_from_the_typed_error() {
        assert_eq!(
            CoreError::Provider(ProviderError::quota("openai", "x")).category(),
            ProcessingJobErrorCategory::Quota
        );
        assert_eq!(
            CoreError::Provider(ProviderError::failure("sqlite", "x")).category(),
            ProcessingJobErrorCategory::Storage
        );
    }
}
