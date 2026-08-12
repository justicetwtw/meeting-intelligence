use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Metadata key recording where the original recording was imported from.
pub const METADATA_SOURCE_AUDIO_PATH: &str = "source_audio_path";
/// Metadata key recording the original recording file name.
pub const METADATA_SOURCE_AUDIO_FILE_NAME: &str = "source_audio_file_name";
/// Metadata key recording which analysis mode produced the stored analysis.
pub const METADATA_ANALYSIS_MODE: &str = "analysis_mode";
/// Metadata key recording which transcription provider produced the transcript.
pub const METADATA_TRANSCRIPTION_PROVIDER: &str = "transcription_provider";
/// Metadata key recording which intelligence provider produced the analysis.
pub const METADATA_INTELLIGENCE_PROVIDER: &str = "intelligence_provider";

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
    /// Total recording length once it is known. Additive and optional so
    /// meetings persisted before duration tracking still deserialize.
    #[serde(default)]
    pub duration_ms: Option<u64>,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessingJobErrorCategory {
    Provider,
    Storage,
    InvalidProviderOutput,
    /// The configured credential was missing, rejected, or revoked.
    Credential,
    /// The account has no remaining quota or was rate limited beyond retries.
    Quota,
    /// Transport-level failure reaching the provider.
    Network,
    /// The recording could not be read, decoded, or prepared for upload.
    Audio,
    /// The provider or runtime is not configured for the requested operation.
    Configuration,
    /// The operator cancelled the run.
    Canceled,
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
    /// How far a transcript speaker ID can be trusted to mean one real person.
    /// Additive with a default so previously stored transcripts still load.
    #[serde(default)]
    pub speaker_scope: SpeakerScope,
}

/// Speaker IDs are always stable *within* their scope. `Chunk` means the
/// transcript was assembled from several independently diarized uploads, so two
/// chunk-scoped IDs are never silently treated as the same person.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeakerScope {
    #[default]
    Meeting,
    Chunk,
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
    /// Operator-supplied label for this meeting-local speaker. It never
    /// replaces `transcript_speaker_id` and never implies a resolved identity.
    #[serde(default)]
    pub manual_display_name: Option<String>,
}

impl Speaker {
    /// Meeting-local label rendered by interfaces and exporters. The manual
    /// label wins because an operator naming a voice is stronger evidence than
    /// a provider candidate, but neither changes the canonical speaker ID.
    pub fn label(&self) -> String {
        if let Some(name) = trimmed(self.manual_display_name.as_deref()) {
            return name.to_owned();
        }
        if self.identity == SpeakerIdentityStatus::Resolved {
            if let Some(name) = trimmed(self.display_name.as_deref()) {
                return name.to_owned();
            }
        }
        fallback_speaker_label(&self.transcript_speaker_id)
    }
}

fn trimmed(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

/// Human-readable fallback for a speaker that has no name yet. Chunk-scoped IDs
/// keep their chunk marker so an operator can see that two chunks were diarized
/// separately.
pub fn fallback_speaker_label(transcript_speaker_id: &str) -> String {
    let mut chunk: Option<&str> = None;
    let mut index: Option<&str> = None;
    for part in transcript_speaker_id.split('_') {
        if let Some(rest) = part.strip_prefix('c') {
            if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()) {
                chunk = Some(rest);
                continue;
            }
        }
        if !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()) {
            index = Some(part);
        }
    }

    match (chunk, index) {
        (Some(chunk), Some(index)) => format!(
            "Speaker {} (part {})",
            strip_leading_zeros(index),
            strip_leading_zeros(chunk)
        ),
        (None, Some(index)) => format!("Speaker {}", strip_leading_zeros(index)),
        _ => transcript_speaker_id.to_owned(),
    }
}

