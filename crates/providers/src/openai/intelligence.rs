use std::sync::Arc;

use async_trait::async_trait;
use meeting_intelligence_core::{
    IntelligenceProvider, IntelligenceRequest, MeetingAnalysis, ProviderError, RunContext,
};
use serde_json::json;
use tracing::{info, warn};

use super::{
    analysis::{
        analysis_json_schema, parse_analysis_draft, parse_responses_output,
        render_transcript_for_prompt, to_meeting_analysis, SYSTEM_PROMPT,
    },
    client::RetryingClient,
    config::OpenAiConfig,
    transport::{HttpMethod, HttpRequest, HttpTransport, RequestBody, PROVIDER},
};

const RESPONSES_PATH: &str = "/v1/responses";

/// Meeting analysis backed by the OpenAI Responses API with a strict schema.
pub struct OpenAiIntelligenceProvider {
    client: RetryingClient,
    config: OpenAiConfig,
}

impl OpenAiIntelligenceProvider {
    pub fn new(transport: Arc<dyn HttpTransport>, config: OpenAiConfig) -> Self {
        Self {
            client: RetryingClient::new(transport, &config),
            config,
        }
    }

    fn build_request(&self, request: &IntelligenceRequest) -> Result<String, ProviderError> {
        let rendered = render_transcript_for_prompt(
            &request.transcript,
            &request.speakers,
            self.config.max_transcript_characters,
        )?;

        let mut user_prompt = String::new();
        user_prompt.push_str("Meeting title: ");
        user_prompt.push_str(&request.meeting.title);
        user_prompt.push('\n');
        if let Some(started_at) = &request.meeting.started_at {
            user_prompt.push_str("Started at: ");
            user_prompt.push_str(started_at);
            user_prompt.push('\n');
        }
        if request.transcript.speaker_scope == meeting_intelligence_core::SpeakerScope::Chunk {
            user_prompt.push_str(
                "Speaker labels are scoped to separately transcribed parts of this \
                 recording. Do not assume that a label in one part is the same person \
                 as the same label in another part.\n",
            );
        }
        user_prompt.push_str("\nTranscript:\n");
        user_prompt.push_str(&rendered);

        let body = json!({
            "model": self.config.intelligence_model,
            "input": [
                {
                    "role": "system",
                    "content": [{ "type": "input_text", "text": SYSTEM_PROMPT }]
                },
                {
                    "role": "user",
                    "content": [{ "type": "input_text", "text": user_prompt }]
                }
            ],
            "reasoning": {
                "effort": self.config.reasoning_presets.effort_for(request.analysis_mode)
            },
            "text": {
                "format": {
                    "type": "json_schema",
                    "name": "meeting_analysis",
                    "strict": true,
                    "schema": analysis_json_schema()
                }
            },
            "max_output_tokens": self.config.max_output_tokens,
            "store": false
        });

        serde_json::to_string(&body).map_err(|_| {
            ProviderError::configuration(PROVIDER, "the analysis request could not be built")
        })
    }
}

#[async_trait]
impl IntelligenceProvider for OpenAiIntelligenceProvider {
    async fn analyze(
        &self,
        request: IntelligenceRequest,
        context: &RunContext,
    ) -> Result<MeetingAnalysis, ProviderError> {
        context
            .cancellation
            .check()
            .map_err(|_| ProviderError::canceled(PROVIDER))?;

        let payload = self.build_request(&request)?;
        let body = self
            .client
            .send(HttpRequest {
                method: HttpMethod::Post,
                path: RESPONSES_PATH,
                body: RequestBody::Json(payload),
            })
            .await?;

        let structured = parse_responses_output(&body)?;
        let draft = parse_analysis_draft(&structured)?;
        let (analysis, evidence) = to_meeting_analysis(
            &request.meeting.id,
            draft,
            &request.transcript,
            &request.speakers,
        );

        if evidence.rejected > 0 {
            warn!(
                meeting_id = request.meeting.id,
                rejected = evidence.rejected,
                accepted = evidence.accepted,
                "dropped analysis evidence that did not match a transcript segment"
            );
        }
        info!(
            meeting_id = request.meeting.id,
            decisions = analysis.decisions.len(),
            action_items = analysis.action_items.len(),
            open_questions = analysis.open_questions.len(),
            "openai analysis completed"
        );
        Ok(analysis)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, PoisonError};

