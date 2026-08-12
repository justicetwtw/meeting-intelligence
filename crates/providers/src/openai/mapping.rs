use std::collections::BTreeMap;

use meeting_intelligence_core::{ProviderError, SpeakerScope, Transcript, TranscriptSegment};
use serde::Deserialize;

use super::transport::PROVIDER;

/// Diarized transcription response.
///
/// Unknown fields are ignored so a provider-side addition cannot break
/// ingestion, but everything the canonical model needs is validated.
#[derive(Clone, Debug, Deserialize)]
pub struct DiarizedTranscription {
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub segments: Vec<DiarizedSegment>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DiarizedSegment {
    #[serde(default)]
    pub start: Option<f64>,
    #[serde(default)]
    pub end: Option<f64>,
    #[serde(default)]
    pub speaker: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
}

/// A response chunk paired with where it sits in the whole meeting.
#[derive(Clone, Debug)]
pub struct ChunkTranscription {
    pub chunk_index: usize,
    pub offset_ms: u64,
    pub end_ms: u64,
    pub response: DiarizedTranscription,
}

pub fn parse_transcription_response(body: &str) -> Result<DiarizedTranscription, ProviderError> {
    let parsed: DiarizedTranscription = serde_json::from_str(body).map_err(|_| {
        ProviderError::invalid_response(PROVIDER, "the transcription response could not be read")
    })?;
    if parsed.segments.is_empty() {
        return Err(ProviderError::invalid_response(
            PROVIDER,
            "the transcription response contained no speaker-aware segments",
        ));
    }
    Ok(parsed)
}

#[derive(Clone, Debug)]
struct StagedSegment {
    chunk_index: usize,
    order: usize,
    start_ms: u64,
    end_ms: u64,
    speaker_key: String,
    text: String,
}

/// Builds one canonical transcript out of every chunk response.
///
/// * timestamps become whole-meeting timestamps,
/// * segment IDs are globally unique and sequential,
/// * ordering is deterministic even when a provider returns segments out of
///   order or with overlapping ranges,
/// * speaker IDs stay scoped to the chunk that produced them whenever the
///   meeting had to be split, because two chunks are diarized independently and
///   nothing here may assume their labels mean the same person.
pub fn merge_chunk_transcriptions(
    meeting_id: &str,
    chunks: &[ChunkTranscription],
) -> Result<Transcript, ProviderError> {
    let is_chunked = chunks.len() > 1;
    let mut staged: Vec<StagedSegment> = Vec::new();
    let mut speaker_ids: BTreeMap<(usize, String), String> = BTreeMap::new();
    let mut per_chunk_speaker_count: BTreeMap<usize, usize> = BTreeMap::new();
    let mut language: Option<String> = None;

    for chunk in chunks {
        if language.is_none() {
            language.clone_from(&chunk.response.language);
        }
        for (order, segment) in chunk.response.segments.iter().enumerate() {
            let text = segment
                .text
                .as_deref()
                .unwrap_or_default()
                .trim()
                .to_owned();
            if text.is_empty() {
                continue;
            }

            let start_ms = seconds_to_ms(segment.start.unwrap_or(0.0))
                .saturating_add(chunk.offset_ms)
                .min(chunk.end_ms);
            let end_ms = seconds_to_ms(segment.end.unwrap_or_default())
                .saturating_add(chunk.offset_ms)
                .clamp(start_ms, chunk.end_ms);

            let raw_speaker = normalize_speaker_label(segment.speaker.as_deref());
            let scope_key = if is_chunked { chunk.chunk_index } else { 0 };
            let speaker_id = speaker_ids
                .entry((scope_key, raw_speaker))
                .or_insert_with(|| {
                    let counter = per_chunk_speaker_count.entry(scope_key).or_insert(0);
                    *counter += 1;
                    if is_chunked {
                        format!("speaker_c{:02}_{:02}", chunk.chunk_index + 1, counter)
                    } else {
                        format!("speaker_{counter:02}")
                    }
                })
                .clone();

            staged.push(StagedSegment {
                chunk_index: chunk.chunk_index,
                order,
                start_ms,
                end_ms,
                speaker_key: speaker_id,
                text,
            });
        }
    }

    if staged.is_empty() {
        return Err(ProviderError::invalid_response(
            PROVIDER,
            "the transcription response contained no usable speech",
        ));
    }

    staged.sort_by(|left, right| {
        left.start_ms
            .cmp(&right.start_ms)
            .then(left.end_ms.cmp(&right.end_ms))
            .then(left.chunk_index.cmp(&right.chunk_index))
            .then(left.order.cmp(&right.order))
    });

    // Diarized output can overlap when two people talk at once. The canonical
    // transcript is a single ordered timeline, so overlaps are resolved by
    // moving the later segment's start rather than by discarding speech.
    let mut previous_end_ms = 0_u64;
    let mut segments = Vec::with_capacity(staged.len());
    for (index, item) in staged.into_iter().enumerate() {
        let start_ms = item.start_ms.max(previous_end_ms);
        let end_ms = item.end_ms.max(start_ms);
        previous_end_ms = end_ms;
        segments.push(TranscriptSegment {
            id: format!("seg_{:05}", index + 1),
            speaker_id: item.speaker_key,
            start_ms,
            end_ms,
            text: item.text,
        });
    }

    Ok(Transcript {
        meeting_id: meeting_id.to_owned(),
        language,
        speaker_scope: if is_chunked {
            SpeakerScope::Chunk
        } else {
            SpeakerScope::Meeting
        },
        segments,
    })
}

/// Collapses provider speaker labels (`A`, `Speaker A`, `speaker_1`, …) onto a
/// stable key. An unlabelled segment gets its own key rather than being folded
/// into a named speaker.
fn normalize_speaker_label(label: Option<&str>) -> String {
    let trimmed = label.unwrap_or_default().trim();
    if trimmed.is_empty() {
        return "\u{0}unlabelled".to_owned();
    }
    let without_prefix = trimmed
        .strip_prefix("Speaker ")
        .or_else(|| trimmed.strip_prefix("speaker "))
        .or_else(|| trimmed.strip_prefix("speaker_"))
        .unwrap_or(trimmed);
    without_prefix.trim().to_ascii_uppercase()
}

fn seconds_to_ms(seconds: f64) -> u64 {
    if !seconds.is_finite() || seconds <= 0.0 {
        return 0;
    }
    let millis = (seconds * 1_000.0).round();
    if millis >= u64::MAX as f64 {
        u64::MAX
    } else {
        millis as u64
    }
}

#[cfg(test)]
mod tests {
    use meeting_intelligence_core::ProcessingJobErrorCategory;

