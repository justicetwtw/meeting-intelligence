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
Application services (`crates/app`)
├── settings and provider selection
├── credential operations
├── meeting library and exports
└── progress and cancellation plumbing
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
├── Transcription: mock | OpenAI | local service (future)
├── Speaker identity: manual (M1) | local voiceprint (future)
├── Intelligence: mock | OpenAI | compatible local endpoint (future)
└── Storage: in-memory mock | SQLite
```

Application services exist so that no interface has to sequence a run itself.
The desktop command boundary is one adapter over them; a future REST interface
is another. They own no domain rules — that stays in Meeting Core.

The core owns workflow order and canonical data. Interfaces translate input and
render output; providers perform one bounded capability. Exporters consume
canonical data and never query UI state.

## Repository boundaries

| Path                 | Responsibility                                      | Must not own                                     |
| -------------------- | --------------------------------------------------- | ------------------------------------------------ |
| `apps/desktop`       | React UI and Tauri command adapter                  | provider credentials in web state; SQLite access |
| `apps/server`        | REST/application entrypoint foundation              | interface-specific business logic                |
| `crates/app`         | application services shared by every interface      | canonical domain rules; UI frameworks            |
| `crates/audio`       | FFmpeg ingestion, normalization, and chunk planning | provider protocols or workflow sequencing        |
| `crates/core`        | canonical model and workflow orchestration          | concrete cloud SDKs or UI types                  |
| `crates/credentials` | secret types and OS-native credential storage       | provider protocols or business workflow          |
| `crates/providers`   | provider traits, mock adapters, and OpenAI adapters | UI state or workflow sequencing                  |
| `crates/storage`     | persistence implementations and migrations          | rendering or provider selection UI               |
| `crates/exporters`   | canonical artifact rendering                        | database access or analysis calls                |
| `services/local-ai`  | standalone local capability HTTP service            | desktop process internals or cloud keys          |
| `packages/ui`        | accessible primitives and design tokens             | business workflow and secrets                    |
| `slack`              | transport contracts, DTOs, manifest, fixtures       | meeting analysis logic                           |

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

Every provider call receives a `RunContext` carrying a progress sink and a
cooperative cancellation token. Providers report the stages only they can know
(audio preparation, upload, transcription); the core reports analysis, saving,
completion, and failure. No stage carries a percentage, because nothing in the
pipeline can measure one honestly.

## Audio ingestion

`crates/audio` turns an imported recording into uploadable audio:

```text
original recording (never modified)
  -> validate container and bounds
  -> ffprobe for duration and stream facts
  -> FFmpeg normalize to mono, speech-oriented mp3 in a temp workspace
  -> fits one upload?  yes -> single request
                        no -> silencedetect -> silence-aware chunk plan -> chunks
```

The temporary workspace is removed when the prepared audio is dropped. Chunk
plans cover the recording exactly once, in order, with no gaps, and prefer the
midpoint of a detected pause so a cut does not land inside a sentence. A cut
that had no usable pause is reported rather than hidden.

Chunking costs speaker continuity: each chunk is diarized independently, so a
chunked transcript is marked `SpeakerScope::Chunk` and its speaker IDs stay
scoped to their chunk. Nothing merges two chunk-local speakers into one person
without evidence; resolving them is an operator decision.

## Credentials

`crates/credentials` owns the secret type and the OS-native store. A
`SecretString` has no `Serialize`, its `Debug` and `Display` are redacted, and
its buffer is zeroed on drop, so a credential cannot reach settings JSON, a
SQLite payload, a tracing span, or an interface error. The OS-native store is
feature-gated so mock mode and CI never link a platform credential manager.

## Deployment modes

### Desktop / BYOK

```text
React UI
  -> allow-listed Tauri command
  -> application services (`crates/app`)
  -> Meeting Core
  -> selected provider
  -> OS-native secure credential storage (credential path only)
```

The webview receives capability and result DTOs, never raw credentials. No key is
stored in React state persistence, `localStorage`, or a JSON settings file. React
submits a pasted key once and drops it from component state as soon as the native
layer confirms it was stored.

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
