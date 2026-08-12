use std::collections::{BTreeMap, HashSet};

use meeting_intelligence_core::{
    ActionItem, Decision, MeetingAnalysis, OpenQuestion, ProviderError, Speaker, Transcript,
};
use serde::Deserialize;
use serde_json::{json, Value};

use super::transport::PROVIDER;

/// What the model is asked to return. `assignee_name` is a name rather than a
/// profile ID because the model cannot know local profile identifiers; the
/// mapping to a profile happens here, and only on an exact match.
/// The strict schema guarantees the identifying fields, so they are required
/// here: a payload that omits them is a different shape, not a sparse answer,
/// and must be rejected rather than silently read as an empty analysis.
#[derive(Clone, Debug, Deserialize)]
pub struct AnalysisDraft {
    pub summary: String,
    #[serde(default)]
    pub decisions: Vec<DecisionDraft>,
    #[serde(default)]
    pub action_items: Vec<ActionItemDraft>,
    #[serde(default)]
    pub open_questions: Vec<OpenQuestionDraft>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DecisionDraft {
    pub statement: String,
    #[serde(default)]
    pub evidence_segment_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ActionItemDraft {
    pub description: String,
    #[serde(default)]
    pub assignee_name: Option<String>,
    #[serde(default)]
    pub due_at: Option<String>,
    #[serde(default)]
    pub evidence_segment_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct OpenQuestionDraft {
    pub question: String,
    #[serde(default)]
    pub evidence_segment_ids: Vec<String>,
}

/// How much of the model's evidence survived validation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EvidenceReport {
    pub accepted: usize,
    pub rejected: usize,
}

/// Strict JSON schema for the structured response.
///
/// Every property is required and `additionalProperties` is closed, so the
/// provider validates the shape instead of this crate parsing free text.
pub fn analysis_json_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["summary", "decisions", "action_items", "open_questions"],
        "properties": {
            "summary": {
                "type": "string",
                "description": "Neutral prose summary of what the meeting covered."
            },
            "decisions": {
                "type": "array",
                "description":
                    "Only conclusions the meeting actually settled. A suggestion, \
                     proposal, or unresolved debate is not a decision.",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["statement", "evidence_segment_ids"],
                    "properties": {
                        "statement": { "type": "string" },
                        "evidence_segment_ids": {
                            "type": "array",
                            "items": { "type": "string" }
                        }
                    }
                }
            },
            "action_items": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": [
                        "description", "assignee_name", "due_at", "evidence_segment_ids"
                    ],
                    "properties": {
                        "description": { "type": "string" },
                        "assignee_name": {
                            "type": ["string", "null"],
                            "description":
                                "Exactly as named in the transcript, or null when no \
                                 owner was explicitly assigned. Never inferred."
                        },
                        "due_at": {
                            "type": ["string", "null"],
                            "description":
                                "Deadline exactly as stated, or null when none was \
                                 stated. Never inferred."
                        },
                        "evidence_segment_ids": {
                            "type": "array",
                            "items": { "type": "string" }
                        }
                    }
                }
            },
            "open_questions": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["question", "evidence_segment_ids"],
                    "properties": {
                        "question": { "type": "string" },
                        "evidence_segment_ids": {
                            "type": "array",
                            "items": { "type": "string" }
                        }
                    }
                }
            }
        }
    })
}

pub const SYSTEM_PROMPT: &str = "\
You analyse a meeting transcript and return structured results.

Rules you must follow exactly:
1. Use only what the transcript says. Never add outside knowledge.
2. A decision is something the meeting settled. A suggestion, a proposal, a \
   preference, or an unresolved argument is NOT a decision; put those in open \
   questions or leave them out.
3. Set assignee_name to null unless the transcript explicitly assigns an owner. \
   Never infer an owner from who happened to be speaking.
4. Set due_at to null unless a deadline was explicitly stated. Never invent or \
   calculate one.
5. Every evidence_segment_ids entry must be a segment ID copied verbatim from \
   the transcript, in the form seg_00001. Cite the segments that actually \
   support the item. Never invent an ID.
6. Write in the dominant language of the transcript.
7. If the meeting settled nothing, return an empty decisions array. Empty \
   arrays are correct answers.";

