use std::{env, fs, path::Path, path::PathBuf, sync::Arc};

use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use meeting_intelligence_core::{
    AudioSource, Meeting, MeetingCore, ProcessMeetingRequest, SpeakerProfile, StorageProvider,
};
use meeting_intelligence_exporters::{
    ArtifactExporter, DocxExporter, ExportBundle, JsonExporter, MarkdownExporter,
};
use meeting_intelligence_providers::{
    FixtureTranscriptionProvider, MockIntelligenceProvider, MockSpeakerIdentityProvider,
};
use meeting_intelligence_storage::SqliteStorage;
use serde::de::DeserializeOwned;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    let _subscriber = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();

    let mut args = env::args_os().skip(1);
    let fixture_root = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("fixtures"));
    let output_root = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/mock-output"));
    if args.next().is_some() {
        bail!("usage: meeting-intelligence-server [fixture-root] [output-directory]");
    }

    let meeting: Meeting = read_fixture(&fixture_root.join("meeting.json"))?;
    let speaker_profiles: Vec<SpeakerProfile> =
        read_fixture(&fixture_root.join("speakers/profiles.json"))?;
    let transcription = FixtureTranscriptionProvider::from_json_path(
        fixture_root.join("transcription/bootstrap-planning.json"),
    )?;
    let speaker_identity = MockSpeakerIdentityProvider::from_json_path(
        fixture_root.join("speakers/identity-candidates.json"),
    )?;
    let intelligence = MockIntelligenceProvider::from_json_path(
        fixture_root.join("analysis/bootstrap-planning.json"),
    )?;
    let storage = Arc::new(SqliteStorage::open_in_memory()?);
    let core = MeetingCore::new(
        Arc::new(transcription),
        Arc::new(speaker_identity),
        Arc::new(intelligence),
        storage.clone(),
    );

    let meeting_id = meeting.id.clone();
    let processed = core
        .process(
            ProcessMeetingRequest::new(
                meeting,
                AudioSource::Fixture {
                    reference: "transcription/bootstrap-planning.json".to_owned(),
                },
            )
            .with_language_hints(vec!["en".to_owned()])
            .with_known_context(Some("offline bootstrap verification".to_owned()))
            .with_speaker_profiles(speaker_profiles),
        )
        .await?;

    let bundle = ExportBundle::from(&processed);
    let markdown = MarkdownExporter.export(&bundle)?;
    let json = JsonExporter.export(&bundle)?;
    let docx = DocxExporter.export(&bundle)?;
    storage.save_artifact(&markdown).await?;
    storage.save_artifact(&json).await?;
    storage.save_artifact(&docx).await?;

    tokio::fs::create_dir_all(&output_root)
        .await
        .with_context(|| format!("could not create {}", output_root.display()))?;
    let markdown_path = output_root.join("meeting.md");
    let json_path = output_root.join("meeting.json");
    let docx_path = output_root.join("meeting.docx");
    tokio::fs::write(&markdown_path, markdown.content)
        .await
        .with_context(|| format!("could not write {}", markdown_path.display()))?;
    tokio::fs::write(&json_path, json.content)
        .await
        .with_context(|| format!("could not write {}", json_path.display()))?;
    let docx_bytes = STANDARD
        .decode(&docx.content)
        .context("the Word artifact was not valid Base64")?;
    tokio::fs::write(&docx_path, docx_bytes)
        .await
        .with_context(|| format!("could not write {}", docx_path.display()))?;

    if storage.load_analysis(&meeting_id).await?.is_none() {
        bail!("mock pipeline completed without persisting its analysis");
    }

    info!(meeting_id, "offline mock pipeline completed");
    println!("wrote {}", markdown_path.display());
    println!("wrote {}", json_path.display());
    println!("wrote {}", docx_path.display());
    Ok(())
}

fn read_fixture<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let bytes = fs::read(path).with_context(|| format!("could not read {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("could not parse {}", path.display()))
}
