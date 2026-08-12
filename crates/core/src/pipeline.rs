use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use tracing::{info, warn};

use crate::{
    AudioSource, CoreError, IntelligenceProvider, IntelligenceRequest, Meeting, MeetingAnalysis,
    MeetingProcessingState, ProcessingJob, ProcessingJobErrorCategory, ProcessingJobKind,
    ProcessingJobStatus, Speaker, SpeakerIdentityProvider, SpeakerIdentityRequest,
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

        self.storage.save_meeting(&request.meeting).await?;
        for profile in &request.speaker_profiles {
            self.storage.save_speaker_profile(profile).await?;
        }
        self.storage.save_job(&job).await?;
        info!(meeting_id, "started meeting pipeline");

        let result = self.process_inner(&request).await;
        match result {
            Ok((transcript, speakers, analysis)) => {
                self.storage.save_transcript(&transcript).await?;
                self.storage.save_speakers(&meeting_id, &speakers).await?;
                self.storage.save_analysis(&analysis).await?;
                job.status = ProcessingJobStatus::Completed;
                let completed_at_ms = unix_time_ms();
                job.updated_at_ms = completed_at_ms;
                job.completed_at_ms = Some(completed_at_ms);
                self.storage.save_job(&job).await?;
                request.meeting.processing_state = MeetingProcessingState::Completed;
                self.storage.save_meeting(&request.meeting).await?;
                info!(meeting_id, "completed meeting pipeline");

                Ok(ProcessedMeeting {
                    meeting: request.meeting,
                    job,
                    transcript,
                    speakers,
                    analysis,
                })
            }
            Err(error) => {
                job.status = ProcessingJobStatus::Failed;
                let completed_at_ms = unix_time_ms();
                job.updated_at_ms = completed_at_ms;
                job.completed_at_ms = Some(completed_at_ms);
                job.error_category = Some(error_category(&error));
                if let Err(storage_error) = self.storage.save_job(&job).await {
                    let category = error_category(&CoreError::Provider(storage_error));
                    warn!(
                        meeting_id,
                        error_category = ?category,
                        "could not persist failed job status"
                    );
                }
                request.meeting.processing_state = MeetingProcessingState::Failed;
                if let Err(storage_error) = self.storage.save_meeting(&request.meeting).await {
                    let category = error_category(&CoreError::Provider(storage_error));
                    warn!(
                        meeting_id,
                        error_category = ?category,
                        "could not persist failed meeting state"
                    );
                }
                Err(error)
            }
        }
    }

    async fn process_inner(
        &self,
        request: &ProcessMeetingRequest,
    ) -> Result<(Transcript, Vec<Speaker>, MeetingAnalysis), CoreError> {
        let transcript = self
            .transcription
            .transcribe(TranscriptionRequest {
                meeting: request.meeting.clone(),
                audio_source: request.audio_source.clone(),
                language_hints: request.language_hints.clone(),
                known_context: request.known_context.clone(),
            })
            .await?;
        validate_transcript(&request.meeting.id, &transcript)?;

        let speakers = self
            .speaker_identity
            .identify(SpeakerIdentityRequest {
                transcript: transcript.clone(),
                enrolled_profiles: request.speaker_profiles.clone(),
            })
            .await?;
        let speakers = normalize_speakers(
            speakers,
            &request.speaker_profiles,
            self.minimum_identity_confidence,
        );

        let analysis = self
            .intelligence
            .analyze(IntelligenceRequest {
                meeting: request.meeting.clone(),
                transcript: transcript.clone(),
                speakers: speakers.clone(),
            })
            .await?;
        validate_analysis(&request.meeting.id, &transcript, &analysis)?;

        Ok((transcript, speakers, analysis))
    }
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

fn error_category(error: &CoreError) -> ProcessingJobErrorCategory {
    match error {
        CoreError::InvalidProviderOutput(_) => ProcessingJobErrorCategory::InvalidProviderOutput,
        CoreError::Provider(crate::ProviderError::Failure { provider, .. })
            if matches!(*provider, "sqlite" | "storage") =>
        {
            ProcessingJobErrorCategory::Storage
        }
        CoreError::Provider(_) => ProcessingJobErrorCategory::Provider,
    }
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
