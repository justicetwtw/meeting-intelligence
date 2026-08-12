use async_trait::async_trait;

use crate::{
    Artifact, CancellationToken, IntelligenceRequest, Meeting, MeetingAnalysis, ProcessingJob,
    ProviderError, RunProgress, Speaker, SpeakerIdentityRequest, SpeakerProfile, Transcript,
    TranscriptionRequest,
};

/// Per-run collaboration channel handed to providers.
///
/// It carries only orchestration concerns — stage reporting and cooperative
/// cancellation — so providers stay stateless and reusable across runs.
#[derive(Clone, Debug)]
pub struct RunContext {
    pub progress: RunProgress,
    pub cancellation: CancellationToken,
}

impl RunContext {
    pub fn new(progress: RunProgress, cancellation: CancellationToken) -> Self {
        Self {
            progress,
            cancellation,
        }
    }

    /// Context used by tests and by interfaces that observe no progress.
    pub fn detached() -> Self {
        Self::new(RunProgress::disabled(), CancellationToken::new())
    }
}

impl Default for RunContext {
    fn default() -> Self {
        Self::detached()
    }
}

#[async_trait]
pub trait TranscriptionProvider: Send + Sync {
    async fn transcribe(
        &self,
        request: TranscriptionRequest,
        context: &RunContext,
    ) -> Result<Transcript, ProviderError>;
}

#[async_trait]
pub trait SpeakerIdentityProvider: Send + Sync {
    async fn identify(
        &self,
        request: SpeakerIdentityRequest,
        context: &RunContext,
    ) -> Result<Vec<Speaker>, ProviderError>;
}

#[async_trait]
pub trait IntelligenceProvider: Send + Sync {
    async fn analyze(
        &self,
        request: IntelligenceRequest,
        context: &RunContext,
    ) -> Result<MeetingAnalysis, ProviderError>;
}

/// Persistence port used by Meeting Core. UI layers should call application
/// services/native commands and must not couple themselves to SQLite.
#[async_trait]
pub trait StorageProvider: Send + Sync {
    async fn save_meeting(&self, meeting: &Meeting) -> Result<(), ProviderError>;
    async fn load_meeting(&self, meeting_id: &str) -> Result<Option<Meeting>, ProviderError>;
    /// Meeting history, most recently updated first. Ordering must be stable so
    /// interfaces can render a deterministic list.
    async fn list_meetings(&self) -> Result<Vec<Meeting>, ProviderError>;

    async fn save_job(&self, job: &ProcessingJob) -> Result<(), ProviderError>;
    async fn load_job(&self, job_id: &str) -> Result<Option<ProcessingJob>, ProviderError>;

    async fn save_transcript(&self, transcript: &Transcript) -> Result<(), ProviderError>;
    async fn load_transcript(&self, meeting_id: &str) -> Result<Option<Transcript>, ProviderError>;

    async fn save_speakers(
        &self,
        meeting_id: &str,
        speakers: &[Speaker],
    ) -> Result<(), ProviderError>;
    async fn load_speakers(&self, meeting_id: &str) -> Result<Vec<Speaker>, ProviderError>;

    async fn save_speaker_profile(&self, profile: &SpeakerProfile) -> Result<(), ProviderError>;
    async fn load_speaker_profile(
        &self,
        profile_id: &str,
    ) -> Result<Option<SpeakerProfile>, ProviderError>;

    async fn save_analysis(&self, analysis: &MeetingAnalysis) -> Result<(), ProviderError>;
    async fn load_analysis(
        &self,
        meeting_id: &str,
    ) -> Result<Option<MeetingAnalysis>, ProviderError>;

    async fn save_artifact(&self, artifact: &Artifact) -> Result<(), ProviderError>;
    async fn load_artifact(&self, artifact_id: &str) -> Result<Option<Artifact>, ProviderError>;
}
