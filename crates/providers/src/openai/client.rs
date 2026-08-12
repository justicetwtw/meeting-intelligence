use std::time::Duration;

use async_trait::async_trait;
use meeting_intelligence_core::ProviderError;
use meeting_intelligence_credentials::SecretString;
use tracing::warn;

use super::{
    config::OpenAiConfig,
    transport::{
        error_for_status, is_retryable_status, HttpMethod, HttpRequest, HttpResponse,
        HttpTransport, RequestBody, PROVIDER,
    },
};

/// Sends OpenAI requests over HTTPS.
///
/// The credential lives here and nowhere else in the process: it is written
/// straight into the `Authorization` header and never placed into a struct that
/// can be serialized, logged, or rendered.
pub struct ReqwestTransport {
    client: reqwest::Client,
    base_url: String,
    api_key: SecretString,
}

impl ReqwestTransport {
    pub fn new(config: &OpenAiConfig, api_key: SecretString) -> Result<Self, ProviderError> {
        let client = reqwest::Client::builder()
            .connect_timeout(config.connect_timeout)
            .timeout(config.request_timeout)
            .build()
            .map_err(|_| {
                ProviderError::configuration(PROVIDER, "the HTTPS client could not be created")
            })?;

        Ok(Self {
            client,
            base_url: config.base_url.trim_end_matches('/').to_owned(),
            api_key,
        })
    }
}

#[async_trait]
impl HttpTransport for ReqwestTransport {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, ProviderError> {
        let url = format!("{}{}", self.base_url, request.path);
        let mut builder = match request.method {
            HttpMethod::Get => self.client.get(&url),
            HttpMethod::Post => self.client.post(&url),
        }
        .bearer_auth(self.api_key.expose());

        builder = match request.body {
            RequestBody::None => builder,
            RequestBody::Json(body) => builder
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body),
            RequestBody::Multipart(upload) => {
                let part = reqwest::multipart::Part::bytes(upload.file_bytes)
                    .file_name(upload.file_name)
                    .mime_str(upload.file_media_type)
                    .map_err(|_| {
                        ProviderError::configuration(PROVIDER, "invalid upload media type")
                    })?;
                let mut form = reqwest::multipart::Form::new().part(upload.file_field, part);
                for (name, value) in upload.fields {
                    form = form.text(name, value);
                }
                builder.multipart(form)
            }
        };

        let response = builder.send().await.map_err(map_transport_error)?;
        let status = response.status().as_u16();
        // Reading the body can also fail mid-stream; that is still a transport
        // problem rather than a provider rejection.
        let body = response.text().await.map_err(map_transport_error)?;
        Ok(HttpResponse { status, body })
    }
}

/// Transport failures never carry the reqwest message, which can contain the
/// full request URL and, for some builds, header names.
fn map_transport_error(error: reqwest::Error) -> ProviderError {
    if error.is_timeout() {
        ProviderError::network(PROVIDER, "the request timed out")
    } else if error.is_connect() {
        ProviderError::network(PROVIDER, "the provider could not be reached")
    } else {
        ProviderError::network(PROVIDER, "the network request failed")
    }
}

/// Applies bounded retries around a transport.
pub struct RetryingClient {
    transport: std::sync::Arc<dyn HttpTransport>,
    max_retries: u32,
    base_delay: Duration,
}

impl RetryingClient {
    pub fn new(transport: std::sync::Arc<dyn HttpTransport>, config: &OpenAiConfig) -> Self {
        Self {
            transport,
            max_retries: config.max_retries,
            base_delay: config.retry_base_delay,
        }
    }

