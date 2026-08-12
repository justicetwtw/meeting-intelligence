use std::time::Duration;

use meeting_intelligence_core::ProviderError;

/// Configuration-only HTTP foundation for future remote providers.
///
/// It deliberately exposes no request/send operation in the bootstrap so the
/// mock pipeline cannot accidentally make a network call.
#[derive(Clone, Debug)]
pub struct HttpClientFoundation {
    client: reqwest::Client,
    base_url: reqwest::Url,
}

impl HttpClientFoundation {
    pub fn new(base_url: &str) -> Result<Self, ProviderError> {
        let base_url = reqwest::Url::parse(base_url)
            .map_err(|error| ProviderError::failure("http", error.to_string()))?;
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|error| ProviderError::failure("http", error.to_string()))?;

        Ok(Self { client, base_url })
    }

    pub fn base_url(&self) -> &reqwest::Url {
        &self.base_url
    }

    pub fn is_configured(&self) -> bool {
        let _client = &self.client;
        self.base_url.has_host()
    }
}
