//! Production OpenAI adapters.
//!
//! The module is feature-gated: nothing here is compiled into mock mode, so a
//! credential-free run cannot reach a paid API even by mistake. Everything
//! except [`client::ReqwestTransport`] is deterministic and tested against
//! recorded responses.

mod analysis;
mod client;
mod config;
mod intelligence;
mod mapping;
mod transcription;
mod transport;

use std::sync::Arc;

use meeting_intelligence_core::ProviderError;
use meeting_intelligence_credentials::SecretString;

pub use analysis::{EvidenceReport, SYSTEM_PROMPT};
pub use client::ReqwestTransport;
pub use config::{
    OpenAiConfig, ReasoningPresets, DEFAULT_BASE_URL, DEFAULT_INTELLIGENCE_MODEL,
    DEFAULT_TRANSCRIPTION_MODEL,
};
pub use intelligence::OpenAiIntelligenceProvider;
pub use transcription::OpenAiTranscriptionProvider;
pub use transport::{
    HttpMethod, HttpRequest, HttpResponse, HttpTransport, MultipartUpload, RequestBody, PROVIDER,
};

use transport::error_for_status;

/// Checks a credential without spending analysis or transcription budget.
///
/// It lists models, which is the cheapest authenticated call the API offers.
/// A success proves the key is accepted; the model list itself is discarded.
pub async fn verify_credential(transport: &dyn HttpTransport) -> Result<(), ProviderError> {
    let response = transport
        .send(HttpRequest {
            method: HttpMethod::Get,
            path: "/v1/models",
            body: RequestBody::None,
        })
        .await?;

    if response.is_success() {
        Ok(())
    } else {
        Err(error_for_status(response.status, &response.body))
    }
}

/// Builds a live HTTPS transport for the supplied credential.
pub fn live_transport(
    config: &OpenAiConfig,
    api_key: SecretString,
) -> Result<Arc<dyn HttpTransport>, ProviderError> {
    Ok(Arc::new(ReqwestTransport::new(config, api_key)?))
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use meeting_intelligence_core::ProcessingJobErrorCategory;
    use std::sync::{Mutex, PoisonError};

    use super::*;

    struct FixedTransport {
        response: HttpResponse,
        requests: Mutex<Vec<HttpRequest>>,
    }

    #[async_trait]
    impl HttpTransport for FixedTransport {
        async fn send(&self, request: HttpRequest) -> Result<HttpResponse, ProviderError> {
            self.requests
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(request);
            Ok(self.response.clone())
        }
    }

    fn transport(status: u16, body: &str) -> FixedTransport {
        FixedTransport {
            response: HttpResponse {
                status,
                body: body.to_owned(),
            },
            requests: Mutex::new(Vec::new()),
        }
    }

    #[tokio::test]
    async fn a_working_credential_verifies_with_one_cheap_call() -> Result<(), ProviderError> {
        let transport = transport(200, r#"{"data":[{"id":"gpt-5.6-luna"}]}"#);

        verify_credential(&transport).await?;

        let requests = transport
            .requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].path, "/v1/models");
        assert_eq!(requests[0].method, HttpMethod::Get);
        assert_eq!(requests[0].body, RequestBody::None);
        Ok(())
    }

    #[tokio::test]
    async fn a_rejected_credential_reports_the_credential_category() {
        let transport = transport(401, r#"{"error":{"code":"invalid_api_key"}}"#);

        let error = verify_credential(&transport)
            .await
            .expect_err("a rejected key must fail verification");

        assert_eq!(error.category(), ProcessingJobErrorCategory::Credential);
    }

    #[tokio::test]
    async fn an_exhausted_account_reports_the_quota_category() {
        let transport = transport(429, r#"{"error":{"code":"insufficient_quota"}}"#);

        let error = verify_credential(&transport)
            .await
            .expect_err("an exhausted account must fail verification");

        assert_eq!(error.category(), ProcessingJobErrorCategory::Quota);
    }
}