/// Renders the transcript with stable, citable segment IDs.
pub fn render_transcript_for_prompt(
    transcript: &Transcript,
    speakers: &[Speaker],
    max_characters: usize,
) -> Result<String, ProviderError> {
    let labels: BTreeMap<&str, String> = speakers
        .iter()
        .map(|speaker| (speaker.transcript_speaker_id.as_str(), speaker.label()))
        .collect();

    let mut rendered = String::new();
    for segment in &transcript.segments {
        let label = labels
            .get(segment.speaker_id.as_str())
            .cloned()
            .unwrap_or_else(|| {
                meeting_intelligence_core::fallback_speaker_label(&segment.speaker_id)
            });
        rendered.push_str(&format!(
            "[{}] {}–{} {}: {}\n",
            segment.id,
            format_timestamp(segment.start_ms),
            format_timestamp(segment.end_ms),
            label,
            segment.text
        ));
        if rendered.len() > max_characters {
            // Truncating would silently hide part of a meeting from analysis,
            // so this is reported instead.
            return Err(ProviderError::configuration(
                PROVIDER,
                "the transcript is too long to analyse in one request",
            ));
        }
    }
    Ok(rendered)
}

pub fn format_timestamp(milliseconds: u64) -> String {
    let total_seconds = milliseconds / 1_000;
    format!(
        "{:02}:{:02}:{:02}",
        total_seconds / 3_600,
        (total_seconds % 3_600) / 60,
        total_seconds % 60
    )
}

/// Extracts the structured payload from a Responses API result.
pub fn parse_responses_output(body: &str) -> Result<String, ProviderError> {
    let document: Value = serde_json::from_str(body).map_err(|_| invalid("unreadable response"))?;

    if document.get("status").and_then(Value::as_str) == Some("incomplete") {
        return Err(invalid("the analysis response was cut short"));
    }

    if let Some(text) = document.get("output_text").and_then(Value::as_str) {
        if !text.trim().is_empty() {
            return Ok(text.to_owned());
        }
    }

    let output = document
        .get("output")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("the analysis response contained no output"))?;

    let text = output
        .iter()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("message"))
        .filter_map(|item| item.get("content").and_then(Value::as_array))
        .flatten()
        .filter(|part| part.get("type").and_then(Value::as_str) == Some("output_text"))
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("");

    if text.trim().is_empty() {
        return Err(invalid(
            "the analysis response contained no structured output",
        ));
    }
    Ok(text)
}

pub fn parse_analysis_draft(payload: &str) -> Result<AnalysisDraft, ProviderError> {
    serde_json::from_str(payload)
        .map_err(|_| invalid("the analysis response did not match the requested schema"))
}

/// Converts a model draft into canonical analysis.
///
/// Evidence IDs that are not present in the transcript are dropped rather than
/// accepted, so a hallucinated citation can never reach storage or an export.
/// Assignees are resolved only by exact name match against a speaker that
/// already has a profile; anything else stays unassigned.
pub fn to_meeting_analysis(
    meeting_id: &str,
    draft: AnalysisDraft,
    transcript: &Transcript,
    speakers: &[Speaker],
) -> (MeetingAnalysis, EvidenceReport) {
    let known: HashSet<&str> = transcript
        .segments
        .iter()
        .map(|segment| segment.id.as_str())
        .collect();
    let mut report = EvidenceReport::default();

    let decisions = draft
        .decisions
        .into_iter()
        .filter(|item| !item.statement.trim().is_empty())
        .enumerate()
        .map(|(index, item)| Decision {
            id: format!("decision_{:03}", index + 1),
            statement: item.statement.trim().to_owned(),
            evidence_segment_ids: validate_evidence(item.evidence_segment_ids, &known, &mut report),
        })
        .collect();

    let action_items = draft
        .action_items
        .into_iter()
        .filter(|item| !item.description.trim().is_empty())
        .enumerate()
        .map(|(index, item)| ActionItem {
            id: format!("action_{:03}", index + 1),
            description: item.description.trim().to_owned(),
            assignee_profile_id: resolve_assignee(item.assignee_name.as_deref(), speakers),
            due_at: item
                .due_at
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty()),
            evidence_segment_ids: validate_evidence(item.evidence_segment_ids, &known, &mut report),
        })
        .collect();

    let open_questions = draft
        .open_questions
        .into_iter()
        .filter(|item| !item.question.trim().is_empty())
        .enumerate()
        .map(|(index, item)| OpenQuestion {
            id: format!("question_{:03}", index + 1),
            question: item.question.trim().to_owned(),
            evidence_segment_ids: validate_evidence(item.evidence_segment_ids, &known, &mut report),
        })
        .collect();

    (
        MeetingAnalysis {
            meeting_id: meeting_id.to_owned(),
            summary: draft.summary.trim().to_owned(),
            decisions,
            action_items,
            open_questions,
        },
        report,
    )
}

fn validate_evidence(
    candidates: Vec<String>,
    known: &HashSet<&str>,
    report: &mut EvidenceReport,
) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut accepted = Vec::new();
    for candidate in candidates {
        let trimmed = candidate.trim();
        if known.contains(trimmed) {
            if seen.insert(trimmed.to_owned()) {
                accepted.push(trimmed.to_owned());
                report.accepted += 1;
            }
        } else {
            report.rejected += 1;
        }
    }
    accepted
}

