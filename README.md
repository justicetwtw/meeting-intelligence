# Meeting Intelligence

> Open-source Meeting Intelligence.
>
> Bring your own API key or run locally.

Meeting Intelligence is an early-stage, open-source foundation for speaker-aware
meeting capture and structured follow-up. The planned product shares one meeting
core across desktop, Slack, and self-hosted interfaces instead of maintaining
separate cloud and local products.

> [!IMPORTANT]
> This repository is in early development. The checked-in code is a mock-first
> engineering skeleton, not a finished meeting assistant. It does not call OpenAI,
> Slack, or any paid service.

## Planned interfaces

- Desktop (Tauri 2, React, and TypeScript)
- Slack (Socket Mode first, HTTP Events compatible by design)
- Self-hosted REST service

## Planned AI modes

- OpenAI (bring your own key)
- Hybrid
- Local

## Bootstrap capabilities

- Replaceable Rust provider contracts and canonical meeting types
- A deterministic fixture-to-analysis-to-export mock pipeline
- A minimal React/Tauri desktop shell and shared design tokens
- A standalone mock local-AI service with health and capability endpoints
- Slack transport contracts and fixtures without a live Slack connection
- Secret-free CI and a Windows Tauri release foundation

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
