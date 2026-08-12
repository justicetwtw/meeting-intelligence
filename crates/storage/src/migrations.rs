//! Explicit, idempotently tracked SQLite migrations.
//!
//! Every migration is applied in order inside one transaction and the database
//! `user_version` is advanced with it, so a partially applied upgrade cannot be
//! mistaken for a complete one. Migrations are append-only: an existing entry is
//! never edited once it can have run against real meeting data.

use meeting_intelligence_core::ProviderError;
use rusqlite::Connection;

/// Schema version this build expects after `migrate` succeeds.
pub const SCHEMA_VERSION: i64 = 2;

const MIGRATIONS: &[&str] = &[
    // v1 — bootstrap document tables.
    "CREATE TABLE IF NOT EXISTS meetings (
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
     );",
    // v2 — deterministic meeting-history ordering for the desktop home screen.
    "ALTER TABLE meetings ADD COLUMN updated_seq INTEGER NOT NULL DEFAULT 0;
     UPDATE meetings SET updated_seq = rowid WHERE updated_seq = 0;
     CREATE INDEX IF NOT EXISTS meetings_updated_seq_idx
       ON meetings (updated_seq DESC, id ASC);",
];

/// Current schema version recorded in the database.
pub fn schema_version(connection: &Connection) -> Result<i64, ProviderError> {
    connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|error| ProviderError::failure("sqlite", error.to_string()))
}

pub(crate) fn migrate(connection: &mut Connection) -> Result<(), ProviderError> {
    let applied = schema_version(connection)?;
    let target = i64::try_from(MIGRATIONS.len()).unwrap_or(SCHEMA_VERSION);
    if applied > target {
        return Err(ProviderError::failure(
            "sqlite",
            format!(
                "database schema version {applied} is newer than this build supports ({target})"
            ),
        ));
    }

    let start = usize::try_from(applied).unwrap_or(0);
    for (offset, statements) in MIGRATIONS.iter().enumerate().skip(start) {
        let version = i64::try_from(offset + 1).unwrap_or(SCHEMA_VERSION);
        let transaction = connection
            .transaction()
            .map_err(|error| ProviderError::failure("sqlite", error.to_string()))?;
        transaction
            .execute_batch(statements)
            .map_err(|error| ProviderError::failure("sqlite", error.to_string()))?;
        transaction
            .pragma_update(None, "user_version", version)
            .map_err(|error| ProviderError::failure("sqlite", error.to_string()))?;
        transaction
            .commit()
            .map_err(|error| ProviderError::failure("sqlite", error.to_string()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrating_a_fresh_database_reaches_the_expected_version() -> Result<(), ProviderError> {
        let mut connection = Connection::open_in_memory()
            .map_err(|error| ProviderError::failure("sqlite", error.to_string()))?;
        migrate(&mut connection)?;

        assert_eq!(schema_version(&connection)?, SCHEMA_VERSION);
        Ok(())
    }

    #[test]
    fn migrating_twice_is_idempotent() -> Result<(), ProviderError> {
        let mut connection = Connection::open_in_memory()
            .map_err(|error| ProviderError::failure("sqlite", error.to_string()))?;
        migrate(&mut connection)?;
        migrate(&mut connection)?;

        assert_eq!(schema_version(&connection)?, SCHEMA_VERSION);
        Ok(())
    }

    #[test]
    fn a_bootstrap_v1_database_upgrades_without_losing_rows() -> Result<(), ProviderError> {
        let mut connection = Connection::open_in_memory()
            .map_err(|error| ProviderError::failure("sqlite", error.to_string()))?;
        connection
            .execute_batch(MIGRATIONS[0])
            .map_err(|error| ProviderError::failure("sqlite", error.to_string()))?;
        connection
            .pragma_update(None, "user_version", 1_i64)
            .map_err(|error| ProviderError::failure("sqlite", error.to_string()))?;
        connection
            .execute(
                "INSERT INTO meetings (id, payload) VALUES ('legacy', '{}')",
                [],
            )
            .map_err(|error| ProviderError::failure("sqlite", error.to_string()))?;

        migrate(&mut connection)?;

        assert_eq!(schema_version(&connection)?, SCHEMA_VERSION);
        let sequence: i64 = connection
            .query_row(
                "SELECT updated_seq FROM meetings WHERE id = 'legacy'",
                [],
                |row| row.get(0),
            )
            .map_err(|error| ProviderError::failure("sqlite", error.to_string()))?;
        assert!(sequence > 0, "existing rows must get an ordering value");
        Ok(())
    }

    #[test]
    fn a_newer_database_is_refused_instead_of_downgraded() -> Result<(), ProviderError> {
        let mut connection = Connection::open_in_memory()
            .map_err(|error| ProviderError::failure("sqlite", error.to_string()))?;
        connection
            .pragma_update(None, "user_version", SCHEMA_VERSION + 5)
            .map_err(|error| ProviderError::failure("sqlite", error.to_string()))?;

        let error = migrate(&mut connection).expect_err("a newer schema must be refused");
        assert!(error.to_string().contains("newer than this build"));
        Ok(())
    }
}
