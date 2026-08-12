use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use serde::{Deserialize, Serialize};

use crate::{CoreError, ProcessingJobErrorCategory};

/// Coarse, truthful stages of a meeting run.
///
/// The product deliberately has no percentage: nothing in the pipeline can
/// measure meaningful fractional progress, so interfaces render the stage name
/// instead of an invented number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessingStage {
    PreparingAudio,
    Uploading,
    Transcribing,
    Analyzing,
    Saving,
    Completed,
    Failed,
}

impl ProcessingStage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PreparingAudio => "preparing_audio",
            Self::Uploading => "uploading",
            Self::Transcribing => "transcribing",
            Self::Analyzing => "analyzing",
            Self::Saving => "saving",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

/// A single progress observation. `error_category` is only ever set on
/// [`ProcessingStage::Failed`] and is always a redacted category.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgressEvent {
    pub meeting_id: String,
    pub job_id: String,
    pub stage: ProcessingStage,
    #[serde(default)]
    pub detail: Option<ProgressDetail>,
    #[serde(default)]
    pub error_category: Option<ProcessingJobErrorCategory>,
}

/// Optional, bounded extra context. Only counts are carried here: never file
/// names, transcript text, or provider payloads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgressDetail {
    pub current_part: u32,
    pub total_parts: u32,
}

pub trait ProgressReporter: Send + Sync {
    fn report(&self, event: ProgressEvent);
}

/// Reporter used when an interface does not observe progress.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoopProgressReporter;

impl ProgressReporter for NoopProgressReporter {
    fn report(&self, _event: ProgressEvent) {}
}

/// Progress sink bound to one meeting run.
#[derive(Clone)]
pub struct RunProgress {
    reporter: Arc<dyn ProgressReporter>,
    meeting_id: String,
    job_id: String,
}

impl RunProgress {
    pub fn new(
        reporter: Arc<dyn ProgressReporter>,
        meeting_id: impl Into<String>,
        job_id: impl Into<String>,
    ) -> Self {
        Self {
            reporter,
            meeting_id: meeting_id.into(),
            job_id: job_id.into(),
        }
    }

    pub fn disabled() -> Self {
        Self::new(Arc::new(NoopProgressReporter), "", "")
    }

    pub fn stage(&self, stage: ProcessingStage) {
        self.emit(stage, None, None);
    }

    pub fn stage_with_parts(&self, stage: ProcessingStage, current_part: u32, total_parts: u32) {
        self.emit(
            stage,
            Some(ProgressDetail {
                current_part,
                total_parts,
            }),
            None,
        );
    }

    pub fn failed(&self, error_category: ProcessingJobErrorCategory) {
        self.emit(ProcessingStage::Failed, None, Some(error_category));
    }

    fn emit(
        &self,
        stage: ProcessingStage,
        detail: Option<ProgressDetail>,
        error_category: Option<ProcessingJobErrorCategory>,
    ) {
        self.reporter.report(ProgressEvent {
            meeting_id: self.meeting_id.clone(),
            job_id: self.job_id.clone(),
            stage,
            detail,
            error_category,
        });
    }
}

impl std::fmt::Debug for RunProgress {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RunProgress")
            .field("meeting_id", &self.meeting_id)
            .field("job_id", &self.job_id)
            .finish_non_exhaustive()
    }
}

/// Cooperative cancellation shared between the interface, Meeting Core, and
/// long-running providers. Cancellation is checked at stage boundaries and by
/// providers around their own external work.
#[derive(Clone, Debug, Default)]
pub struct CancellationToken {
    canceled: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.canceled.store(true, Ordering::SeqCst);
    }

    pub fn is_canceled(&self) -> bool {
        self.canceled.load(Ordering::SeqCst)
    }

    pub fn check(&self) -> Result<(), CoreError> {
        if self.is_canceled() {
            Err(CoreError::Canceled)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct RecordingReporter {
        events: Mutex<Vec<ProgressEvent>>,
    }

    impl ProgressReporter for RecordingReporter {
        fn report(&self, event: ProgressEvent) {
            if let Ok(mut events) = self.events.lock() {
                events.push(event);
            }
        }
    }

    #[test]
    fn progress_events_carry_ids_stages_and_bounded_details_only() {
        let reporter = Arc::new(RecordingReporter::default());
        let progress = RunProgress::new(reporter.clone(), "meeting-1", "job-1");

        progress.stage(ProcessingStage::PreparingAudio);
        progress.stage_with_parts(ProcessingStage::Uploading, 2, 3);
        progress.failed(ProcessingJobErrorCategory::Network);

        let events = reporter.events.lock().expect("reporter lock").clone();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].stage, ProcessingStage::PreparingAudio);
        assert_eq!(events[0].detail, None);
        assert_eq!(events[0].meeting_id, "meeting-1");
        assert_eq!(events[0].job_id, "job-1");
        assert_eq!(
            events[1].detail,
            Some(ProgressDetail {
                current_part: 2,
                total_parts: 3
            })
        );
        assert_eq!(events[2].stage, ProcessingStage::Failed);
        assert_eq!(
            events[2].error_category,
            Some(ProcessingJobErrorCategory::Network)
        );
    }

    #[test]
    fn cancellation_is_observable_and_sticky() {
        let token = CancellationToken::new();
        assert!(token.check().is_ok());

        let clone = token.clone();
        clone.cancel();

        assert!(token.is_canceled());
        assert!(matches!(token.check(), Err(CoreError::Canceled)));
    }
}
