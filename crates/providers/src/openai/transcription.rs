use std::sync::Arc;

use async_trait::async_trait;
use meeting_intelligence_audio::{AudioPreparation, PreparedAudio};
use meeting_intelligence_core::{
    AudioSource, ProcessingStage, ProviderError, RunContext, Transcript, TranscriptionProvider,
    TranscriptionRequest,
};
use tracing::info;

use super::{
    client::RetryingClient,
    config::OpenAiConfig,
    mapping::{merge_chunk_transcriptions, parse_transcription_response, ChunkTranscription},
    transport::{HttpMethod, HttpRequest, HttpTransport, MultipartUpload, RequestBody, PROVIDER},
};

const TRANSCRIPTION_PATH: &str = "/v1/audio/transcriptions";
/// Diarized output is what makes a speaker-aware transcript possible; a plain
/// text response would not satisfy the canonical contract.
const RESPONSE_FORMAT: &str = "diarized_json";

/// Speaker-aware transcription backed by OpenAI.
///
/// The provider owns audio preparation because "turn this recording into a
/// canonical transcript" is one bounded capability; Meeting Core never learns
/// about FFmpeg, chunking, or upload limits.
pub struct OpenAiTranscriptionProvider {
    client: RetryingClient,
    preparation: Arc<AudioPreparation>,
    config: OpenAiConfig,
}

impl OpenAiTranscriptionProvider {
    pub fn new(
        transport: Arc<dyn HttpTransport>,
        preparation: Arc<AudioPreparation>,
        config: OpenAiConfig,
    ) -> Self {
        Self {
            client: RetryingClient::new(transport, &config),
            preparation,
            config,
        }
    }

    async fn transcribe_prepared(
        &self,
        meeting_id: &str,
        prepared: &PreparedAudio,
        language_hints: &[String],
        known_context: Option<&str>,
        context: &RunContext,
    ) -> Result<Transcript, ProviderError> {
        let total_parts = u32::try_from(prepared.chunks.len()).unwrap_or(u32::MAX);
        let mut chunk_transcriptions = Vec::with_capacity(prepared.chunks.len());

        for (position, chunk) in prepared.chunks.iter().enumerate() {
            context.cancellation.check().map_err(|_| cancelled())?;
            let part = u32::try_from(position + 1).unwrap_or(u32::MAX);
            context
                .progress
                .stage_with_parts(ProcessingStage::Uploading, part, total_parts);

            let bytes = tokio::fs::read(&chunk.path).await.map_err(|_| {
                ProviderError::audio(PROVIDER, "prepared audio could not be read back")
            })?;
            let upload = MultipartUpload {
                file_field: "file",
                file_name: format!("meeting-part-{:03}.mp3", chunk.plan.index + 1),
                file_media_type: "audio/mpeg",
                file_bytes: bytes,
                fields: self.form_fields(language_hints, known_context),
            };

            context
                .progress
                .stage_with_parts(ProcessingStage::Transcribing, part, total_parts);
            let body = self
                .client
                .send(HttpRequest {
                    method: HttpMethod::Post,
                    path: TRANSCRIPTION_PATH,
                    body: RequestBody::Multipart(Box::new(upload)),
                })
                .await?;

            chunk_transcriptions.push(ChunkTranscription {
                chunk_index: chunk.plan.index,
                offset_ms: chunk.plan.start_ms,
                end_ms: chunk.plan.end_ms,
                response: parse_transcription_response(&body)?,
            });
        }

        context.cancellation.check().map_err(|_| cancelled())?;
        let transcript = merge_chunk_transcriptions(meeting_id, &chunk_transcriptions)?;
        info!(
            meeting_id,
            chunk_count = prepared.chunks.len(),
            segment_count = transcript.segments.len(),
            "openai transcription completed"
        );
        Ok(transcript)
    }

    fn form_fields(
        &self,
        language_hints: &[String],
        known_context: Option<&str>,
    ) -> Vec<(String, String)> {
        let mut fields = vec![
            ("model".to_owned(), self.config.transcription_model.clone()),
            ("response_format".to_owned(), RESPONSE_FORMAT.to_owned()),
        ];
        // The API accepts a single language hint; sending several would be an
        // invented parameter rather than a supported one.
        if let Some(language) = language_hints.iter().find(|hint| !hint.trim().is_empty()) {
            fields.push(("language".to_owned(), language.trim().to_owned()));
        }
        if let Some(prompt) = known_context
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            fields.push(("prompt".to_owned(), prompt.to_owned()));
        }
        fields
    }
}

fn cancelled() -> ProviderError {
    ProviderError::canceled(PROVIDER)
}

