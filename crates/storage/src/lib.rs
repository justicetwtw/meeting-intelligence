//! SQLite persistence behind the provider-neutral storage port.

use std::{
    path::Path,
    sync::{Mutex, MutexGuard},
};

use async_trait::async_trait;
use meeting_intelligence_core::{
    Artifact, Meeting, MeetingAnalysis, ProcessingJob, ProviderError, Speaker, SpeakerProfile,
    StorageProvider, Transcript,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};

mod migrations;

pub use migrations::{schema_version, SCHEMA_VERSION};

pub struct SqliteStorage {
    connection: Mutex<Connection>,
}

#[derive(Clone, Copy)]
enum DocumentTable {
    Jobs,
    Transcripts,
    SpeakerSets,
    SpeakerProfiles,
    Analyses,
    Artifacts,
}

impl DocumentTable {
    fn upsert_sql(self) -> &'static str {
        match self {
            Self::Jobs => {
                "INSERT INTO processing_jobs (id, payload) VALUES (?1, ?2) \
                 ON CONFLICT(id) DO UPDATE SET payload = excluded.payload"
            }
            Self::Transcripts => {
                "INSERT INTO transcripts (meeting_id, payload) VALUES (?1, ?2) \
                 ON CONFLICT(meeting_id) DO UPDATE SET payload = excluded.payload"
            }
            Self::SpeakerSets => {
                "INSERT INTO speaker_sets (meeting_id, payload) VALUES (?1, ?2) \
                 ON CONFLICT(meeting_id) DO UPDATE SET payload = excluded.payload"
            }
            Self::SpeakerProfiles => {
                "INSERT INTO speaker_profiles (id, payload) VALUES (?1, ?2) \
                 ON CONFLICT(id) DO UPDATE SET payload = excluded.payload"
            }
            Self::Analyses => {
                "INSERT INTO meeting_analyses (meeting_id, payload) VALUES (?1, ?2) \
                 ON CONFLICT(meeting_id) DO UPDATE SET payload = excluded.payload"
            }
            Self::Artifacts => {
                "INSERT INTO artifacts (id, meeting_id, payload) VALUES (?1, ?2, ?3) \
                 ON CONFLICT(id) DO UPDATE SET \
                 meeting_id = excluded.meeting_id, payload = excluded.payload"
            }
        }
    }

    fn select_sql(self) -> &'static str {
        match self {
            Self::Jobs => "SELECT payload FROM processing_jobs WHERE id = ?1",
            Self::Transcripts => "SELECT payload FROM transcripts WHERE meeting_id = ?1",
            Self::SpeakerSets => "SELECT payload FROM speaker_sets WHERE meeting_id = ?1",
            Self::SpeakerProfiles => "SELECT payload FROM speaker_profiles WHERE id = ?1",
            Self::Analyses => "SELECT payload FROM meeting_analyses WHERE meeting_id = ?1",
            Self::Artifacts => "SELECT payload FROM artifacts WHERE id = ?1",
        }
    }
}

impl SqliteStorage {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ProviderError> {
        let connection = Connection::open(path).map_err(storage_error)?;
        Self::from_connection(connection)
    }

    pub fn open_in_memory() -> Result<Self, ProviderError> {
        let connection = Connection::open_in_memory().map_err(storage_error)?;
        Self::from_connection(connection)
    }

    fn from_connection(mut connection: Connection) -> Result<Self, ProviderError> {
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(storage_error)?;
        migrations::migrate(&mut connection)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    fn connection(&self) -> Result<MutexGuard<'_, Connection>, ProviderError> {
        self.connection
            .lock()
            .map_err(|_| ProviderError::failure("sqlite", "connection lock was poisoned"))
    }

    fn upsert<T: Serialize>(
        &self,
        table: DocumentTable,
        id: &str,
        value: &T,
    ) -> Result<(), ProviderError> {
        let payload = serde_json::to_string(value).map_err(storage_error)?;
        self.connection()?
            .execute(table.upsert_sql(), params![id, payload])
            .map_err(storage_error)?;
        Ok(())
    }