    use super::*;

    fn response(json: &str) -> DiarizedTranscription {
        parse_transcription_response(json).expect("fixture response should parse")
    }

    const SINGLE_CHUNK: &str = r#"{
      "task": "transcribe",
      "language": "zh",
      "duration": 12.6,
      "text": "ignored",
      "segments": [
        {"id":"s0","type":"transcript.text.segment","start":0.0,"end":4.2,
         "speaker":"Speaker A","text":"Let us keep this deterministic."},
        {"id":"s1","type":"transcript.text.segment","start":4.3,"end":8.1,
         "speaker":"Speaker B","text":"Can we export Markdown and JSON?"},
        {"id":"s2","type":"transcript.text.segment","start":8.2,"end":12.6,
         "speaker":"Speaker A","text":"Yes."}
      ]
    }"#;

    fn single(chunk: DiarizedTranscription, end_ms: u64) -> Vec<ChunkTranscription> {
        vec![ChunkTranscription {
            chunk_index: 0,
            offset_ms: 0,
            end_ms,
            response: chunk,
        }]
    }

    #[test]
    fn a_single_chunk_maps_to_meeting_scoped_speakers() -> Result<(), ProviderError> {
        let transcript = merge_chunk_transcriptions("m1", &single(response(SINGLE_CHUNK), 12_600))?;

        assert_eq!(transcript.meeting_id, "m1");
        assert_eq!(transcript.language.as_deref(), Some("zh"));
        assert_eq!(transcript.speaker_scope, SpeakerScope::Meeting);
        assert_eq!(
            transcript
                .segments
                .iter()
                .map(|segment| segment.id.as_str())
                .collect::<Vec<_>>(),
            vec!["seg_00001", "seg_00002", "seg_00003"]
        );
        assert_eq!(
            transcript
                .segments
                .iter()
                .map(|segment| segment.speaker_id.as_str())
                .collect::<Vec<_>>(),
            vec!["speaker_01", "speaker_02", "speaker_01"]
        );
        assert_eq!(transcript.segments[0].start_ms, 0);
        assert_eq!(transcript.segments[0].end_ms, 4_200);
        assert_eq!(transcript.segments[2].end_ms, 12_600);
        Ok(())
    }

