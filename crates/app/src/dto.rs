use meeting_intelligence_core::{
    AnalysisMode, Meeting, MeetingAnalysis, MeetingProcessingState, ProcessingJobErrorCategory,
    Speaker, SpeakerIdentityStatus, SpeakerScope, Transcript, METADATA_SOURCE_AUDIO_FILE_NAME,
};
use meeting_intelligence_credentials::CredentialStatus;
use serde::{Deserialize, Serialize};

use crate::{AppSettings, ProviderChoice, ThemeChoice};

/// Row in the meeting history list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingSummaryDto {
    pub id: String,
    pub title: String,
    pub started_at: Option<String>,
    pub processing_state: MeetingProcessingState,
    pub duration_ms: Option<u64>,
    pub duration_label: Option<String>,
    pub source_file_name: Option<String>,
    pub error_category: Option<ProcessingJobErrorCategory>,
}

impl MeetingSummaryDto {
    pub fn from_meeting(
        meeting: &Meeting,
        error_category: Option<ProcessingJobErrorCategory>,
    ) -> Self {
        Self {
            id: meeting.id.clone(),
            title: meeting.title.clone(),
            started_at: meeting.started_at.clone(),
            processing_state: meeting.processing_state.clone(),
            duration_ms: meeting.duration_ms,
            duration_label: meeting.duration_ms.map(format_duration),
            source_file_name: meeting
                .metadata
                .get(METADATA_SOURCE_AUDIO_FILE_NAME)
                .cloned(),
            error_category,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeakerDto {
    /// Canonical transcript speaker ID. Renaming never changes it.
    pub id: String,
    /// What the interface shows.
    pub label: String,
    pub renamed: bool,
    pub identity_resolved: bool,
    pub segment_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentDto {
    pub id: String,
    pub speaker_id: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub timestamp: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceItemDto {
    pub id: String,
    pub text: String,
    pub assignee: Option<String>,
    pub due_at: Option<String>,
    pub evidence_segment_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisDto {
    pub summary: String,
    pub decisions: Vec<EvidenceItemDto>,
    pub action_items: Vec<EvidenceItemDto>,
    pub open_questions: Vec<EvidenceItemDto>,
}

impl AnalysisDto {
    pub fn from_analysis(analysis: &MeetingAnalysis) -> Self {
        Self {
            summary: analysis.summary.clone(),
            decisions: analysis
                .decisions
                .iter()
                .map(|decision| EvidenceItemDto {
                    id: decision.id.clone(),
                    text: decision.statement.clone(),
                    assignee: None,
                    due_at: None,
                    evidence_segment_ids: decision.evidence_segment_ids.clone(),
                })
                .collect(),
            action_items: analysis
                .action_items
                .iter()
                .map(|item| EvidenceItemDto {
                    id: item.id.clone(),
                    text: item.description.clone(),
                    assignee: item.assignee_profile_id.clone(),
                    due_at: item.due_at.clone(),
                    evidence_segment_ids: item.evidence_segment_ids.clone(),
                })
                .collect(),
            open_questions: analysis
                .open_questions
                .iter()
                .map(|question| EvidenceItemDto {
                    id: question.id.clone(),
                    text: question.question.clone(),
                    assignee: None,
                    due_at: None,
                    evidence_segment_ids: question.evidence_segment_ids.clone(),
                })
                .collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingDetailDto {
    pub meeting: MeetingSummaryDto,
    pub speakers: Vec<SpeakerDto>,
    pub segments: Vec<SegmentDto>,
    pub analysis: Option<AnalysisDto>,
    pub speaker_scope: SpeakerScope,
    /// Present when speaker labels are only meaningful inside one part.
    pub speaker_scope_note: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialStatusDto {
    pub configured: bool,
    /// Non-reversible tail of the stored key, for example `…3f9a`.
    pub hint: Option<String>,
}

impl From<CredentialStatus> for CredentialStatusDto {
    fn from(status: CredentialStatus) -> Self {
        Self {
            configured: status.configured,
            hint: status.hint,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsDto {
    pub transcription_provider: ProviderChoice,
    pub intelligence_provider: ProviderChoice,
    pub analysis_mode: AnalysisMode,
    pub theme: ThemeChoice,
    pub output_directory: Option<String>,
    pub ffmpeg_directory: Option<String>,
    pub language_hint: Option<String>,
}

impl From<&AppSettings> for SettingsDto {
    fn from(settings: &AppSettings) -> Self {
        Self {
            transcription_provider: settings.transcription_provider,
            intelligence_provider: settings.intelligence_provider,
            analysis_mode: settings.analysis_mode,
            theme: settings.theme,
            output_directory: settings
                .output_directory
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned()),
            ffmpeg_directory: settings
                .ffmpeg_directory
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned()),
            language_hint: settings.language_hint.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStatusDto {
    /// `true` when the app still needs a credential before it can process a
    /// meeting with the configured providers.
    pub onboarding_required: bool,
    pub credential: CredentialStatusDto,
    pub settings: SettingsDto,
    pub supported_extensions: Vec<String>,
    /// `true` when this build contains the production OpenAI adapters.
    pub openai_available: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Markdown,
    Json,
    Docx,
}

/// Builds the interface view of one meeting.
pub fn meeting_detail(
    meeting: &Meeting,
    transcript: Option<&Transcript>,
    speakers: &[Speaker],
    analysis: Option<&MeetingAnalysis>,
    error_category: Option<ProcessingJobErrorCategory>,
) -> MeetingDetailDto {
    let segments = transcript
        .map(|value| value.segments.as_slice())
        .unwrap_or(&[]);
    let speaker_scope = transcript.map_or(SpeakerScope::Meeting, |value| value.speaker_scope);

    let speaker_dtos = speakers
        .iter()
        .map(|speaker| SpeakerDto {
            id: speaker.transcript_speaker_id.clone(),
            label: speaker.label(),
            renamed: speaker.manual_display_name.is_some(),
            identity_resolved: speaker.identity == SpeakerIdentityStatus::Resolved,
            segment_count: segments
                .iter()
                .filter(|segment| segment.speaker_id == speaker.transcript_speaker_id)
                .count(),
        })
        .collect();

    MeetingDetailDto {
        meeting: MeetingSummaryDto::from_meeting(meeting, error_category),
        speakers: speaker_dtos,
        segments: segments
            .iter()
            .map(|segment| SegmentDto {
                id: segment.id.clone(),
                speaker_id: segment.speaker_id.clone(),
                start_ms: segment.start_ms,
                end_ms: segment.end_ms,
                timestamp: meeting_intelligence_exporters::format_timestamp(segment.start_ms),
                text: segment.text.clone(),
            })
            .collect(),
        analysis: analysis.map(AnalysisDto::from_analysis),
        speaker_scope,
        speaker_scope_note: match speaker_scope {
            SpeakerScope::Meeting => None,
            SpeakerScope::Chunk => Some(
                "This recording was transcribed in several parts. Speaker labels are \
                 reliable within one part only."
                    .to_owned(),
            ),
        },
    }
}

pub fn format_duration(milliseconds: u64) -> String {
    meeting_intelligence_exporters::format_timestamp(milliseconds)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use meeting_intelligence_core::{Decision, TranscriptSegment};

    use super::*;

    fn meeting() -> Meeting {
        let mut metadata = BTreeMap::new();
        metadata.insert(
            METADATA_SOURCE_AUDIO_FILE_NAME.to_owned(),
            "bcrc-2026-08-12.m4a".to_owned(),
        );
        Meeting {
            id: "m1".to_owned(),
            title: "BCRC Meeting".to_owned(),
            started_at: Some("2026-08-12T09:00:00Z".to_owned()),
            processing_state: MeetingProcessingState::Completed,
            artifact_ids: Vec::new(),
            participants: Vec::new(),
            duration_ms: Some(5_254_000),
            metadata,
        }
    }

    fn transcript(scope: SpeakerScope) -> Transcript {
        Transcript {
            meeting_id: "m1".to_owned(),
            language: Some("zh".to_owned()),
            speaker_scope: scope,
            segments: vec![
                TranscriptSegment {
                    id: "seg_00001".to_owned(),
                    speaker_id: "speaker_01".to_owned(),
                    start_ms: 0,
                    end_ms: 4_000,
                    text: "one".to_owned(),
                },
                TranscriptSegment {
                    id: "seg_00002".to_owned(),
                    speaker_id: "speaker_02".to_owned(),
                    start_ms: 865_000,
                    end_ms: 883_000,
                    text: "two".to_owned(),
                },
                TranscriptSegment {
                    id: "seg_00003".to_owned(),
                    speaker_id: "speaker_02".to_owned(),
                    start_ms: 883_000,
                    end_ms: 890_000,
                    text: "three".to_owned(),
                },
            ],
        }
    }

    fn speakers() -> Vec<Speaker> {
        vec![
            Speaker {
                transcript_speaker_id: "speaker_01".to_owned(),
                identity: SpeakerIdentityStatus::Unknown,
                profile_id: None,
                display_name: None,
                confidence: None,
                manual_display_name: None,
            },
            Speaker {
                transcript_speaker_id: "speaker_02".to_owned(),
                identity: SpeakerIdentityStatus::Unknown,
                profile_id: None,
                display_name: None,
                confidence: None,
                manual_display_name: Some("Mr. Wang".to_owned()),
            },
        ]
    }

    #[test]
    fn a_history_row_shows_a_readable_duration_and_source_file() {
        let dto = MeetingSummaryDto::from_meeting(&meeting(), None);

        assert_eq!(dto.duration_label.as_deref(), Some("01:27:34"));
        assert_eq!(dto.source_file_name.as_deref(), Some("bcrc-2026-08-12.m4a"));
    }

    #[test]
    fn the_detail_view_keeps_canonical_ids_separate_from_labels() {
        let detail = meeting_detail(
            &meeting(),
            Some(&transcript(SpeakerScope::Meeting)),
            &speakers(),
            None,
            None,
        );

        assert_eq!(detail.speakers[0].id, "speaker_01");
        assert_eq!(detail.speakers[0].label, "Speaker 1");
        assert!(!detail.speakers[0].renamed);
        assert_eq!(detail.speakers[1].id, "speaker_02");
        assert_eq!(detail.speakers[1].label, "Mr. Wang");
        assert!(detail.speakers[1].renamed);
        assert_eq!(
            detail.segments[1].speaker_id, "speaker_02",
            "renaming must not rewrite the transcript speaker ID"
        );
    }

    #[test]
    fn speaker_rows_carry_their_segment_counts() {
        let detail = meeting_detail(
            &meeting(),
            Some(&transcript(SpeakerScope::Meeting)),
            &speakers(),
            None,
            None,
        );

        assert_eq!(detail.speakers[0].segment_count, 1);
        assert_eq!(detail.speakers[1].segment_count, 2);
    }

    #[test]
    fn a_chunked_transcript_carries_a_note_for_the_interface() {
        let detail = meeting_detail(
            &meeting(),
            Some(&transcript(SpeakerScope::Chunk)),
            &speakers(),
            None,
            None,
        );

        assert_eq!(detail.speaker_scope, SpeakerScope::Chunk);
        assert!(detail.speaker_scope_note.is_some());
    }

    #[test]
    fn a_meeting_without_a_transcript_still_renders() {
        let detail = meeting_detail(&meeting(), None, &[], None, None);

        assert!(detail.segments.is_empty());
        assert!(detail.analysis.is_none());
        assert_eq!(detail.speaker_scope_note, None);
    }

    #[test]
    fn analysis_evidence_reaches_the_interface_unchanged() {
        let analysis = MeetingAnalysis {
            meeting_id: "m1".to_owned(),
            summary: "s".to_owned(),
            decisions: vec![Decision {
                id: "decision_001".to_owned(),
                statement: "Ship it.".to_owned(),
                evidence_segment_ids: vec!["seg_00001".to_owned()],
            }],
            action_items: Vec::new(),
            open_questions: Vec::new(),
        };

        let dto = AnalysisDto::from_analysis(&analysis);

        assert_eq!(dto.decisions[0].evidence_segment_ids, vec!["seg_00001"]);
        assert_eq!(dto.decisions[0].text, "Ship it.");
    }

    #[test]
    fn dtos_serialize_with_the_camel_case_names_the_interface_expects() {
        let json = serde_json::to_string(&MeetingSummaryDto::from_meeting(&meeting(), None))
            .expect("summary should serialize");

        assert!(json.contains("\"processingState\""));
        assert!(json.contains("\"durationLabel\""));
        assert!(json.contains("\"sourceFileName\""));
    }
}