#[async_trait]
impl TranscriptionProvider for OpenAiTranscriptionProvider {
    async fn transcribe(
        &self,
        request: TranscriptionRequest,
        context: &RunContext,
    ) -> Result<Transcript, ProviderError> {
        let path = match &request.audio_source {
            AudioSource::LocalFile { path } => std::path::PathBuf::from(path),
            AudioSource::Fixture { .. } | AudioSource::RemoteObject { .. } => {
                return Err(ProviderError::configuration(
                    PROVIDER,
                    "this provider transcribes local recordings only",
                ))
            }
        };

        context.progress.stage(ProcessingStage::PreparingAudio);
        let prepared = self
            .preparation
            .prepare(&path, &context.cancellation)
            .await?;

        self.transcribe_prepared(
            &request.meeting.id,
            &prepared,
            &request.language_hints,
            request.known_context.as_deref(),
            context,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, PoisonError};

    use meeting_intelligence_audio::{AudioPreparationConfig, CommandOutput, FfmpegRunner};
    use meeting_intelligence_core::{
        Meeting, MeetingProcessingState, ProcessingJobErrorCategory, SpeakerScope,
    };
    use tempfile::TempDir;

    use super::super::transport::HttpResponse;
    use super::*;

    struct StubFfmpeg {
        duration_seconds: f64,
        normalized_bytes: usize,
    }

    #[async_trait]
    impl FfmpegRunner for StubFfmpeg {
        async fn run_ffprobe(
            &self,
            _arguments: &[std::ffi::OsString],
        ) -> Result<CommandOutput, meeting_intelligence_audio::AudioError> {
            Ok(CommandOutput {
                success: true,
                stdout: format!(
                    r#"{{"streams":[{{"codec_type":"audio","codec_name":"aac","channels":1,
                       "sample_rate":"16000"}}],"format":{{"duration":"{}"}}}}"#,
                    self.duration_seconds
                ),
                stderr: String::new(),
            })
        }

        async fn run_ffmpeg(
            &self,
            arguments: &[std::ffi::OsString],
        ) -> Result<CommandOutput, meeting_intelligence_audio::AudioError> {
            let rendered: Vec<String> = arguments
                .iter()
                .map(|value| value.to_string_lossy().into_owned())
                .collect();
            if rendered.iter().any(|value| value.contains("silencedetect")) {
                return Ok(CommandOutput {
                    success: true,
                    stdout: String::new(),
                    stderr: String::new(),
                });
            }
            if let Some(destination) = rendered.last() {
                std::fs::write(destination, vec![0_u8; self.normalized_bytes])
                    .map_err(|_| meeting_intelligence_audio::AudioError::Workspace)?;
            }
            Ok(CommandOutput {
                success: true,
                stdout: String::new(),
                stderr: String::new(),
            })
        }
    }

    struct ReplayTransport {
        bodies: Mutex<Vec<String>>,
        requests: Mutex<Vec<HttpRequest>>,
    }

    impl ReplayTransport {
        fn new(bodies: Vec<String>) -> Arc<Self> {
            Arc::new(Self {
                bodies: Mutex::new(bodies),
                requests: Mutex::new(Vec::new()),
            })
        }

        fn requests(&self) -> Vec<HttpRequest> {
            self.requests
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
        }
    }

    #[async_trait]
    impl HttpTransport for ReplayTransport {
        async fn send(&self, request: HttpRequest) -> Result<HttpResponse, ProviderError> {
            self.requests
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(request);
            let mut bodies = self.bodies.lock().unwrap_or_else(PoisonError::into_inner);
            if bodies.is_empty() {
                return Ok(HttpResponse {
                    status: 500,
                    body: "no recorded response".to_owned(),
                });
            }
            Ok(HttpResponse {
                status: 200,
                body: bodies.remove(0),
            })
        }
    }

    fn meeting() -> Meeting {
        Meeting {
            id: "meeting_test_001".to_owned(),
            title: "Test".to_owned(),
            started_at: None,
            processing_state: MeetingProcessingState::Pending,
            artifact_ids: Vec::new(),
            participants: Vec::new(),
            duration_ms: None,
            metadata: Default::default(),
        }
    }

    fn provider(
        transport: Arc<dyn HttpTransport>,
        duration_seconds: f64,
        normalized_bytes: usize,
        max_upload_bytes: u64,
    ) -> OpenAiTranscriptionProvider {
        let preparation = Arc::new(AudioPreparation::new(
            Arc::new(StubFfmpeg {
                duration_seconds,
                normalized_bytes,
            }),
            AudioPreparationConfig {
                max_upload_bytes,
                min_chunk_ms: 10_000,
                ..AudioPreparationConfig::default()
            },
        ));
        OpenAiTranscriptionProvider::new(transport, preparation, OpenAiConfig::default())
    }

    fn recording(directory: &TempDir) -> String {
        let path = directory.path().join("meeting.m4a");
        std::fs::write(&path, vec![9_u8; 2_048]).expect("recording should be writable");
        path.to_string_lossy().into_owned()
    }

    fn request(path: String) -> TranscriptionRequest {
        TranscriptionRequest {
            meeting: meeting(),
            audio_source: AudioSource::LocalFile { path },
            language_hints: vec!["zh".to_owned()],
            known_context: Some("BCRC weekly".to_owned()),
        }
    }

    const RECORDED_RESPONSE: &str = r#"{"language":"zh","duration":20.0,"segments":[
      {"start":0.0,"end":9.5,"speaker":"Speaker A","text":"First point."},
      {"start":10.0,"end":20.0,"speaker":"Speaker B","text":"Second point."}
    ]}"#;

    #[tokio::test]
    async fn a_single_upload_produces_a_meeting_scoped_transcript() -> Result<(), ProviderError> {
        let directory = TempDir::new().expect("temp dir");
        let transport = ReplayTransport::new(vec![RECORDED_RESPONSE.to_owned()]);
        let provider = provider(transport.clone(), 20.0, 1_000, 24 * 1024 * 1024);

        let transcript = provider
            .transcribe(request(recording(&directory)), &RunContext::detached())
            .await?;

        assert_eq!(transcript.meeting_id, "meeting_test_001");
        assert_eq!(transcript.speaker_scope, SpeakerScope::Meeting);
        assert_eq!(transcript.segments.len(), 2);
        assert_eq!(transport.requests().len(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn the_request_carries_the_diarizing_model_and_hints() -> Result<(), ProviderError> {
        let directory = TempDir::new().expect("temp dir");
        let transport = ReplayTransport::new(vec![RECORDED_RESPONSE.to_owned()]);
        let provider = provider(transport.clone(), 20.0, 1_000, 24 * 1024 * 1024);

        provider
            .transcribe(request(recording(&directory)), &RunContext::detached())
            .await?;

        let requests = transport.requests();
        let RequestBody::Multipart(upload) = &requests[0].body else {
            panic!("transcription must be a multipart upload");
        };
        assert_eq!(requests[0].path, "/v1/audio/transcriptions");
        assert!(upload
            .fields
            .contains(&("model".to_owned(), "gpt-4o-transcribe-diarize".to_owned())));
        assert!(upload
            .fields
            .contains(&("response_format".to_owned(), "diarized_json".to_owned())));
        assert!(upload
            .fields
            .contains(&("language".to_owned(), "zh".to_owned())));
        assert!(upload
            .fields
            .contains(&("prompt".to_owned(), "BCRC weekly".to_owned())));
        Ok(())
    }

    #[tokio::test]
    async fn a_chunked_meeting_uploads_each_part_and_scopes_its_speakers(
    ) -> Result<(), ProviderError> {
        let directory = TempDir::new().expect("temp dir");
        // 200 s measured at 4000 bytes against a 1000 byte limit gives 4 parts.
        let transport = ReplayTransport::new(vec![
            RECORDED_RESPONSE.to_owned(),
            RECORDED_RESPONSE.to_owned(),
            RECORDED_RESPONSE.to_owned(),
            RECORDED_RESPONSE.to_owned(),
        ]);
        let provider = provider(transport.clone(), 200.0, 4_000, 1_000);

        let transcript = provider
            .transcribe(request(recording(&directory)), &RunContext::detached())
            .await?;

        assert_eq!(transport.requests().len(), 4);
        assert_eq!(transcript.speaker_scope, SpeakerScope::Chunk);
        assert!(transcript
            .segments
            .iter()
            .any(|segment| segment.speaker_id.starts_with("speaker_c01_")));
        assert!(transcript
            .segments
            .iter()
            .any(|segment| segment.speaker_id.starts_with("speaker_c04_")));
        for pair in transcript.segments.windows(2) {
            assert!(pair[1].start_ms >= pair[0].end_ms);
        }
        Ok(())
    }

    #[tokio::test]
    async fn a_fixture_audio_source_is_refused_by_the_network_provider() {
        let transport = ReplayTransport::new(Vec::new());
        let provider = provider(transport, 20.0, 1_000, 24 * 1024 * 1024);

        let error = provider
            .transcribe(
                TranscriptionRequest {
                    meeting: meeting(),
                    audio_source: AudioSource::Fixture {
                        reference: "bootstrap".to_owned(),
                    },
                    language_hints: Vec::new(),
                    known_context: None,
                },
                &RunContext::detached(),
            )
            .await
            .expect_err("a fixture cannot be uploaded");

        assert_eq!(error.category(), ProcessingJobErrorCategory::Configuration);
    }

    #[tokio::test]
    async fn cancellation_stops_before_any_upload() {
        let directory = TempDir::new().expect("temp dir");
        let transport = ReplayTransport::new(vec![RECORDED_RESPONSE.to_owned()]);
        let provider = provider(transport.clone(), 20.0, 1_000, 24 * 1024 * 1024);
        let context = RunContext::detached();
        context.cancellation.cancel();

        let error = provider
            .transcribe(request(recording(&directory)), &context)
            .await
            .expect_err("a cancelled run must stop");

        assert_eq!(error.category(), ProcessingJobErrorCategory::Canceled);
        assert!(transport.requests().is_empty());
    }
}
