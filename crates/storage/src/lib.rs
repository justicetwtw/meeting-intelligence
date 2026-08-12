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

pub struct SqliteStorage {
    connection: Mutex<Connection>,
}

#[derive(Clone, Copy)]
enum DocumentTable {
    Meetings,
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
            Self::Meetings => {
                "INSERT INTO meetings (id, payload) VALUES (?1, ?2) \
                 ON CONFLICT(id) DO UPDATE SET payload = excluded.payload"
            }
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
            Self::Meetings => "SELECT payload FROM meetings WHERE id = ?1",
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

    fn from_connection(connection: Connection) -> Result<Self, ProviderError> {
        connection
            .execute_batch(
                "PRAGMA foreign_keys = ON;
                 CREATE TABLE IF NOT EXISTS meetings (
                   id TEXT PRIMARY KEY NOT NULL,
                   payload TEXT NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS processing_jobs (
                   id TEXT PRIMARY KEY NOT NULL,
                   payload TEXT NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS transcripts (
                   meeting_id TEXT PRIMARY KEY NOT NULL,
                   payload TEXT NOT NULL,
                   FOREIGN KEY(meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
                 );
                 CREATE TABLE IF NOT EXISTS speaker_sets (
                   meeting_id TEXT PRIMARY KEY NOT NULL,
                   payload TEXT NOT NULL,
                   FOREIGN KEY(meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
                 );
                 CREATE TABLE IF NOT EXISTS speaker_profiles (
                   id TEXT PRIMARY KEY NOT NULL,
                   payload TEXT NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS meeting_analyses (
                   meeting_id TEXT PRIMARY KEY NOT NULL,
                   payload TEXT NOT NULL,
                   FOREIGN KEY(meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
                 );
                 CREATE TABLE IF NOT EXISTS artifacts (
                   id TEXT PRIMARY KEY NOT NULL,
                   meeting_id TEXT NOT NULL,
                   payload TEXT NOT NULL,
                   FOREIGN KEY(meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
                 );
                 PRAGMA user_version = 1;",
            )
            .map_err(storage_error)?;
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
        self.upsert(DocumentTable::Meetings, &meeting.id, meeting)
    }

    async fn load_meeting(&self, meeting_id: &str) -> Result<Option<Meeting>, ProviderError> {
        self.load(DocumentTable::Meetings, meeting_id)
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
