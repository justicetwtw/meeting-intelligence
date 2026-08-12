//! Offline, deterministic exports for processed meetings.

mod docx;
mod markdown;

use meeting_intelligence_core::{
    Artifact, ArtifactEncoding, ArtifactKind, Meeting, MeetingAnalysis, ProcessedMeeting, Speaker,
    SpeakerScope, Transcript,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use docx::DocxExporter;
pub use markdown::MarkdownExporter;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExportBundle {
    pub meeting: Meeting,
    pub transcript: Transcript,
    pub speakers: Vec<Speaker>,
    pub analysis: MeetingAnalysis,
}

impl From<&ProcessedMeeting> for ExportBundle {
    fn from(processed: &ProcessedMeeting) -> Self {
        Self {
            meeting: processed.meeting.clone(),
            transcript: processed.transcript.clone(),
            speakers: processed.speakers.clone(),
            analysis: processed.analysis.clone(),
        }
    }
}

impl ExportBundle {
    /// Meeting-local label for a transcript speaker ID. Falls back to a stable
    /// positional label when no speaker record exists.
    pub fn speaker_label(&self, transcript_speaker_id: &str) -> String {
        self.speakers
            .iter()
            .find(|speaker| speaker.transcript_speaker_id == transcript_speaker_id)
            .map_or_else(
                || meeting_intelligence_core::fallback_speaker_label(transcript_speaker_id),
                Speaker::label,
            )
    }

    /// Present when speaker labels are only trustworthy inside one part of the
    /// recording, so every rendering can say so instead of implying continuity.
    pub fn speaker_scope_note(&self) -> Option<&'static str> {
        match self.transcript.speaker_scope {
            SpeakerScope::Meeting => None,
            SpeakerScope::Chunk => Some(
                "This recording was transcribed in several parts. Speaker labels are \
                 reliable within one part only; the same label in a different part may \
                 be a different person.",
            ),
        }
    }
}