    use meeting_intelligence_core::{
        AnalysisMode, Meeting, MeetingProcessingState, ProcessingJobErrorCategory, SpeakerScope,
        Transcript, TranscriptSegment,
    };
    use serde_json::Value;

    use super::super::transport::HttpResponse;
    use super::*;

    struct ReplayTransport {
        responses: Mutex<Vec<HttpResponse>>,
        requests: Mutex<Vec<HttpRequest>>,
    }

    impl ReplayTransport {
        fn new(responses: Vec<HttpResponse>) -> Arc<Self> {
            Arc::new(Self {
                responses: Mutex::new(responses),
                requests: Mutex::new(Vec::new()),
            })
        }

        fn last_request_json(&self) -> Value {
            let requests = self.requests.lock().unwrap_or_else(PoisonError::into_inner);
            let RequestBody::Json(body) = &requests[requests.len() - 1].body else {
                panic!("the analysis request must be JSON");
            };
            serde_json::from_str(body).expect("request body should be valid JSON")
        }
    }

    #[async_trait]
    impl HttpTransport for ReplayTransport {
        async fn send(&self, request: HttpRequest) -> Result<HttpResponse, ProviderError> {
            self.requests
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(request);
            let mut responses = self
                .responses
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if responses.is_empty() {
                return Err(ProviderError::network(PROVIDER, "no recorded response"));
            }
            Ok(responses.remove(0))
        }
    }

    fn request(mode: AnalysisMode) -> IntelligenceRequest {
        IntelligenceRequest {
            meeting: Meeting {
                id: "m1".to_owned(),
                title: "BCRC weekly".to_owned(),
                started_at: Some("2026-08-12T09:00:00Z".to_owned()),
                processing_state: MeetingProcessingState::Processing,
                artifact_ids: Vec::new(),
                participants: Vec::new(),
                duration_ms: None,
                metadata: Default::default(),
            },
            transcript: Transcript {
                meeting_id: "m1".to_owned(),
                language: Some("en".to_owned()),
                speaker_scope: SpeakerScope::Meeting,
                segments: vec![TranscriptSegment {
                    id: "seg_00001".to_owned(),
                    speaker_id: "speaker_01".to_owned(),
                    start_ms: 0,
                    end_ms: 4_000,
                    text: "We will ship on Friday.".to_owned(),
                }],
            },
            speakers: Vec::new(),
            analysis_mode: mode,
        }
    }

    fn recorded(structured: &str) -> HttpResponse {
        HttpResponse {
            status: 200,
            body: json!({
                "id": "resp_1",
                "status": "completed",
                "output": [{
                    "type": "message",
                    "role": "assistant",
                    "content": [{ "type": "output_text", "text": structured }]
                }]
            })
            .to_string(),
        }
    }

