//! Tauri 2 native boundary for Meeting Intelligence.
//!
//! The webview reaches Meeting Core only through the allow-listed commands in
//! [`commands`]. It never receives a credential, never sees a provider, and
//! never touches SQLite.

mod commands;

use std::sync::Arc;

use meeting_intelligence_app::{MeetingService, RuntimeProviderFactory};
use meeting_intelligence_credentials::OsCredentialStore;
use meeting_intelligence_storage::SqliteStorage;
use tauri::Manager;
use tracing_subscriber::EnvFilter;

use commands::AppState;

pub fn run() {
    // Structured logging with an explicit default: no request bodies, no
    // transcripts, and no credentials are ever recorded.
    let _subscriber = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .try_init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_directory = commands::data_directory(&app.handle().clone());
            std::fs::create_dir_all(&data_directory)?;

            let storage = Arc::new(SqliteStorage::open(data_directory.join("library.sqlite"))?);
            let credentials = Arc::new(OsCredentialStore::new());
            let factory = Arc::new(RuntimeProviderFactory::new(credentials.clone()));
            let service = Arc::new(MeetingService::new(
                storage,
                credentials,
                factory,
                data_directory.join("settings.json"),
            ));

            app.manage(AppState::new(service));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_status,
            commands::update_settings,
            commands::save_openai_key,
            commands::remove_openai_key,
            commands::test_openai_connection,
            commands::list_meetings,
            commands::get_meeting,
            commands::import_meeting,
            commands::retry_meeting,
            commands::cancel_processing,
            commands::rename_speaker,
            commands::export_meeting,
        ])
        .run(tauri::generate_context!())
        .expect("Tauri application failed to run");
}
