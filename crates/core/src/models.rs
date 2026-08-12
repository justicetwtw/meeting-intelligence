use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meeting {
    pub id: String,
    pub title: String,
    pub started_at: Option<String>,
    #[serde(default)]
    pub processing_state: MeetingProcessingState,
    #[serde(default)]
    pub artifact_ids: Vec<String>,
    #[serde(default)]
    pub participants: Vec<String>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeetingProcessingState {
    #[default]
    Pending,
    Processing,
    Completed,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessingJob {
    pub id: String,
    pub meeting_id: String,
    pub kind: ProcessingJobKind,
    pub status: ProcessingJobStatus,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    pub completed_at_ms: Option<u64>,
    pub error_category: Option<ProcessingJobErrorCategory>,
}

/// Stable, redacted categories safe to persist or return to an interface.
/// Provider messages can contain sensitive upstream data and are never stored.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessingJobErrorCategory {
    Provider,
    Storage,
    InvalidProviderOutput,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessingJobKind {
    MeetingPipeline,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessingJobStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Transcript {
    pub meeting_id: String,
    pub language: Option<String>,
    pub segments: Vec<TranscriptSegment>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TranscriptSegment {
    pub id: String,
    pub speaker_id: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Speaker {
    pub transcript_speaker_id: String,
    pub identity: SpeakerIdentityStatus,
    pub profile_id: Option<String>,
    pub display_name: Option<String>,
    pub confidence: Option<f32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeakerIdentityStatus {
    Resolved,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpeakerProfile {
    pub id: String,
    pub display_name: String,
    #[serde(default)]
    pub enrollment_references: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeetingAnalysis {
    pub meeting_id: String,
    pub summary: String,
    #[serde(default)]
    pub decisions: Vec<Decision>,
    #[serde(default)]
    pub action_items: Vec<ActionItem>,
    #[serde(default)]
    pub open_questions: Vec<OpenQuestion>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    pub id: String,
    pub statement: String,
    #[serde(default)]
    pub evidence_segment_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionItem {
    pub id: String,
    pub description: String,
    pub assignee_profile_id: Option<String>,
    pub due_at: Option<String>,
    #[serde(default)]
    pub evidence_segment_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenQuestion {
    pub id: String,
    pub question: String,
    #[serde(default)]
    pub evidence_segment_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    pub id: String,
    pub meeting_id: String,
    pub kind: ArtifactKind,
    pub media_type: String,
    pub content: String,
    pub created_at: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Markdown,
    Json,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AudioSource {
    Fixture { reference: String },
    LocalFile { path: String },
    RemoteObject { uri: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptionRequest {
    pub meeting: Meeting,
    pub audio_source: AudioSource,
    #[serde(default)]
    pub language_hints: Vec<String>,
    pub known_context: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpeakerIdentityRequest {
    pub transcript: Transcript,
    #[serde(default)]
    pub enrolled_profiles: Vec<SpeakerProfile>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IntelligenceRequest {
    pub meeting: Meeting,
    pub transcript: Transcript,
    #[serde(default)]
    pub speakers: Vec<Speaker>,
}
