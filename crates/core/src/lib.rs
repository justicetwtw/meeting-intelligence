//! Provider-neutral Meeting Intelligence domain model and orchestration.

mod error;
mod models;
mod pipeline;
mod ports;

pub use error::{CoreError, ProviderError};
pub use models::*;
pub use pipeline::{MeetingCore, ProcessMeetingRequest, ProcessedMeeting};
pub use ports::{
    IntelligenceProvider, SpeakerIdentityProvider, StorageProvider, TranscriptionProvider,
};