#[derive(Debug, Error)]
pub enum ExportError {
    #[error("could not serialize JSON artifact: {0}")]
    Json(#[from] serde_json::Error),
    #[error("could not build the Word document")]
    Docx,
}

pub trait ArtifactExporter {
    fn export(&self, bundle: &ExportBundle) -> Result<Artifact, ExportError>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct JsonExporter;

impl ArtifactExporter for JsonExporter {
    fn export(&self, bundle: &ExportBundle) -> Result<Artifact, ExportError> {
        Ok(Artifact {
            id: format!("artifact-{}-json", bundle.meeting.id),
            meeting_id: bundle.meeting.id.clone(),
            kind: ArtifactKind::Json,
            media_type: "application/json".to_owned(),
            content: serde_json::to_string_pretty(bundle)?,
            encoding: ArtifactEncoding::Utf8,
            created_at: None,
        })
    }
}

/// `HH:MM:SS`, shared by every renderer so exports agree with the interface.
pub fn format_timestamp(milliseconds: u64) -> String {
    let total_seconds = milliseconds / 1_000;
    format!(
        "{:02}:{:02}:{:02}",
        total_seconds / 3_600,
        (total_seconds % 3_600) / 60,
        total_seconds % 60
    )
}

pub(crate) fn evidence(ids: &[String]) -> String {
    if ids.is_empty() {
        "none".to_owned()
    } else {
        ids.join(", ")
    }
}

pub(crate) fn assignee_label(assignee_profile_id: Option<&str>) -> &str {
    assignee_profile_id.unwrap_or("unassigned")
}

pub(crate) fn due_label(due_at: Option<&str>) -> &str {
    due_at.unwrap_or("no due date")
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::collections::BTreeMap;

    use meeting_intelligence_core::{
        ActionItem, Decision, MeetingProcessingState, OpenQuestion, SpeakerIdentityStatus,
        TranscriptSegment,
    };

    use super::*;

    pub fn bundle() -> ExportBundle {
        ExportBundle {
            meeting: Meeting {
                id: "meeting_fixture_001".to_owned(),
                title: "Bootstrap planning".to_owned(),
                started_at: Some("2026-08-12T09:00:00Z".to_owned()),
                processing_state: MeetingProcessingState::Completed,
                artifact_ids: Vec::new(),
                participants: vec!["Alice".to_owned(), "Guest".to_owned()],
                duration_ms: Some(12_600),
                metadata: BTreeMap::new(),
            },
            transcript: Transcript {
                meeting_id: "meeting_fixture_001".to_owned(),
                language: Some("en".to_owned()),
                speaker_scope: SpeakerScope::Meeting,
                segments: vec![
                    TranscriptSegment {
                        id: "seg_001".to_owned(),
                        speaker_id: "speaker_01".to_owned(),
                        start_ms: 0,
                        end_ms: 4_200,
                        text: "Keep the pipeline deterministic.".to_owned(),
                    },
                    TranscriptSegment {
                        id: "seg_002".to_owned(),
                        speaker_id: "speaker_02".to_owned(),
                        start_ms: 4_300,
                        end_ms: 8_100,
                        text: "Can we export Markdown and JSON?".to_owned(),
                    },
                ],
            },
            speakers: vec![
                Speaker {
                    transcript_speaker_id: "speaker_01".to_owned(),
                    identity: SpeakerIdentityStatus::Resolved,
                    profile_id: Some("profile_alice".to_owned()),
                    display_name: Some("Alice".to_owned()),
                    confidence: Some(0.97),
                    manual_display_name: None,
                },
                Speaker {
                    transcript_speaker_id: "speaker_02".to_owned(),
                    identity: SpeakerIdentityStatus::Unknown,
                    profile_id: None,
                    display_name: None,
                    confidence: Some(0.42),
                    manual_display_name: None,
                },
            ],
            analysis: MeetingAnalysis {
                meeting_id: "meeting_fixture_001".to_owned(),
                summary: "The team agreed to keep processing offline.".to_owned(),
                decisions: vec![Decision {
                    id: "decision_001".to_owned(),
                    statement: "Use fixtures & mocks only.".to_owned(),
                    evidence_segment_ids: vec!["seg_001".to_owned()],
                }],
                action_items: vec![ActionItem {
                    id: "action_001".to_owned(),
                    description: "Verify both artifacts.".to_owned(),
                    assignee_profile_id: Some("profile_alice".to_owned()),
                    due_at: None,
                    evidence_segment_ids: vec!["seg_002".to_owned()],
                }],
                open_questions: vec![OpenQuestion {
                    id: "question_001".to_owned(),
                    question: "Which provider ships first?".to_owned(),
                    evidence_segment_ids: vec!["seg_001".to_owned()],
                }],
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{test_support::bundle, *};

    #[test]
    fn json_export_round_trips_and_stays_utf8() -> Result<(), ExportError> {
        let bundle = bundle();
        let artifact = JsonExporter.export(&bundle)?;

        assert_eq!(artifact.encoding, ArtifactEncoding::Utf8);
        let decoded: ExportBundle = serde_json::from_str(&artifact.content)?;
        assert_eq!(decoded, bundle);
        Ok(())
    }

    #[test]
    fn timestamps_render_as_hours_minutes_seconds() {
        assert_eq!(format_timestamp(0), "00:00:00");
        assert_eq!(format_timestamp(865_000), "00:14:25");
        assert_eq!(format_timestamp(5_247_000), "01:27:27");
    }

    #[test]
    fn an_unnamed_speaker_never_borrows_a_provider_candidate_name() {
        let bundle = bundle();

        assert_eq!(bundle.speaker_label("speaker_01"), "Alice");
        assert_eq!(bundle.speaker_label("speaker_02"), "Speaker 2");
        assert_eq!(bundle.speaker_label("speaker_09"), "Speaker 9");
    }

    #[test]
    fn a_manual_rename_is_used_by_every_export() {
        let mut bundle = bundle();
        bundle.speakers[1].manual_display_name = Some("Mr. Wang".to_owned());

        assert_eq!(bundle.speaker_label("speaker_02"), "Mr. Wang");
    }

    #[test]
    fn a_chunked_transcript_carries_a_scope_warning() {
        let mut bundle = bundle();
        assert_eq!(bundle.speaker_scope_note(), None);

        bundle.transcript.speaker_scope = SpeakerScope::Chunk;
        assert!(bundle
            .speaker_scope_note()
            .expect("a chunked transcript must warn")
            .contains("reliable within one part only"));
    }
}