    const GOOD_ANALYSIS: &str = r#"{"summary":"The team agreed to ship on Friday.",
        "decisions":[{"statement":"Ship on Friday.","evidence_segment_ids":["seg_00001"]}],
        "action_items":[{"description":"Prepare the release notes.",
          "assignee_name":null,"due_at":null,"evidence_segment_ids":["seg_00001"]}],
        "open_questions":[]}"#;

    #[tokio::test]
    async fn a_recorded_response_becomes_canonical_analysis() -> Result<(), ProviderError> {
        let transport = ReplayTransport::new(vec![recorded(GOOD_ANALYSIS)]);
        let provider = OpenAiIntelligenceProvider::new(transport, OpenAiConfig::default());

        let analysis = provider
            .analyze(request(AnalysisMode::Standard), &RunContext::detached())
            .await?;

        assert_eq!(analysis.meeting_id, "m1");
        assert_eq!(analysis.decisions.len(), 1);
        assert_eq!(analysis.decisions[0].id, "decision_001");
        assert_eq!(analysis.action_items[0].assignee_profile_id, None);
        Ok(())
    }

    #[tokio::test]
    async fn the_request_uses_the_luna_model_and_a_strict_schema() -> Result<(), ProviderError> {
        let transport = ReplayTransport::new(vec![recorded(GOOD_ANALYSIS)]);
        let provider = OpenAiIntelligenceProvider::new(transport.clone(), OpenAiConfig::default());

        provider
            .analyze(request(AnalysisMode::Standard), &RunContext::detached())
            .await?;

        let body = transport.last_request_json();
        assert_eq!(body["model"], "gpt-5.6-luna");
        assert_eq!(body["text"]["format"]["type"], "json_schema");
        assert_eq!(body["text"]["format"]["strict"], Value::Bool(true));
        assert!(body["input"][1]["content"][0]["text"]
            .as_str()
            .expect("user prompt")
            .contains("[seg_00001]"));
        Ok(())
    }

    #[tokio::test]
    async fn each_analysis_mode_sends_its_configured_reasoning_effort() -> Result<(), ProviderError>
    {
        for (mode, expected) in [
            (AnalysisMode::Fast, "low"),
            (AnalysisMode::Standard, "medium"),
            (AnalysisMode::Deep, "max"),
        ] {
            let transport = ReplayTransport::new(vec![recorded(GOOD_ANALYSIS)]);
            let provider =
                OpenAiIntelligenceProvider::new(transport.clone(), OpenAiConfig::default());

            provider
                .analyze(request(mode), &RunContext::detached())
                .await?;

            assert_eq!(
                transport.last_request_json()["reasoning"]["effort"],
                expected
            );
        }
        Ok(())
    }

    #[tokio::test]
    async fn hallucinated_evidence_never_reaches_canonical_analysis() -> Result<(), ProviderError> {
        let transport = ReplayTransport::new(vec![recorded(
            r#"{"summary":"s",
                "decisions":[{"statement":"Ship on Friday.",
                  "evidence_segment_ids":["seg_00001","seg_00042"]}],
                "action_items":[],"open_questions":[]}"#,
        )]);
        let provider = OpenAiIntelligenceProvider::new(transport, OpenAiConfig::default());

        let analysis = provider
            .analyze(request(AnalysisMode::Standard), &RunContext::detached())
            .await?;

        assert_eq!(
            analysis.decisions[0].evidence_segment_ids,
            vec!["seg_00001".to_owned()]
        );
        Ok(())
    }

    #[tokio::test]
    async fn a_malformed_structured_payload_is_rejected() {
        let transport = ReplayTransport::new(vec![recorded("not json at all")]);
        let provider = OpenAiIntelligenceProvider::new(transport, OpenAiConfig::default());

        let error = provider
            .analyze(request(AnalysisMode::Standard), &RunContext::detached())
            .await
            .expect_err("malformed output must be rejected");

        assert_eq!(
            error.category(),
            ProcessingJobErrorCategory::InvalidProviderOutput
        );
    }

    #[tokio::test]
    async fn a_rejected_credential_is_surfaced_as_a_credential_error() {
        let transport = ReplayTransport::new(vec![HttpResponse {
            status: 401,
            body: r#"{"error":{"code":"invalid_api_key","message":"sk-live-LEAK"}}"#.to_owned(),
        }]);
        let provider = OpenAiIntelligenceProvider::new(transport, OpenAiConfig::default());

        let error = provider
            .analyze(request(AnalysisMode::Standard), &RunContext::detached())
            .await
            .expect_err("a rejected credential must fail");

        assert_eq!(error.category(), ProcessingJobErrorCategory::Credential);
        assert!(!error.to_string().contains("sk-live-LEAK"));
    }

    #[tokio::test]
    async fn cancellation_stops_before_the_request_is_sent() {
        let transport = ReplayTransport::new(vec![recorded(GOOD_ANALYSIS)]);
        let provider = OpenAiIntelligenceProvider::new(transport, OpenAiConfig::default());
        let context = RunContext::detached();
        context.cancellation.cancel();

        let error = provider
            .analyze(request(AnalysisMode::Standard), &context)
            .await
            .expect_err("a cancelled run must stop");

        assert_eq!(error.category(), ProcessingJobErrorCategory::Canceled);
    }
}