    #[test]
    fn differently_written_labels_for_one_voice_collapse_together() -> Result<(), ProviderError> {
        let json = r#"{"segments":[
          {"start":0.0,"end":1.0,"speaker":"Speaker A","text":"one"},
          {"start":1.0,"end":2.0,"speaker":"speaker_A","text":"two"},
          {"start":2.0,"end":3.0,"speaker":"a","text":"three"}
        ]}"#;

        let transcript = merge_chunk_transcriptions("m1", &single(response(json), 3_000))?;

        assert!(transcript
            .segments
            .iter()
            .all(|segment| segment.speaker_id == "speaker_01"));
        Ok(())
    }

    #[test]
    fn an_unlabelled_segment_is_not_folded_into_a_named_speaker() -> Result<(), ProviderError> {
        let json = r#"{"segments":[
          {"start":0.0,"end":1.0,"speaker":"A","text":"one"},
          {"start":1.0,"end":2.0,"text":"two"}
        ]}"#;

        let transcript = merge_chunk_transcriptions("m1", &single(response(json), 2_000))?;

        assert_ne!(
            transcript.segments[0].speaker_id,
            transcript.segments[1].speaker_id
        );
        Ok(())
    }

    #[test]
    fn out_of_order_and_overlapping_segments_become_one_ordered_timeline(
    ) -> Result<(), ProviderError> {
        let json = r#"{"segments":[
          {"start":4.0,"end":6.0,"speaker":"B","text":"second"},
          {"start":0.0,"end":5.0,"speaker":"A","text":"first"}
        ]}"#;

        let transcript = merge_chunk_transcriptions("m1", &single(response(json), 6_000))?;

        assert_eq!(transcript.segments[0].text, "first");
        assert_eq!(transcript.segments[1].text, "second");
        for pair in transcript.segments.windows(2) {
            assert!(
                pair[1].start_ms >= pair[0].end_ms,
                "the canonical timeline must never overlap"
            );
        }
        for segment in &transcript.segments {
            assert!(segment.end_ms >= segment.start_ms);
        }
        Ok(())
    }

    #[test]
    fn blank_segments_are_dropped_without_breaking_id_sequencing() -> Result<(), ProviderError> {
        let json = r#"{"segments":[
          {"start":0.0,"end":1.0,"speaker":"A","text":"one"},
          {"start":1.0,"end":2.0,"speaker":"A","text":"   "},
          {"start":2.0,"end":3.0,"speaker":"A","text":"three"}
        ]}"#;

        let transcript = merge_chunk_transcriptions("m1", &single(response(json), 3_000))?;

        assert_eq!(
            transcript
                .segments
                .iter()
                .map(|segment| segment.id.as_str())
                .collect::<Vec<_>>(),
            vec!["seg_00001", "seg_00002"]
        );
        Ok(())
    }

    #[test]
    fn chunked_meetings_offset_timestamps_and_scope_speaker_ids() -> Result<(), ProviderError> {
        let first = r#"{"language":"en","segments":[
          {"start":0.0,"end":10.0,"speaker":"A","text":"chunk one speaker one"},
          {"start":10.0,"end":20.0,"speaker":"B","text":"chunk one speaker two"}
        ]}"#;
        let second = r#"{"segments":[
          {"start":0.0,"end":9.0,"speaker":"A","text":"chunk two speaker one"}
        ]}"#;

        let transcript = merge_chunk_transcriptions(
            "m1",
            &[
                ChunkTranscription {
                    chunk_index: 0,
                    offset_ms: 0,
                    end_ms: 45_000,
                    response: response(first),
                },
                ChunkTranscription {
                    chunk_index: 1,
                    offset_ms: 45_000,
                    end_ms: 90_000,
                    response: response(second),
                },
            ],
        )?;

        assert_eq!(transcript.speaker_scope, SpeakerScope::Chunk);
        assert_eq!(
            transcript
                .segments
                .iter()
                .map(|segment| segment.speaker_id.as_str())
                .collect::<Vec<_>>(),
            vec!["speaker_c01_01", "speaker_c01_02", "speaker_c02_01"]
        );
        assert_ne!(
            transcript.segments[0].speaker_id, transcript.segments[2].speaker_id,
            "the same provider label in two chunks must not be merged into one person"
        );
        assert_eq!(transcript.segments[2].start_ms, 45_000);
        assert_eq!(transcript.segments[2].end_ms, 54_000);
        assert_eq!(
            transcript
                .segments
                .iter()
                .map(|segment| segment.id.as_str())
                .collect::<Vec<_>>(),
            vec!["seg_00001", "seg_00002", "seg_00003"],
            "segment IDs must stay unique across chunks"
        );
        Ok(())
    }

    #[test]
    fn a_segment_past_its_chunk_boundary_is_clamped() -> Result<(), ProviderError> {
        let json = r#"{"segments":[{"start":0.0,"end":99.0,"speaker":"A","text":"long"}]}"#;

        let transcript = merge_chunk_transcriptions(
            "m1",
            &[ChunkTranscription {
                chunk_index: 0,
                offset_ms: 0,
                end_ms: 45_000,
                response: response(json),
            }],
        )?;

        assert_eq!(transcript.segments[0].end_ms, 45_000);
        Ok(())
    }

    #[test]
    fn merging_is_deterministic() -> Result<(), ProviderError> {
        let first = merge_chunk_transcriptions("m1", &single(response(SINGLE_CHUNK), 12_600))?;
        let second = merge_chunk_transcriptions("m1", &single(response(SINGLE_CHUNK), 12_600))?;

        assert_eq!(first, second);
        Ok(())
    }

    #[test]
    fn a_response_without_segments_is_rejected() {
        let error = parse_transcription_response(r#"{"text":"hello","segments":[]}"#)
            .expect_err("a non-diarized response must be rejected");

        assert_eq!(
            error.category(),
            ProcessingJobErrorCategory::InvalidProviderOutput
        );
    }

    #[test]
    fn a_malformed_response_is_rejected() {
        assert!(parse_transcription_response("not json").is_err());
        assert!(parse_transcription_response(r#"{"segments":"nope"}"#).is_err());
    }

    #[test]
    fn a_response_with_only_blank_speech_is_rejected() {
        let json = r#"{"segments":[{"start":0.0,"end":1.0,"speaker":"A","text":"  "}]}"#;
        let error = merge_chunk_transcriptions("m1", &single(response(json), 1_000))
            .expect_err("a silent response must be rejected");

        assert_eq!(
            error.category(),
            ProcessingJobErrorCategory::InvalidProviderOutput
        );
    }
}
