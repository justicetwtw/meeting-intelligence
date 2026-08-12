use std::time::Duration;

use meeting_intelligence_core::AnalysisMode;

/// Default OpenAI API root. Overridable so an operator can point at a
/// compatible gateway without a code change.
pub const DEFAULT_BASE_URL: &str = "https://api.openai.com";
/// Speaker-aware transcription model.
pub const DEFAULT_TRANSCRIPTION_MODEL: &str = "gpt-4o-transcribe-diarize";
/// Meeting analysis model.
pub const DEFAULT_INTELLIGENCE_MODEL: &str = "gpt-5.6-luna";

/// Maps the three product-level analysis modes onto provider reasoning effort.
///
/// Interfaces show Fast / Standard / Deep and never a vendor parameter; the
/// mapping lives here so it is configured in exactly one place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReasoningPresets {
    pub fast: String,
    pub standard: String,
    pub deep: String,
}

impl Default for ReasoningPresets {
    fn default() -> Self {
        Self {
            fast: "low".to_owned(),
            standard: "medium".to_owned(),
            deep: "max".to_owned(),
        }
    }
}

impl ReasoningPresets {
    pub fn effort_for(&self, mode: AnalysisMode) -> &str {
        match mode {
            AnalysisMode::Fast => &self.fast,
            AnalysisMode::Standard => &self.standard,
            AnalysisMode::Deep => &self.deep,
        }
    }
}

#[derive(Clone, Debug)]
pub struct OpenAiConfig {
    pub base_url: String,
    pub transcription_model: String,
    pub intelligence_model: String,
    pub reasoning_presets: ReasoningPresets,
    pub request_timeout: Duration,
    pub connect_timeout: Duration,
    /// Attempts after the first one, for retryable failures only.
    pub max_retries: u32,
    pub retry_base_delay: Duration,
    /// Refuses to build a prompt beyond this size instead of silently dropping
    /// part of a meeting.
    pub max_transcript_characters: usize,
    pub max_output_tokens: u32,
}

impl Default for OpenAiConfig {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.to_owned(),
            transcription_model: DEFAULT_TRANSCRIPTION_MODEL.to_owned(),
            intelligence_model: DEFAULT_INTELLIGENCE_MODEL.to_owned(),
            reasoning_presets: ReasoningPresets::default(),
            // A long meeting chunk can take several minutes to transcribe.
            request_timeout: Duration::from_secs(900),
            connect_timeout: Duration::from_secs(20),
            max_retries: 2,
            retry_base_delay: Duration::from_secs(2),
            max_transcript_characters: 600_000,
            max_output_tokens: 16_000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_presets_map_to_distinct_efforts() {
        let presets = ReasoningPresets::default();

        assert_eq!(presets.effort_for(AnalysisMode::Fast), "low");
        assert_eq!(presets.effort_for(AnalysisMode::Standard), "medium");
        assert_eq!(presets.effort_for(AnalysisMode::Deep), "max");
    }

    #[test]
    fn presets_are_configuration_rather_than_hard_coded_call_sites() {
        let presets = ReasoningPresets {
            fast: "minimal".to_owned(),
            standard: "low".to_owned(),
            deep: "high".to_owned(),
        };

        assert_eq!(presets.effort_for(AnalysisMode::Deep), "high");
    }

    #[test]
    fn the_default_configuration_names_the_milestone_models() {
        let config = OpenAiConfig::default();

        assert_eq!(config.transcription_model, "gpt-4o-transcribe-diarize");
        assert_eq!(config.intelligence_model, "gpt-5.6-luna");
        assert_eq!(config.base_url, "https://api.openai.com");
    }
}
