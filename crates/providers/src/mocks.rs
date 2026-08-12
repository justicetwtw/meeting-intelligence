use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
};

use async_trait::async_trait;
use meeting_intelligence_core::{
    Artifact, IntelligenceProvider, IntelligenceRequest, Meeting, MeetingAnalysis, ProcessingJob,
    ProviderError, Speaker, SpeakerIdentityProvider, SpeakerIdentityRequest, SpeakerIdentityStatus,
    SpeakerProfile, StorageProvider, Transcript, TranscriptionProvider, TranscriptionRequest,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FixtureLoadError {
    #[error("could not read fixture {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("could not parse fixture {path}: {source}")]
    Parse {
        path: String,
        #[source]
        source: serde_json::Error,
    },
}

fn load_json<T: DeserializeOwned>(path: &Path) -> Result<T, FixtureLoadError> {
    let display_path = path.display().to_string();
    let bytes = fs::read(path).map_err(|source| FixtureLoadError::Read {
        path: display_path.clone(),
        source,
    })?;
    serde_json::from_slice(&bytes).map_err(|source| FixtureLoadError::Parse {
        path: display_path,
        source,
    })
}

#[derive(Clone, Debug)]
pub struct FixtureTranscriptionProvider {
    transcript: Transcript,
}

impl FixtureTranscriptionProvider {
    pub fn new(transcript: Transcript) -> Self {
        Self { transcript }
    }

    pub fn from_json_path(path: impl AsRef<Path>) -> Result<Self, FixtureLoadError> {
        load_json(path.as_ref()).map(Self::new)
    }
}

#[async_trait]
impl TranscriptionProvider for FixtureTranscriptionProvider {
    async fn transcribe(
        &self,
        _request: TranscriptionRequest,
    ) -> Result<Transcript, ProviderError> {
        Ok(self.transcript.clone())
    }
}

#[derive(Clone, Debug)]
pub struct MockIntelligenceProvider {
    analysis: MeetingAnalysis,
}

impl MockIntelligenceProvider {
    pub fn new(analysis: MeetingAnalysis) -> Self {
        Self { analysis }
    }

    pub fn from_json_path(path: impl AsRef<Path>) -> Result<Self, FixtureLoadError> {
        load_json(path.as_ref()).map(Self::new)
    }
}

#[async_trait]
impl IntelligenceProvider for MockIntelligenceProvider {
    async fn analyze(
        &self,
        _request: IntelligenceRequest,
    ) -> Result<MeetingAnalysis, ProviderError> {
        Ok(self.analysis.clone())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CandidateIdentity {
    pub profile_id: String,
    pub display_name: String,
    pub confidence: f32,
}

#[derive(Clone, Debug)]
pub struct MockSpeakerIdentityProvider {
    candidates: HashMap<String, CandidateIdentity>,
}

impl MockSpeakerIdentityProvider {
    pub fn new(candidates: HashMap<String, CandidateIdentity>) -> Self {
        Self { candidates }
    }

    pub fn from_json_path(path: impl AsRef<Path>) -> Result<Self, FixtureLoadError> {
        load_json(path.as_ref()).map(Self::new)
    }
}

#[async_trait]
impl SpeakerIdentityProvider for MockSpeakerIdentityProvider {
    async fn identify(
        &self,
        request: SpeakerIdentityRequest,
    ) -> Result<Vec<Speaker>, ProviderError> {
        let enrolled_profile_ids: HashSet<&str> = request
            .enrolled_profiles
            .iter()
            .map(|profile| profile.id.as_str())
            .collect();
        let mut seen_speakers = HashSet::new();
        let mut speakers = Vec::new();

        for segment in &request.transcript.segments {
            if !seen_speakers.insert(segment.speaker_id.as_str()) {
                continue;
            }

            let candidate = self.candidates.get(&segment.speaker_id);
            let is_resolved = candidate.is_some_and(|candidate| {
                enrolled_profile_ids.contains(candidate.profile_id.as_str())
            });

            speakers.push(match (candidate, is_resolved) {
                (Some(candidate), true) => Speaker {
                    transcript_speaker_id: segment.speaker_id.clone(),
                    identity: SpeakerIdentityStatus::Resolved,
                    profile_id: Some(candidate.profile_id.clone()),
                    display_name: Some(candidate.display_name.clone()),
                    confidence: Some(candidate.confidence),
                },
                (candidate, _) => Speaker {
                    transcript_speaker_id: segment.speaker_id.clone(),
                    identity: SpeakerIdentityStatus::Unknown,
                    profile_id: None,
                    display_name: None,
                    confidence: candidate.map(|candidate| candidate.confidence),
                },
            });
        }

        Ok(speakers)
    }
}

#[derive(Default)]
struct InMemoryState {
    meetings: HashMap<String, Meeting>,
    jobs: HashMap<String, ProcessingJob>,
    transcripts: HashMap<String, Transcript>,
    speakers: HashMap<String, Vec<Speaker>>,
    speaker_profiles: HashMap<String, SpeakerProfile>,
    analyses: HashMap<String, MeetingAnalysis>,
    artifacts: HashMap<String, Artifact>,
}

#[derive(Clone, Default)]
pub struct InMemoryStorageProvider {
    state: Arc<RwLock<InMemoryState>>,
}

impl InMemoryStorageProvider {
    fn read_state(&self) -> Result<RwLockReadGuard<'_, InMemoryState>, ProviderError> {
        self.state
            .read()
            .map_err(|_| ProviderError::failure("storage", "in-memory read lock was poisoned"))
    }

    fn write_state(&self) -> Result<RwLockWriteGuard<'_, InMemoryState>, ProviderError> {
        self.state
            .write()
            .map_err(|_| ProviderError::failure("storage", "in-memory write lock was poisoned"))
    }
}

#[async_trait]
impl StorageProvider for InMemoryStorageProvider {
    async fn save_meeting(&self, meeting: &Meeting) -> Result<(), ProviderError> {
        self.write_state()?
            .meetings
            .insert(meeting.id.clone(), meeting.clone());
        Ok(())
    }

    async fn load_meeting(&self, meeting_id: &str) -> Result<Option<Meeting>, ProviderError> {
        Ok(self.read_state()?.meetings.get(meeting_id).cloned())
    }

    async fn save_job(&self, job: &ProcessingJob) -> Result<(), ProviderError> {
        self.write_state()?.jobs.insert(job.id.clone(), job.clone());
        Ok(())
    }

    async fn load_job(&self, job_id: &str) -> Result<Option<ProcessingJob>, ProviderError> {
        Ok(self.read_state()?.jobs.get(job_id).cloned())
    }

    async fn save_transcript(&self, transcript: &Transcript) -> Result<(), ProviderError> {
        self.write_state()?
            .transcripts
            .insert(transcript.meeting_id.clone(), transcript.clone());
        Ok(())
    }

    async fn load_transcript(&self, meeting_id: &str) -> Result<Option<Transcript>, ProviderError> {
        Ok(self.read_state()?.transcripts.get(meeting_id).cloned())
    }

    async fn save_speakers(
        &self,
        meeting_id: &str,
        speakers: &[Speaker],
    ) -> Result<(), ProviderError> {
        self.write_state()?
            .speakers
            .insert(meeting_id.to_owned(), speakers.to_vec());
        Ok(())
    }

    async fn load_speakers(&self, meeting_id: &str) -> Result<Vec<Speaker>, ProviderError> {
        Ok(self
            .read_state()?
            .speakers
            .get(meeting_id)
            .cloned()
            .unwrap_or_default())
    }

    async fn save_speaker_profile(&self, profile: &SpeakerProfile) -> Result<(), ProviderError> {
        self.write_state()?
            .speaker_profiles
            .insert(profile.id.clone(), profile.clone());
        Ok(())
    }

    async fn load_speaker_profile(
        &self,
        profile_id: &str,
    ) -> Result<Option<SpeakerProfile>, ProviderError> {
        Ok(self.read_state()?.speaker_profiles.get(profile_id).cloned())
    }

    async fn save_analysis(&self, analysis: &MeetingAnalysis) -> Result<(), ProviderError> {
        self.write_state()?
            .analyses
            .insert(analysis.meeting_id.clone(), analysis.clone());
        Ok(())
    }

    async fn load_analysis(
        &self,
        meeting_id: &str,
    ) -> Result<Option<MeetingAnalysis>, ProviderError> {
        Ok(self.read_state()?.analyses.get(meeting_id).cloned())
    }

    async fn save_artifact(&self, artifact: &Artifact) -> Result<(), ProviderError> {
        self.write_state()?
            .artifacts
            .insert(artifact.id.clone(), artifact.clone());
        Ok(())
    }

    async fn load_artifact(&self, artifact_id: &str) -> Result<Option<Artifact>, ProviderError> {
        Ok(self.read_state()?.artifacts.get(artifact_id).cloned())
    }
}
