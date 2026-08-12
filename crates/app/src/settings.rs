use std::path::{Path, PathBuf};

use meeting_intelligence_core::AnalysisMode;
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::AppError;

/// Which implementation backs one capability.
///
/// Transcription and intelligence are configured separately so a later local
/// provider can replace either one without a product change.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderChoice {
    /// Deterministic fixtures. No credential, no network, no cost.
    Mock,
    #[default]
    OpenAi,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
}

/// Everything the application remembers between launches.
///
/// This type is serialized to a plain JSON file, so it must never gain a
/// credential field. Secrets live only in the OS credential store.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct AppSettings {
    pub transcription_provider: ProviderChoice,
    pub intelligence_provider: ProviderChoice,
    pub analysis_mode: AnalysisMode,
    pub theme: ThemeChoice,
    /// Where exports are written. `None` means "ask each time".
    pub output_directory: Option<PathBuf>,
    /// Directory containing `ffmpeg` and `ffprobe` when they are not on `PATH`.
    pub ffmpeg_directory: Option<PathBuf>,
    /// Optional transcription language hint, for example `zh` or `en`.
    pub language_hint: Option<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            transcription_provider: ProviderChoice::OpenAi,
            intelligence_provider: ProviderChoice::OpenAi,
            analysis_mode: AnalysisMode::Standard,
            theme: ThemeChoice::System,
            output_directory: None,
            ffmpeg_directory: None,
            language_hint: None,
        }
    }
}

impl AppSettings {
    /// Reads settings, falling back to defaults when the file is missing or
    /// unreadable. A corrupt settings file must not stop the app from starting.
    pub fn load(path: &Path) -> Self {
        let Ok(contents) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        match serde_json::from_str(&contents) {
            Ok(settings) => settings,
            Err(_) => {
                warn!("settings file could not be read; using defaults");
                Self::default()
            }
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), AppError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|_| AppError::FileSystem)?;
        }
        let contents = serde_json::to_string_pretty(self).map_err(|_| AppError::FileSystem)?;
        std::fs::write(path, contents).map_err(|_| AppError::FileSystem)
    }

    pub fn uses_openai(&self) -> bool {
        self.transcription_provider == ProviderChoice::OpenAi
            || self.intelligence_provider == ProviderChoice::OpenAi
    }

    pub fn language_hints(&self) -> Vec<String> {
        self.language_hint
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| vec![value.to_owned()])
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn defaults_select_openai_for_both_capabilities() {
        let settings = AppSettings::default();

        assert_eq!(settings.transcription_provider, ProviderChoice::OpenAi);
        assert_eq!(settings.intelligence_provider, ProviderChoice::OpenAi);
        assert_eq!(settings.analysis_mode, AnalysisMode::Standard);
        assert!(settings.uses_openai());
    }

    #[test]
    fn the_two_capabilities_can_be_configured_independently() {
        let settings = AppSettings {
            transcription_provider: ProviderChoice::Mock,
            intelligence_provider: ProviderChoice::OpenAi,
            ..AppSettings::default()
        };

        assert!(settings.uses_openai());

        let offline = AppSettings {
            transcription_provider: ProviderChoice::Mock,
            intelligence_provider: ProviderChoice::Mock,
            ..AppSettings::default()
        };
        assert!(!offline.uses_openai());
    }

    #[test]
    fn settings_round_trip_through_the_file() -> Result<(), AppError> {
        let directory = TempDir::new().expect("temp dir");
        let path = directory.path().join("nested").join("settings.json");
        let settings = AppSettings {
            analysis_mode: AnalysisMode::Deep,
            theme: ThemeChoice::Dark,
            output_directory: Some(PathBuf::from("C:/exports")),
            language_hint: Some("zh".to_owned()),
            ..AppSettings::default()
        };

        settings.save(&path)?;

        assert_eq!(AppSettings::load(&path), settings);
        Ok(())
    }

    #[test]
    fn the_serialized_settings_file_contains_no_credential_field() -> Result<(), AppError> {
        let directory = TempDir::new().expect("temp dir");
        let path = directory.path().join("settings.json");
        AppSettings::default().save(&path)?;

        let contents = std::fs::read_to_string(&path).expect("settings file");
        let lowered = contents.to_ascii_lowercase();
        for forbidden in ["api_key", "apikey", "token", "secret", "credential", "sk-"] {
            assert!(
                !lowered.contains(forbidden),
                "settings must never persist {forbidden}"
            );
        }
        Ok(())
    }

    #[test]
    fn a_missing_or_corrupt_settings_file_falls_back_to_defaults() {
        let directory = TempDir::new().expect("temp dir");
        let missing = directory.path().join("absent.json");
        assert_eq!(AppSettings::load(&missing), AppSettings::default());

        let corrupt = directory.path().join("corrupt.json");
        std::fs::write(&corrupt, "{ not json").expect("write corrupt settings");
        assert_eq!(AppSettings::load(&corrupt), AppSettings::default());
    }

    #[test]
    fn settings_written_by_an_older_build_keep_their_known_fields() {
        let directory = TempDir::new().expect("temp dir");
        let path = directory.path().join("settings.json");
        std::fs::write(&path, r#"{"analysis_mode":"deep"}"#).expect("write legacy settings");

        let settings = AppSettings::load(&path);

        assert_eq!(settings.analysis_mode, AnalysisMode::Deep);
        assert_eq!(settings.theme, ThemeChoice::System);
    }

    #[test]
    fn a_blank_language_hint_is_not_sent_to_a_provider() {
        let settings = AppSettings {
            language_hint: Some("   ".to_owned()),
            ..AppSettings::default()
        };

        assert!(settings.language_hints().is_empty());
    }
}
