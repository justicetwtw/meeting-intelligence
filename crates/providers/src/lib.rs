//! Provider implementations.
//!
//! Mock providers are always available and never touch the network. The OpenAI
//! adapters live behind the `openai` feature, so an offline or credential-free
//! build cannot make a paid call.

#[cfg(feature = "http-client")]
mod http_foundation;
mod mocks;
#[cfg(feature = "openai")]
pub mod openai;

#[cfg(feature = "http-client")]
pub use http_foundation::HttpClientFoundation;
pub use meeting_intelligence_core::{
    IntelligenceProvider, RunContext, SpeakerIdentityProvider, StorageProvider,
    TranscriptionProvider,
};
pub use mocks::{
    CandidateIdentity, FixtureLoadError, FixtureTranscriptionProvider, InMemoryStorageProvider,
    MockIntelligenceProvider, MockSpeakerIdentityProvider,
};