fn strip_leading_zeros(value: &str) -> &str {
    let trimmed = value.trim_start_matches('0');
    if trimmed.is_empty() {
        "0"
    } else {
        trimmed
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
    /// How `content` is encoded. Text artifacts stay human-readable; binary
    /// artifacts such as Word documents are Base64 so the canonical artifact
    /// shape does not change.
    #[serde(default)]
    pub encoding: ArtifactEncoding,
    pub created_at: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Markdown,
    Json,
    Docx,
}

impl ArtifactKind {
    pub fn file_extension(self) -> &'static str {
        match self {
            Self::Markdown => "md",
            Self::Json => "json",
            Self::Docx => "docx",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactEncoding {
    #[default]
    Utf8,
    Base64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AudioSource {
    Fixture { reference: String },
    LocalFile { path: String },
    RemoteObject { uri: String },
}

/// Analysis depth requested by the operator. Interfaces expose these three
/// presets; provider-specific reasoning settings are resolved from
/// configuration so no interface hard-codes a vendor parameter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisMode {
    Fast,
    #[default]
    Standard,
    Deep,
}

impl AnalysisMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fast => "fast",
            Self::Standard => "standard",
            Self::Deep => "deep",
        }
    }

    pub fn from_str_opt(value: &str) -> Option<Self> {
        match value {
            "fast" => Some(Self::Fast),
            "standard" => Some(Self::Standard),
            "deep" => Some(Self::Deep),
            _ => None,
        }
    }
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
    #[serde(default)]
    pub analysis_mode: AnalysisMode,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn speaker(id: &str) -> Speaker {
        Speaker {
            transcript_speaker_id: id.to_owned(),
            identity: SpeakerIdentityStatus::Unknown,
            profile_id: None,
            display_name: None,
            confidence: None,
            manual_display_name: None,
        }
    }

    #[test]
    fn meeting_scoped_ids_render_a_plain_speaker_label() {
        assert_eq!(speaker("speaker_02").label(), "Speaker 2");
    }

    #[test]
    fn chunk_scoped_ids_keep_their_chunk_marker() {
        assert_eq!(speaker("speaker_c03_02").label(), "Speaker 2 (part 3)");
    }

    #[test]
    fn unrecognized_ids_fall_back_to_the_raw_identifier() {
        assert_eq!(speaker("guest-voice").label(), "guest-voice");
    }

    #[test]
    fn manual_label_wins_over_a_resolved_profile_name() {
        let mut value = speaker("speaker_01");
        value.identity = SpeakerIdentityStatus::Resolved;
        value.display_name = Some("Alice".to_owned());
        value.manual_display_name = Some("  Mr. Wang  ".to_owned());

        assert_eq!(value.label(), "Mr. Wang");
    }

    #[test]
    fn blank_manual_label_is_ignored() {
        let mut value = speaker("speaker_01");
        value.manual_display_name = Some("   ".to_owned());

        assert_eq!(value.label(), "Speaker 1");
    }

    #[test]
    fn unresolved_identity_never_shows_a_provider_candidate_name() {
        let mut value = speaker("speaker_01");
        value.display_name = Some("Alice".to_owned());

        assert_eq!(value.label(), "Speaker 1");
    }

    #[test]
    fn additive_fields_default_when_absent_from_stored_json() {
        let transcript: Transcript =
            serde_json::from_str(r#"{"meeting_id":"m1","language":"en","segments":[]}"#)
                .expect("legacy transcript payload should still deserialize");
        assert_eq!(transcript.speaker_scope, SpeakerScope::Meeting);

        let meeting: Meeting = serde_json::from_str(r#"{"id":"m1","title":"t","started_at":null}"#)
            .expect("legacy meeting payload should still deserialize");
        assert_eq!(meeting.duration_ms, None);

        let stored: Speaker = serde_json::from_str(
            r#"{"transcript_speaker_id":"speaker_01","identity":"unknown",
                "profile_id":null,"display_name":null,"confidence":null}"#,
        )
        .expect("legacy speaker payload should still deserialize");
        assert_eq!(stored.manual_display_name, None);

        let artifact: Artifact = serde_json::from_str(
            r#"{"id":"a1","meeting_id":"m1","kind":"markdown",
                "media_type":"text/markdown","content":"Title","created_at":null}"#,
        )
        .expect("legacy artifact payload should still deserialize");
        assert_eq!(artifact.encoding, ArtifactEncoding::Utf8);
    }

    #[test]
    fn analysis_mode_round_trips_through_its_stable_string() {
        for mode in [
            AnalysisMode::Fast,
            AnalysisMode::Standard,
            AnalysisMode::Deep,
        ] {
            assert_eq!(AnalysisMode::from_str_opt(mode.as_str()), Some(mode));
        }
        assert_eq!(AnalysisMode::from_str_opt("turbo"), None);
    }
}
