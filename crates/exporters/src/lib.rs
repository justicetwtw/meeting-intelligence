//! Offline, deterministic exports for processed meetings.

use std::{collections::HashMap, fmt::Write};

use meeting_intelligence_core::{
    Artifact, ArtifactKind, Meeting, MeetingAnalysis, ProcessedMeeting, Speaker,
    SpeakerIdentityStatus, Transcript,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

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

#[derive(Debug, Error)]
pub enum ExportError {
    #[error("could not serialize JSON artifact: {0}")]
    Json(#[from] serde_json::Error),
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
            created_at: None,
        })
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct MarkdownExporter;

impl ArtifactExporter for MarkdownExporter {
    fn export(&self, bundle: &ExportBundle) -> Result<Artifact, ExportError> {
        let mut output = String::new();
        let speaker_names: HashMap<&str, &str> = bundle
            .speakers
            .iter()
            .map(|speaker| {
                let name = if speaker.identity == SpeakerIdentityStatus::Resolved {
                    speaker.display_name.as_deref().unwrap_or("Unknown speaker")
                } else {
                    "Unknown speaker"
                };
                (speaker.transcript_speaker_id.as_str(), name)
            })
            .collect();

        let _ = writeln!(output, "# {}\n", bundle.meeting.title);
        let _ = writeln!(output, "## Summary\n\n{}\n", bundle.analysis.summary);

        output.push_str("## Decisions\n\n");
        if bundle.analysis.decisions.is_empty() {
            output.push_str("- None\n");
        } else {
            for decision in &bundle.analysis.decisions {
                let _ = writeln!(
                    output,
                    "- {} _(evidence: {})_",
                    decision.statement,
                    evidence(&decision.evidence_segment_ids)
                );
            }
        }

        output.push_str("\n## Action items\n\n");
        if bundle.analysis.action_items.is_empty() {
            output.push_str("- None\n");
        } else {
            for item in &bundle.analysis.action_items {
                let assignee = item.assignee_profile_id.as_deref().unwrap_or("unassigned");
                let _ = writeln!(
                    output,
                    "- {} — {} _(evidence: {})_",
                    assignee,
                    item.description,
                    evidence(&item.evidence_segment_ids)
                );
            }
        }

        output.push_str("\n## Open questions\n\n");
        if bundle.analysis.open_questions.is_empty() {
            output.push_str("- None\n");
        } else {
            for question in &bundle.analysis.open_questions {
                let _ = writeln!(
                    output,
                    "- {} _(evidence: {})_",
                    question.question,
                    evidence(&question.evidence_segment_ids)
                );
            }
        }

        output.push_str("\n## Transcript\n\n");
        for segment in &bundle.transcript.segments {
            let speaker = speaker_names
                .get(segment.speaker_id.as_str())
                .copied()
                .unwrap_or("Unknown speaker");
            let _ = writeln!(
                output,
                "- `{}` {} ({}–{} ms): {}",
                segment.id, speaker, segment.start_ms, segment.end_ms, segment.text
            );
        }

        Ok(Artifact {
            id: format!("artifact-{}-markdown", bundle.meeting.id),
            meeting_id: bundle.meeting.id.clone(),
            kind: ArtifactKind::Markdown,
            media_type: "text/markdown; charset=utf-8".to_owned(),
            content: output,
            created_at: None,
        })
    }
}

fn evidence(ids: &[String]) -> String {
    if ids.is_empty() {
        "none".to_owned()
    } else {
        ids.join(", ")
    }
}
