//! Native application services shared by every interface.
//!
//! This layer sits between an interface (the Tauri command boundary today) and
//! Meeting Core. It owns settings, provider selection, credential handling, the
//! meeting library, and exports, so no UI ever needs to know that OpenAI,
//! FFmpeg, or SQLite exist.

mod clock;
mod dto;
mod error;
mod providers;
mod service;
mod settings;

pub use clock::{format_rfc3339, unix_time_ms};
pub use dto::{
    format_duration, meeting_detail, AnalysisDto, AppStatusDto, CredentialStatusDto,
    EvidenceItemDto, ExportFormat, MeetingDetailDto, MeetingSummaryDto, SegmentDto, SettingsDto,
    SpeakerDto,
};
pub use error::{default_message, AppError, AppErrorDto};
pub use providers::{
    manual_speaker_identity, mock_intelligence, mock_transcription, ProviderBundle,
    ProviderFactory, RuntimeProviderFactory,
};
pub use service::{ChannelProgressReporter, ImportRequest, MeetingService};
pub use settings::{AppSettings, ProviderChoice, ThemeChoice};
