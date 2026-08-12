# Meeting Intelligence

> Open-source Meeting Intelligence.
>
> Bring your own API key or run locally.

Meeting Intelligence is an early-stage, open-source foundation for speaker-aware
meeting capture and structured follow-up. The planned product shares one meeting
core across desktop, Slack, and self-hosted interfaces instead of maintaining
separate cloud and local products.

> [!IMPORTANT]
> This repository is in early development. The desktop application can process a
> real meeting with your own OpenAI key, but Slack, self-hosting, local
> inference, persistent speaker identity, and signed releases are not built yet.
> Development and CI stay in mock mode and never call a paid service.

## Planned interfaces

- Desktop (Tauri 2, React, and TypeScript)
- Slack (Socket Mode first, HTTP Events compatible by design)
- Self-hosted REST service

## Planned AI modes

- OpenAI (bring your own key)
- Hybrid
- Local

## What works today

Desktop, with your own OpenAI key:

- Import `.mp3`, `.m4a`, `.wav`, `.mp4`, and other common containers by drag and
  drop or file picker
- Speaker-aware transcription, with long recordings split on silence boundaries
  and merged back into one whole-meeting transcript
- Summary, decisions, action items, and open questions, each able to cite the
  transcript segments that support it
- Manual speaker naming that applies across the whole meeting without rewriting
  the canonical transcript
- A local meeting library that survives a restart, with retry for failed runs
- Markdown, JSON, and Word export

Also in the repository:

- Replaceable Rust provider contracts and canonical meeting types
- A deterministic fixture-to-analysis-to-export mock pipeline that needs no key
- A standalone mock local-AI service with health and capability endpoints
- Slack transport contracts and fixtures without a live Slack connection
- Secret-free CI and a Windows Tauri release foundation

## Where your data goes

```text
Cloud mode:
Audio and transcript needed for the AI processing you select are sent to the
configured provider.

Local meeting history remains on this device.
```

Your API key is stored in the Windows Credential Manager, never in a settings
file, the local database, or a log. There is no telemetry or analytics.

## Architecture

```text
Desktop / Slack / REST API
            |
            v
       Meeting Core
 ingest | jobs | transcript | speakers | analysis | export | storage
            |
            v
 Replaceable provider contracts
```

Cloud-backed and local providers implement the same contracts. Provider details
must not leak into UI components, and the UI never accesses SQLite directly.
See [Architecture](docs/architecture.md) and [Provider contracts](docs/provider-contracts.md).

## Run the desktop app

```bash
npm ci
npm run tauri dev --workspace @meeting-intelligence/desktop
```

On first launch, paste an OpenAI API key and use **Test connection**. Processing
a real recording also needs `ffmpeg` and `ffprobe` on `PATH`, or an FFmpeg folder
set in Settings.

To work without a key, choose the mock transcription and intelligence providers
in Settings; the whole path stays offline and free.

## Quick start (mock mode)

Prerequisites are Node.js 22.12+, Rust stable, and Python 3.12+.

```bash
npm ci
npm run lint
npm run typecheck
npm test
npm run build

cargo build --workspace
cargo test --workspace

python -m pip install -e "services/local-ai[dev]"
python -m pytest services/local-ai
```

The exact local setup and offline mock-pipeline command are documented in
[Development](docs/development.md).

## Security and privacy

Credentials must stay behind native or server boundaries and must never enter
React persistence, `localStorage`, repository files, or logs. Please read
[Security](docs/security.md), [Privacy](docs/privacy.md), and the
[vulnerability reporting policy](SECURITY.md).

## Contributing

Start with [CONTRIBUTING.md](CONTRIBUTING.md), then read the product and
architecture documents before changing code. The project is licensed under the
[MIT License](LICENSE).