    /// Sends `request`, retrying only transient failures, and converts a
    /// non-success status into a typed error.
    pub async fn send(&self, request: HttpRequest) -> Result<String, ProviderError> {
        let mut attempt = 0_u32;
        loop {
            let outcome = self.transport.send(request.clone()).await;
            let retryable = match &outcome {
                Ok(response) => !response.is_success() && is_retryable_status(response.status),
                Err(error) => matches!(error, ProviderError::Network { .. }),
            };

            if !retryable || attempt >= self.max_retries {
                return match outcome? {
                    response if response.is_success() => Ok(response.body),
                    response => Err(error_for_status(response.status, &response.body)),
                };
            }

            attempt += 1;
            warn!(
                attempt,
                path = request.path,
                "retrying a transient OpenAI failure"
            );
            tokio::time::sleep(self.base_delay * attempt).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex, PoisonError};

    use meeting_intelligence_core::ProcessingJobErrorCategory;

    use super::*;

    struct ScriptedTransport {
        responses: Mutex<Vec<Result<HttpResponse, ProviderError>>>,
        calls: Mutex<u32>,
    }

    impl ScriptedTransport {
        fn new(responses: Vec<Result<HttpResponse, ProviderError>>) -> Arc<Self> {
            Arc::new(Self {
                responses: Mutex::new(responses),
                calls: Mutex::new(0),
            })
        }

        fn calls(&self) -> u32 {
            *self.calls.lock().unwrap_or_else(PoisonError::into_inner)
        }
    }

    #[async_trait]
    impl HttpTransport for ScriptedTransport {
        async fn send(&self, _request: HttpRequest) -> Result<HttpResponse, ProviderError> {
            *self.calls.lock().unwrap_or_else(PoisonError::into_inner) += 1;
            let mut responses = self
                .responses
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if responses.is_empty() {
                return Err(ProviderError::network(PROVIDER, "no scripted response"));
            }
            responses.remove(0)
        }
    }

    fn request() -> HttpRequest {
        HttpRequest {
            method: HttpMethod::Get,
            path: "/v1/models",
            body: RequestBody::None,
        }
    }

    fn config() -> OpenAiConfig {
        OpenAiConfig {
            max_retries: 2,
            retry_base_delay: Duration::from_millis(1),
            ..OpenAiConfig::default()
        }
    }

    #[tokio::test]
    async fn a_transient_failure_is_retried_until_it_succeeds() -> Result<(), ProviderError> {
        let transport = ScriptedTransport::new(vec![
            Ok(HttpResponse {
                status: 503,
                body: "unavailable".to_owned(),
            }),
            Ok(HttpResponse {
                status: 200,
                body: "{\"ok\":true}".to_owned(),
            }),
        ]);
        let client = RetryingClient::new(transport.clone(), &config());

        assert_eq!(client.send(request()).await?, "{\"ok\":true}");
        assert_eq!(transport.calls(), 2);
        Ok(())
    }

    #[tokio::test]
    async fn a_rejected_credential_is_never_retried() {
        let transport = ScriptedTransport::new(vec![Ok(HttpResponse {
            status: 401,
            body: r#"{"error":{"code":"invalid_api_key"}}"#.to_owned(),
        })]);
        let client = RetryingClient::new(transport.clone(), &config());

        let error = client
            .send(request())
            .await
            .expect_err("a rejected credential must fail");

        assert_eq!(error.category(), ProcessingJobErrorCategory::Credential);
        assert_eq!(transport.calls(), 1);
    }

    #[tokio::test]
    async fn retries_are_bounded() {
        let transport = ScriptedTransport::new(vec![
            Ok(HttpResponse {
                status: 500,
                body: String::new(),
            }),
            Ok(HttpResponse {
                status: 500,
                body: String::new(),
            }),
            Ok(HttpResponse {
                status: 500,
                body: String::new(),
            }),
            Ok(HttpResponse {
                status: 200,
                body: "late".to_owned(),
            }),
        ]);
        let client = RetryingClient::new(transport.clone(), &config());

        let error = client.send(request()).await.expect_err("must give up");

        assert_eq!(error.category(), ProcessingJobErrorCategory::Provider);
        assert_eq!(transport.calls(), 3, "one attempt plus two retries");
    }

    #[tokio::test]
    async fn network_failures_are_retried_and_stay_network_errors() {
        let transport = ScriptedTransport::new(vec![
            Err(ProviderError::network(PROVIDER, "connect failed")),
            Err(ProviderError::network(PROVIDER, "connect failed")),
            Err(ProviderError::network(PROVIDER, "connect failed")),
        ]);
        let client = RetryingClient::new(transport.clone(), &config());

        let error = client.send(request()).await.expect_err("must give up");

        assert_eq!(error.category(), ProcessingJobErrorCategory::Network);
        assert_eq!(transport.calls(), 3);
    }
}