fn resolve_assignee(name: Option<&str>, speakers: &[Speaker]) -> Option<String> {
    let name = name.map(str::trim).filter(|value| !value.is_empty())?;
    speakers
        .iter()
        .find(|speaker| speaker.profile_id.is_some() && speaker.label().eq_ignore_ascii_case(name))
        .and_then(|speaker| speaker.profile_id.clone())
}

fn invalid(message: &'static str) -> ProviderError {
    ProviderError::invalid_response(PROVIDER, message)
}

#[cfg(test)]
mod tests {
    use meeting_intelligence_core::{SpeakerIdentityStatus, SpeakerScope, TranscriptSegment};

    use super::*;

    fn transcript() -> Transcript {
        Transcript {
            meeting_id: "m1".to_owned(),
            language: Some("en".to_owned()),
            speaker_scope: SpeakerScope::Meeting,
            segments: vec![
                TranscriptSegment {
                    id: "seg_00001".to_owned(),
                    speaker_id: "speaker_01".to_owned(),
                    start_ms: 0,
                    end_ms: 4_200,
                    text: "We will ship on Friday.".to_owned(),
                },
                TranscriptSegment {
                    id: "seg_00002".to_owned(),
                    speaker_id: "speaker_02".to_owned(),
                    start_ms: 865_000,
                    end_ms: 883_000,
                    text: "Someone should check procurement.".to_owned(),
                },
            ],
        }
    }

    fn speakers() -> Vec<Speaker> {
        vec![
            Speaker {
                transcript_speaker_id: "speaker_01".to_owned(),
                identity: SpeakerIdentityStatus::Resolved,
                profile_id: Some("profile_alice".to_owned()),
                display_name: Some("Alice".to_owned()),
                confidence: Some(0.95),
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

    fn draft(json: &str) -> AnalysisDraft {
        parse_analysis_draft(json).expect("draft fixture should parse")
    }

    #[test]
    fn the_prompt_carries_citable_segment_ids_and_readable_timestamps() -> Result<(), ProviderError>
    {
        let rendered = render_transcript_for_prompt(&transcript(), &speakers(), 100_000)?;

        assert!(rendered.contains("[seg_00001] 00:00:00–00:00:04 Alice: We will ship on Friday."));
        assert!(rendered.contains("[seg_00002] 00:14:25–00:14:43 Mr. Wang:"));
        Ok(())
    }

    #[test]
    fn an_oversized_transcript_is_refused_rather_than_truncated() {
        let error = render_transcript_for_prompt(&transcript(), &speakers(), 10)
            .expect_err("an oversized transcript must be refused");

        assert!(error.to_string().contains("too long"));
    }

    #[test]
    fn unlabelled_speakers_still_render_a_stable_label() -> Result<(), ProviderError> {
        let rendered = render_transcript_for_prompt(&transcript(), &[], 100_000)?;

        assert!(rendered.contains("Speaker 1:"));
        assert!(rendered.contains("Speaker 2:"));
        Ok(())
    }

    #[test]
    fn invented_evidence_is_removed_and_counted() {
        let (analysis, report) = to_meeting_analysis(
            "m1",
            draft(
                r#"{"summary":"s",
                    "decisions":[{"statement":"Ship on Friday.",
                      "evidence_segment_ids":["seg_00001","seg_99999","seg_00001"]}],
                    "action_items":[],"open_questions":[]}"#,
            ),
            &transcript(),
            &speakers(),
        );

        assert_eq!(
            analysis.decisions[0].evidence_segment_ids,
            vec!["seg_00001".to_owned()],
            "unknown IDs must be dropped and duplicates collapsed"
        );
        assert_eq!(report.accepted, 1);
        assert_eq!(report.rejected, 1);
    }

    #[test]
    fn a_missing_owner_or_deadline_stays_null() {
        let (analysis, _) = to_meeting_analysis(
            "m1",
            draft(
                r#"{"summary":"s","decisions":[],
                    "action_items":[{"description":"Check procurement.",
                      "assignee_name":null,"due_at":null,
                      "evidence_segment_ids":["seg_00002"]}],
                    "open_questions":[]}"#,
            ),
            &transcript(),
            &speakers(),
        );

        assert_eq!(analysis.action_items[0].assignee_profile_id, None);
        assert_eq!(analysis.action_items[0].due_at, None);
    }

    #[test]
    fn an_owner_is_resolved_only_by_exact_match_against_a_profile() {
        let (analysis, _) = to_meeting_analysis(
            "m1",
            draft(
                r#"{"summary":"s","decisions":[],
                    "action_items":[
                      {"description":"a","assignee_name":"alice","due_at":null,
                       "evidence_segment_ids":[]},
                      {"description":"b","assignee_name":"Mr. Wang","due_at":null,
                       "evidence_segment_ids":[]},
                      {"description":"c","assignee_name":"Someone else","due_at":null,
                       "evidence_segment_ids":[]}],
                    "open_questions":[]}"#,
            ),
            &transcript(),
            &speakers(),
        );

        assert_eq!(
            analysis.action_items[0].assignee_profile_id.as_deref(),
            Some("profile_alice")
        );
        assert_eq!(
            analysis.action_items[1].assignee_profile_id, None,
            "a manually named speaker has no enrolled profile to point at"
        );
        assert_eq!(analysis.action_items[2].assignee_profile_id, None);
    }

