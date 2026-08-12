//! Provider implementations used by bootstrap and tests.
//!
//! This crate intentionally contains no production AI network calls.

#[cfg(feature = "http-client")]
mod http_foundation;
mod mocks;

#[cfg(feature = "http-client")]
pub use http_foundation::HttpClientFoundation;
pub use meeting_intelligence_core::{
    IntelligenceProvider, SpeakerIdentityProvider, StorageProvider, TranscriptionProvider,
};
pub use mocks::{
    CandidateIdentity, FixtureLoadError, FixtureTranscriptionProvider, InMemoryStorageProvider,
    MockIntelligenceProvider, MockSpeakerIdentityProvider,
};
