use async_trait::async_trait;
use meeting_intelligence_core::ProviderError;

/// Provider name used in every error raised by the OpenAI adapters.
pub const PROVIDER: &str = "openai";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
}

/// One uploaded file plus the form fields that accompany it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultipartUpload {
    pub file_field: &'static str,
    pub file_name: String,
    pub file_media_type: &'static str,
    pub file_bytes: Vec<u8>,
    pub fields: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RequestBody {
    None,
    Json(String),
    Multipart(Box<MultipartUpload>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: HttpMethod,
    pub path: &'static str,
    pub body: RequestBody,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

impl HttpResponse {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// The single place where an OpenAI request leaves the process.
///
/// Everything above this trait is deterministic, which is what lets the
/// adapters be tested against recorded responses without a socket, a
/// credential, or a paid call.
#[async_trait]
pub trait HttpTransport: Send + Sync {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, ProviderError>;
}

/// Maps an HTTP status onto a typed, already-redacted provider error.
///
/// The response body is deliberately not included: it can echo request content
/// and, on some gateways, request headers.
pub fn error_for_status(status: u16, body: &str) -> ProviderError {
    let code = error_code(body);
    match status {
        401 => ProviderError::credential(
            PROVIDER,
            "the API key was rejected; check or replace it in Settings",
        ),
        403 => {
            ProviderError::credential(PROVIDER, "the API key is not permitted to use this model")
        }
        402 => ProviderError::quota(PROVIDER, "the account has no remaining credit"),
        429 if code.as_deref() == Some("insufficient_quota") => {
            ProviderError::quota(PROVIDER, "the account has no remaining credit")
        }
        429 => ProviderError::quota(PROVIDER, "the request was rate limited"),
        400 | 415 | 422 if is_audio_code(code.as_deref()) => {
            ProviderError::audio(PROVIDER, "the provider could not decode the prepared audio")
        }
        404 => ProviderError::configuration(
            PROVIDER,
            "the configured model or endpoint does not exist for this account",
        ),
        400..=499 => ProviderError::failure(PROVIDER, format!("request rejected ({status})")),
        _ => ProviderError::failure(PROVIDER, format!("service error ({status})")),
    }
}

/// `true` when a failure is worth retrying: rate limits and transient service
/// errors only. A rejected credential or a bad request never is.
pub fn is_retryable_status(status: u16) -> bool {
    status == 408 || status == 409 || status == 429 || (500..600).contains(&status)
}

fn error_code(body: &str) -> Option<String> {
    let document: serde_json::Value = serde_json::from_str(body).ok()?;
    let error = document.get("error")?;
    error
        .get("code")
        .and_then(serde_json::Value::as_str)
        .or_else(|| error.get("type").and_then(serde_json::Value::as_str))
        .map(str::to_owned)
}

fn is_audio_code(code: Option<&str>) -> bool {
    matches!(
        code,
        Some(
            "invalid_file_format"
                | "unsupported_file_format"
                | "audio_too_short"
                | "invalid_audio"
                | "file_too_large"
        )
    )
}

#[cfg(test)]
mod tests {
    use meeting_intelligence_core::ProcessingJobErrorCategory;

    use super::*;

    #[test]
    fn authentication_failures_are_credential_errors() {
        for status in [401, 403] {
            assert_eq!(
                error_for_status(status, "{}").category(),
                ProcessingJobErrorCategory::Credential
            );
        }
    }

    #[test]
    fn quota_and_rate_limits_are_quota_errors() {
        assert_eq!(
            error_for_status(402, "{}").category(),
            ProcessingJobErrorCategory::Quota
        );
        assert_eq!(
            error_for_status(
                429,
                r#"{"error":{"code":"insufficient_quota","message":"You exceeded your quota"}}"#
            )
            .category(),
            ProcessingJobErrorCategory::Quota
        );
        assert_eq!(
            error_for_status(429, "{}").category(),
            ProcessingJobErrorCategory::Quota
        );
    }

    #[test]
    fn undecodable_uploads_are_audio_errors() {
        assert_eq!(
            error_for_status(400, r#"{"error":{"code":"invalid_file_format"}}"#).category(),
            ProcessingJobErrorCategory::Audio
        );
    }

    #[test]
    fn a_missing_model_is_a_configuration_error() {
        assert_eq!(
            error_for_status(404, r#"{"error":{"code":"model_not_found"}}"#).category(),
            ProcessingJobErrorCategory::Configuration
        );
    }

    #[test]
    fn error_messages_never_echo_the_upstream_body() {
        let body = r#"{"error":{"code":"invalid_api_key",
            "message":"Incorrect API key provided: sk-live-SECRET123.",
            "request_headers":{"Authorization":"Bearer sk-live-SECRET123"}}}"#;

        for status in [400, 401, 403, 404, 422, 429, 500, 503] {
            let rendered = error_for_status(status, body).to_string();
            assert!(
                !rendered.contains("SECRET123"),
                "status {status} leaked a key"
            );
            assert!(
                !rendered.contains("Bearer"),
                "status {status} leaked a header"
            );
            assert!(
                !rendered.contains("Incorrect API key provided"),
                "status {status} echoed the upstream message"
            );
        }
    }

    #[test]
    fn only_transient_failures_are_retried() {
        for status in [408, 409, 429, 500, 502, 503, 504] {
            assert!(is_retryable_status(status), "{status} should be retried");
        }
        for status in [200, 400, 401, 402, 403, 404, 422] {
            assert!(!is_retryable_status(status), "{status} must not be retried");
        }
    }

    #[test]
    fn a_body_that_is_not_json_does_not_break_classification() {
        assert_eq!(
            error_for_status(500, "<html>Bad Gateway</html>").category(),
            ProcessingJobErrorCategory::Provider
        );
    }
}
