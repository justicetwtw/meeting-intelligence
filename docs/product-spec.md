# Product specification

## Positioning

> Open-source Meeting Intelligence. Bring your own API key or run locally.

The planned product provides desktop, Slack, and self-hosted entrypoints over one
meeting-processing core. Users choose OpenAI, hybrid, or fully local providers
without changing the canonical meeting model.

## Planned user capabilities

- Speaker-aware transcripts
- Persistent speaker profiles with explicit unknown fallback
- Meeting summaries, decisions, action items, and open questions
- Evidence links from decisions and action items to transcript segment IDs
- Word, Markdown, and JSON export
- Desktop, Slack, and self-hosted use
- Credential-free mock mode for development and CI

## Product principles

1. **One core, multiple interfaces.** Desktop, Slack, and REST share the same
   workflow and data contracts.
2. **Provider choice.** Network and local implementations are replaceable.
3. **Privacy by boundary.** Credentials and meeting content stay in the smallest
   appropriate native or server trust boundary.
4. **Evidence over unsupported claims.** Structured conclusions can reference
   stable transcript segments.
5. **Unknown is valid.** Speaker identity never guesses below a provider's
   accepted confidence threshold.
6. **Mock-first development.** The entire canonical path remains testable without
   keys, paid services, live Slack, a GPU, or downloaded models.

## Bootstrap acceptance

The bootstrap milestone provides repository structure, contracts, canonical
types, deterministic mocks, fixtures, minimal interface/service shells,
documentation, CI, release scaffolding, and security configuration. A fixture
must traverse Meeting Core, mock analysis, and mock export entirely offline.

## Explicit bootstrap non-goals

This milestone does not provide production transcription, a working Slack bot,
Whisper or pyannote downloads, Ollama, speaker embeddings, a complete UI,
production hosting, user accounts, billing, telemetry, analytics, or a finished
landing page. It must not call OpenAI or another paid AI API.

## Success criteria

- A clean clone installs and builds the frontend.
- The complete Rust workspace builds and tests.
- Python lint, formatting, and tests pass.
- The offline mock pipeline passes deterministically.
- Pull-request CI passes without secrets or external services.
- A `v*` tag has a Windows Tauri installer release path that requires no
  production signing credential during bootstrap.