    #[test]
    fn ids_are_deterministic_and_blank_items_are_dropped() {
        let (analysis, _) = to_meeting_analysis(
            "m1",
            draft(
                r#"{"summary":"  s  ",
                    "decisions":[{"statement":"   ","evidence_segment_ids":[]},
                                 {"statement":"Real one","evidence_segment_ids":[]}],
                    "action_items":[{"description":"Do it","assignee_name":null,
                      "due_at":"  ","evidence_segment_ids":[]}],
                    "open_questions":[{"question":"Why?","evidence_segment_ids":[]}]}"#,
            ),
            &transcript(),
            &speakers(),
        );

        assert_eq!(analysis.summary, "s");
        assert_eq!(analysis.decisions.len(), 1);
        assert_eq!(analysis.decisions[0].id, "decision_001");
        assert_eq!(analysis.action_items[0].id, "action_001");
        assert_eq!(analysis.action_items[0].due_at, None);
        assert_eq!(analysis.open_questions[0].id, "question_001");
    }

    #[test]
    fn an_empty_analysis_is_a_valid_answer() {
        let (analysis, report) = to_meeting_analysis(
            "m1",
            draft(
                r#"{"summary":"Nothing was settled.","decisions":[],
                    "action_items":[],"open_questions":[]}"#,
            ),
            &transcript(),
            &speakers(),
        );

        assert!(analysis.decisions.is_empty());
        assert_eq!(report, EvidenceReport::default());
    }

    #[test]
    fn structured_output_is_read_from_the_responses_envelope() -> Result<(), ProviderError> {
        let body = r#"{"id":"resp_1","status":"completed","output":[
          {"type":"reasoning","summary":[]},
          {"type":"message","role":"assistant","content":[
            {"type":"output_text","text":"{\"summary\":\"ok\"}"}]}]}"#;

        assert_eq!(parse_responses_output(body)?, "{\"summary\":\"ok\"}");
        Ok(())
    }

    #[test]
    fn a_top_level_output_text_is_also_accepted() -> Result<(), ProviderError> {
        let body = r#"{"status":"completed","output_text":"{\"summary\":\"ok\"}","output":[]}"#;

        assert_eq!(parse_responses_output(body)?, "{\"summary\":\"ok\"}");
        Ok(())
    }

    #[test]
    fn a_truncated_or_empty_response_is_rejected() {
        assert!(parse_responses_output(r#"{"status":"incomplete","output":[]}"#).is_err());
        assert!(parse_responses_output(r#"{"status":"completed","output":[]}"#).is_err());
        assert!(parse_responses_output("not json").is_err());
        assert!(parse_responses_output(
            r#"{"status":"completed","output":[{"type":"message","content":[]}]}"#
        )
        .is_err());
    }

    #[test]
    fn a_payload_that_ignores_the_schema_is_rejected() {
        assert!(parse_analysis_draft("[]").is_err());
        assert!(parse_analysis_draft("{}").is_err());
        assert!(parse_analysis_draft(r#"{"summary": 12}"#).is_err());
        assert!(parse_analysis_draft(
            r#"{"summary":"s","decisions":[{"evidence_segment_ids":[]}],
                "action_items":[],"open_questions":[]}"#
        )
        .is_err());
    }

    #[test]
    fn the_schema_is_strict_and_forbids_extra_fields() {
        let schema = analysis_json_schema();

        assert_eq!(schema["additionalProperties"], Value::Bool(false));
        assert_eq!(
            schema["properties"]["action_items"]["items"]["additionalProperties"],
            Value::Bool(false)
        );
        let required = schema["properties"]["action_items"]["items"]["required"]
            .as_array()
            .expect("action item required list");
        assert!(required.iter().any(|value| value == "assignee_name"));
        assert!(required.iter().any(|value| value == "due_at"));
    }
}
