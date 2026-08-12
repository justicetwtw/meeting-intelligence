use std::fmt::Write;

use meeting_intelligence_core::{Artifact, ArtifactEncoding, ArtifactKind};

use crate::{
    assignee_label, due_label, evidence, format_timestamp, ArtifactExporter, ExportBundle,
    ExportError,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct MarkdownExporter;

impl ArtifactExporter for MarkdownExporter {
    fn export(&self, bundle: &ExportBundle) -> Result<Artifact, ExportError> {
        let mut output = String::new();

        let _ = writeln!(output, "# {}\n", bundle.meeting.title);
        if let Some(started_at) = &bundle.meeting.started_at {
            let _ = writeln!(output, "- Date: {started_at}");
        }
        if let Some(duration_ms) = bundle.meeting.duration_ms {
            let _ = writeln!(output, "- Duration: {}", format_timestamp(duration_ms));
        }
        if !bundle.meeting.participants.is_empty() {
            let _ = writeln!(
                output,
                "- Participants: {}",
                bundle.meeting.participants.join(", ")
            );
        }
        if bundle.meeting.started_at.is_some()
            || bundle.meeting.duration_ms.is_some()
            || !bundle.meeting.participants.is_empty()
        {
            output.push('\n');
        }
        if let Some(note) = bundle.speaker_scope_note() {
            let _ = writeln!(output, "> {note}\n");
        }

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
                let _ = writeln!(
                    output,
                    "- {} — {} (due: {}) _(evidence: {})_",
                    assignee_label(item.assignee_profile_id.as_deref()),
                    item.description,
                    due_label(item.due_at.as_deref()),
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
            let _ = writeln!(
                output,
                "- `{}` {}–{} {}: {}",
                segment.id,
                format_timestamp(segment.start_ms),
                format_timestamp(segment.end_ms),
                bundle.speaker_label(&segment.speaker_id),
                segment.text
            );
        }

        Ok(Artifact {
            id: format!("artifact-{}-markdown", bundle.meeting.id),
            meeting_id: bundle.meeting.id.clone(),
            kind: ArtifactKind::Markdown,
            media_type: "text/markdown; charset=utf-8".to_owned(),
            content: output,
            encoding: ArtifactEncoding::Utf8,
            created_at: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use meeting_intelligence_core::SpeakerScope;

    use crate::test_support::bundle;

    use super::*;

    #[test]
    fn markdown_renders_every_required_section() -> Result<(), ExportError> {
        let artifact = MarkdownExporter.export(&bundle())?;

        for heading in [
            "# Bootstrap planning",
            "## Summary",
            "## Decisions",
            "## Action items",
            "## Open questions",
            "## Transcript",
        ] {
            assert!(artifact.content.contains(heading), "missing {heading}");
        }
        assert_eq!(artifact.kind, ArtifactKind::Markdown);
        assert_eq!(artifact.encoding, ArtifactEncoding::Utf8);
        Ok(())
    }

    #[test]
    fn the_transcript_shows_ids_timestamps_and_speaker_labels() -> Result<(), ExportError> {
        let artifact = MarkdownExporter.export(&bundle())?;

        assert!(artifact
            .content
            .contains("`seg_001` 00:00:00–00:00:04 Alice: Keep the pipeline deterministic."));
        assert!(artifact
            .content
            .contains("`seg_002` 00:00:04–00:00:08 Speaker 2:"));
        Ok(())
    }

    #[test]
    fn a_low_confidence_speaker_is_never_given_the_candidate_name() -> Result<(), ExportError> {
        let mut bundle = bundle();
        // The identity provider proposed "Bob" at 0.42 confidence; Meeting Core
        // rejected it, so no export may show that name.
        bundle.speakers[1].display_name = Some("Bob".to_owned());
        let artifact = MarkdownExporter.export(&bundle)?;

        assert!(!artifact.content.contains("Bob"));
        assert!(artifact.content.contains("Speaker 2"));
        Ok(())
    }

    #[test]
    fn a_manual_rename_replaces_the_label_everywhere() -> Result<(), ExportError> {
        let mut bundle = bundle();
        bundle.speakers[1].manual_display_name = Some("Mr. Wang".to_owned());
        let artifact = MarkdownExporter.export(&bundle)?;

        assert!(artifact.content.contains("Mr. Wang: Can we export"));
        assert!(!artifact.content.contains("Speaker 2:"));
        Ok(())
    }

    #[test]
    fn a_chunked_transcript_warns_about_speaker_scope() -> Result<(), ExportError> {
        let mut bundle = bundle();
        bundle.transcript.speaker_scope = SpeakerScope::Chunk;
        let artifact = MarkdownExporter.export(&bundle)?;

        assert!(artifact.content.contains("reliable within one part only"));
        Ok(())
    }

    #[test]
    fn empty_analysis_sections_say_none_rather_than_disappearing() -> Result<(), ExportError> {
        let mut bundle = bundle();
        bundle.analysis.decisions.clear();
        bundle.analysis.action_items.clear();
        bundle.analysis.open_questions.clear();
        let artifact = MarkdownExporter.export(&bundle)?;

        assert_eq!(artifact.content.matches("- None").count(), 3);
        Ok(())
    }

    #[test]
    fn markdown_export_is_deterministic() -> Result<(), ExportError> {
        let bundle = bundle();

        assert_eq!(
            MarkdownExporter.export(&bundle)?.content,
            MarkdownExporter.export(&bundle)?.content
        );
        Ok(())
    }
}
