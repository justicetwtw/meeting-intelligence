//! Provider-neutral Meeting Intelligence domain model and orchestration.

mod error;
mod models;
mod pipeline;
mod ports;
mod progress;

pub use error::{CoreError, ProviderError};
pub use models::*;
pub use pipeline::{transcript_duration_ms, MeetingCore, ProcessMeetingRequest, ProcessedMeeting};
pub use ports::{
    IntelligenceProvider, RunContext, SpeakerIdentityProvider, StorageProvider,
    TranscriptionProvider,
};
pub use progress::{
    CancellationToken, NoopProgressReporter, ProcessingStage, ProgressDetail, ProgressEvent,
    ProgressReporter, RunProgress,
};
