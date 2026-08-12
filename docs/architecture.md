# Architecture

## Goals

Meeting Intelligence uses one canonical meeting core across desktop, Slack, and
self-hosted interfaces. Cloud-backed and local operation are deployment choices,
not separate products. All external AI, identity, persistence, and transport
details remain replaceable behind explicit contracts.

## System view

```text
Interfaces
├── Desktop (Tauri command boundary)
├── Slack (transport boundary)
└── REST API (server boundary)
        |
        v
Meeting Core
├── ingest
├── jobs
├── transcript
├── speakers
├── analysis
├── export
└── storage orchestration
        |
        v
Providers
├── Transcription: mock | OpenAI (future) | local service (future)
├── Speaker identity: mock | local voiceprint (future) | manual fallback
├── Intelligence: mock | OpenAI (future) | compatible local endpoint (future)
└── Storage: in-memory mock | SQLite (foundation) | future implementations
```

The core owns workflow order and canonical data. Interfaces translate input and
render output; providers perform one bounded capability. Exporters consume
canonical data and never query UI state.

## Repository boundaries

| Path                | Responsibility                                | Must not own                                     |
| ------------------- | --------------------------------------------- | ------------------------------------------------ |
| `apps/desktop`      | React UI and Tauri command adapter            | provider credentials in web state; SQLite access |
| `apps/server`       | REST/application entrypoint foundation        | interface-specific business logic                |
| `crates/core`       | canonical model and workflow orchestration    | concrete cloud SDKs or UI types                  |
| `crates/providers`  | provider traits and mock adapters             | UI state or workflow sequencing                  |
| `crates/storage`    | persistence implementations                   | rendering or provider selection UI               |
| `crates/exporters`  | canonical artifact rendering                  | database access or analysis calls                |
| `services/local-ai` | standalone local capability HTTP service      | desktop process internals or cloud keys          |
| `packages/ui`       | accessible primitives and design tokens       | business workflow and secrets                    |
| `slack`             | transport contracts, DTOs, manifest, fixtures | meeting analysis logic                           |

Dependencies point inward: interfaces and concrete adapters depend on canonical
contracts; the core does not depend on interface frameworks.

## Canonical flow

```text
audio source or fixture
  -> TranscriptionProvider
  -> canonical Transcript
  -> optional SpeakerIdentityProvider
  -> IntelligenceProvider
  -> canonical MeetingAnalysis
  -> Exporter
  -> Artifact stored through StorageProvider
```

Every `TranscriptSegment` has a stable ID, speaker ID, millisecond start/end, and
text. Decisions and action items carry transcript segment IDs as evidence. A
provider may return no identity; low confidence is never converted into a guess.

## Deployment modes

### Desktop / BYOK

```text
React UI
  -> allow-listed Tauri command
  -> native backend
  -> OS-native secure credential storage
  -> selected provider
```

The webview receives capability and result DTOs, never raw credentials. No key is
stored in React state persistence, `localStorage`, or a JSON settings file.

### Self-hosted

```text
Desktop or Slack
  -> Meeting Server
  -> server-selected provider
```

Provider credentials exist only on the server. Clients submit authenticated,
bounded requests and receive canonical results.

### Local AI

The Python service is an independent process with a versioned HTTP contract.
Meeting Core calls it through a provider adapter. The service may eventually
host faster-whisper or diarization implementations, but model runtimes and GPU
requirements do not enter the core or bootstrap CI.

## Slack transports

`SlackTransport` normalizes incoming events. `SocketModeTransport` is the first
planned self-hosted implementation; `HttpEventsTransport` remains a future
adapter. Both feed the same event DTO and meeting application service. Neither
transport owns analysis logic.

## Observability

Use structured tracing with explicit redaction. Never log credentials,
authorization headers, raw tokens, complete sensitive requests, full recordings,
or full transcripts by default. Prefer stable IDs, operation names, durations,
sizes, and redacted error categories.

## Architecture change policy

Changes to provider contracts, canonical IDs, evidence links, trust boundaries,
storage ownership, or deployment topology are material decisions. Document the
tradeoff and obtain maintainer agreement before implementation. Small internal
refactors within a boundary do not require an architecture decision record.