    fn upsert_artifact(&self, artifact: &Artifact) -> Result<(), ProviderError> {
        let payload = serde_json::to_string(artifact).map_err(storage_error)?;
        self.connection()?
            .execute(
                DocumentTable::Artifacts.upsert_sql(),
                params![artifact.id, artifact.meeting_id, payload],
            )
            .map_err(storage_error)?;
        Ok(())
    }

    fn load<T: DeserializeOwned>(
        &self,
        table: DocumentTable,
        id: &str,
    ) -> Result<Option<T>, ProviderError> {
        let payload: Option<String> = self
            .connection()?
            .query_row(table.select_sql(), params![id], |row| row.get(0))
            .optional()
            .map_err(storage_error)?;
        payload
            .map(|payload| serde_json::from_str(&payload).map_err(storage_error))
            .transpose()
    }
}

fn storage_error(error: impl std::fmt::Display) -> ProviderError {
    ProviderError::failure("sqlite", error.to_string())
}

#[async_trait]
impl StorageProvider for SqliteStorage {
    async fn save_meeting(&self, meeting: &Meeting) -> Result<(), ProviderError> {
        let payload = serde_json::to_string(meeting).map_err(storage_error)?;
        let connection = self.connection()?;
        // A monotonic sequence keeps history ordering deterministic without
        // depending on wall-clock resolution.
        let next_sequence: i64 = connection
            .query_row(
                "SELECT COALESCE(MAX(updated_seq), 0) + 1 FROM meetings",
                [],
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        connection
            .execute(
                "INSERT INTO meetings (id, payload, updated_seq) VALUES (?1, ?2, ?3) \
                 ON CONFLICT(id) DO UPDATE SET \
                 payload = excluded.payload, updated_seq = excluded.updated_seq",
                params![meeting.id, payload, next_sequence],
            )
            .map_err(storage_error)?;
        Ok(())
    }

    async fn load_meeting(&self, meeting_id: &str) -> Result<Option<Meeting>, ProviderError> {
        let payload: Option<String> = self
            .connection()?
            .query_row(
                "SELECT payload FROM meetings WHERE id = ?1",
                params![meeting_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(storage_error)?;
        payload
            .map(|payload| serde_json::from_str(&payload).map_err(storage_error))
            .transpose()
    }

    async fn list_meetings(&self) -> Result<Vec<Meeting>, ProviderError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare("SELECT payload FROM meetings ORDER BY updated_seq DESC, id ASC")
            .map_err(storage_error)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(storage_error)?;

        let mut meetings = Vec::new();
        for row in rows {
            let payload = row.map_err(storage_error)?;
            meetings.push(serde_json::from_str(&payload).map_err(storage_error)?);
        }
        Ok(meetings)
    }

    async fn save_job(&self, job: &ProcessingJob) -> Result<(), ProviderError> {
        self.upsert(DocumentTable::Jobs, &job.id, job)
    }

    async fn load_job(&self, job_id: &str) -> Result<Option<ProcessingJob>, ProviderError> {
        self.load(DocumentTable::Jobs, job_id)
    }

    async fn save_transcript(&self, transcript: &Transcript) -> Result<(), ProviderError> {
        self.upsert(
            DocumentTable::Transcripts,
            &transcript.meeting_id,
            transcript,
        )
    }

    async fn load_transcript(&self, meeting_id: &str) -> Result<Option<Transcript>, ProviderError> {
        self.load(DocumentTable::Transcripts, meeting_id)
    }

    async fn save_speakers(
        &self,
        meeting_id: &str,
        speakers: &[Speaker],
    ) -> Result<(), ProviderError> {
        self.upsert(DocumentTable::SpeakerSets, meeting_id, &speakers)
    }

    async fn load_speakers(&self, meeting_id: &str) -> Result<Vec<Speaker>, ProviderError> {
        Ok(self
            .load(DocumentTable::SpeakerSets, meeting_id)?
            .unwrap_or_default())
    }

    async fn save_speaker_profile(&self, profile: &SpeakerProfile) -> Result<(), ProviderError> {
        self.upsert(DocumentTable::SpeakerProfiles, &profile.id, profile)
    }

    async fn load_speaker_profile(
        &self,
        profile_id: &str,
    ) -> Result<Option<SpeakerProfile>, ProviderError> {
        self.load(DocumentTable::SpeakerProfiles, profile_id)
    }

    async fn save_analysis(&self, analysis: &MeetingAnalysis) -> Result<(), ProviderError> {
        self.upsert(DocumentTable::Analyses, &analysis.meeting_id, analysis)
    }

    async fn load_analysis(
        &self,
        meeting_id: &str,
    ) -> Result<Option<MeetingAnalysis>, ProviderError> {
        self.load(DocumentTable::Analyses, meeting_id)
    }

    async fn save_artifact(&self, artifact: &Artifact) -> Result<(), ProviderError> {
        self.upsert_artifact(artifact)
    }

    async fn load_artifact(&self, artifact_id: &str) -> Result<Option<Artifact>, ProviderError> {
        self.load(DocumentTable::Artifacts, artifact_id)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use meeting_intelligence_core::{
        ArtifactEncoding, ArtifactKind, MeetingProcessingState, SpeakerIdentityStatus,
        SpeakerScope, TranscriptSegment,
    };

    use super::*;

    fn meeting(id: &str, title: &str) -> Meeting {
        Meeting {
            id: id.to_owned(),
            title: title.to_owned(),
            started_at: Some("2026-08-12T09:00:00Z".to_owned()),
            processing_state: MeetingProcessingState::Completed,
            artifact_ids: Vec::new(),
            participants: Vec::new(),
            duration_ms: Some(5_247_000),
            metadata: BTreeMap::new(),
        }
    }

    #[tokio::test]
    async fn history_lists_the_most_recently_saved_meeting_first() -> Result<(), ProviderError> {
        let storage = SqliteStorage::open_in_memory()?;
        storage.save_meeting(&meeting("m1", "First")).await?;
        storage.save_meeting(&meeting("m2", "Second")).await?;

        let ids: Vec<String> = storage
            .list_meetings()
            .await?
            .into_iter()
            .map(|value| value.id)
            .collect();
        assert_eq!(ids, vec!["m2".to_owned(), "m1".to_owned()]);

        storage.save_meeting(&meeting("m1", "First again")).await?;
        let ids: Vec<String> = storage
            .list_meetings()
            .await?
            .into_iter()
            .map(|value| value.id)
            .collect();
        assert_eq!(ids, vec!["m1".to_owned(), "m2".to_owned()]);
        Ok(())
    }

    #[tokio::test]
    async fn history_is_empty_before_any_meeting_exists() -> Result<(), ProviderError> {
        let storage = SqliteStorage::open_in_memory()?;
        assert!(storage.list_meetings().await?.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn manual_speaker_labels_and_binary_artifacts_round_trip() -> Result<(), ProviderError> {
        let storage = SqliteStorage::open_in_memory()?;
        storage.save_meeting(&meeting("m1", "Round trip")).await?;

        let transcript = Transcript {
            meeting_id: "m1".to_owned(),
            language: Some("zh".to_owned()),
            speaker_scope: SpeakerScope::Chunk,
            segments: vec![TranscriptSegment {
                id: "seg_00001".to_owned(),
                speaker_id: "speaker_c01_01".to_owned(),
                start_ms: 0,
                end_ms: 1_500,
                text: "hello".to_owned(),
            }],
        };
        storage.save_transcript(&transcript).await?;
        assert_eq!(
            storage.load_transcript("m1").await?.as_ref(),
            Some(&transcript)
        );

        let speakers = vec![Speaker {
            transcript_speaker_id: "speaker_c01_01".to_owned(),
            identity: SpeakerIdentityStatus::Unknown,
            profile_id: None,
            display_name: None,
            confidence: None,
            manual_display_name: Some("Mr. Wang".to_owned()),
        }];
        storage.save_speakers("m1", &speakers).await?;
        assert_eq!(storage.load_speakers("m1").await?, speakers);

        let artifact = Artifact {
            id: "artifact-m1-docx".to_owned(),
            meeting_id: "m1".to_owned(),
            kind: ArtifactKind::Docx,
            media_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
                .to_owned(),
            content: "UEsDBA==".to_owned(),
            encoding: ArtifactEncoding::Base64,
            created_at: None,
        };
        storage.save_artifact(&artifact).await?;
        assert_eq!(
            storage.load_artifact("artifact-m1-docx").await?,
            Some(artifact)
        );
        Ok(())
    }
}
