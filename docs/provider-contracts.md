# Provider contracts

Provider contracts use canonical, serializable DTOs and asynchronous Rust traits.
Implementations must not mutate UI state or rely on a particular interface.

## Canonical types

- `Meeting`: stable identity, metadata, processing state, and artifact links.
- `ProcessingJob`: requested operation, lifecycle state, timestamps, and error
  category without sensitive payloads.
- `Transcript`: ordered canonical segments and language metadata.
- `TranscriptSegment`: stable ID, speaker ID, `start_ms`, `end_ms`, and text.
- `Speaker`: meeting-local speaker identity.
- `SpeakerProfile`: persistent, explicitly enrolled identity metadata.
- `MeetingAnalysis`: summary, decisions, action items, and open questions.
- `Decision` and `ActionItem`: structured result plus `evidence_segment_ids`.
- `OpenQuestion`: unresolved question plus optional evidence segment IDs.
- `Artifact`: typed export metadata and content reference.

IDs remain stable across analysis and export. Providers return canonical data or
typed errors; they do not return framework-specific response objects.

### Additive fields

The following optional fields exist alongside the original bootstrap shape. Each
has a serde default, so a payload written by an earlier build still loads.

- `Transcript.speaker_scope`: `meeting` (default) or `chunk`. `chunk` means the
  transcript was assembled from independently diarized uploads, so a speaker ID
  is only meaningful inside its own chunk.
- `Speaker.manual_display_name`: an operator-supplied label. It never replaces
  `transcript_speaker_id` and never implies a resolved identity.
- `Meeting.duration_ms`: total recording length once known.
- `Artifact.encoding`: `utf8` (default) or `base64`, so binary artifacts such as
  Word documents fit the existing artifact shape.
- `IntelligenceRequest.analysis_mode`: `fast`, `standard` (default), or `deep`.

`ProcessingJobErrorCategory` gained `credential`, `quota`, `network`, `audio`,
`configuration`, and `canceled`. Categories remain redacted: they never carry a
provider message, request body, or header.

## `RunContext`

Every provider call receives a `RunContext` with a progress sink and a
cooperative cancellation token. It carries orchestration only, so providers stay
stateless and reusable across runs. Progress events carry stable IDs, a stage,
and bounded part counts — never file names, transcript text, or payloads.

## `TranscriptionProvider`

Input:

- an audio source reference (not necessarily a local path)
- zero or more language hints
- optional known context
- a `RunContext`

Output: a canonical, speaker-aware `Transcript`. The provider must preserve
ordered millisecond timing and stable segment IDs. A network implementation must
be explicitly selected and disabled in offline mock mode.

A provider that splits a recording is responsible for producing one coherent
transcript: whole-meeting timestamps, globally unique and ordered segment IDs,
and speaker IDs whose scope is stated in `Transcript.speaker_scope`. Diarized
output may overlap; the canonical transcript is a single ordered timeline, so a
provider resolves overlaps deterministically rather than discarding speech.

## `SpeakerIdentityProvider`

Input:

- speaker segments or derived, bounded features
- enrolled `SpeakerProfile` records

Output: identity candidates with calibrated confidence, or `unknown`. A caller
must define an acceptance threshold. Results below it remain unknown; neither
provider nor core may select the highest candidate merely because one exists.

## `IntelligenceProvider`

Input:

- canonical `Transcript`
- meeting metadata

Output: structured `MeetingAnalysis`. Evidence fields may contain only segment
IDs present in the input transcript. Providers must not invent evidence
references; an ID that is not in the transcript is dropped and counted, never
accepted. An action item has no assignee and no due date unless the transcript
states one — neither may be inferred from who was speaking.

A network implementation must request a strict structured output rather than
parsing free text, and must map its own reasoning settings from the product's
three analysis modes so no interface encodes a vendor parameter.

## `StorageProvider`

The storage contract owns meeting, job, transcript, speaker profile, analysis,
and artifact persistence, plus ordered meeting history. It exposes domain
operations rather than SQL. UI code and provider code cannot depend on SQLite
types, schema details, or connections.

SQLite is the first local implementation. Migrations are an append-only list
applied in order inside a transaction, tracked by `PRAGMA user_version`, and
tested for a fresh database, a repeat run, an upgrade from an existing database,
and a refusal to open a database newer than the build.

## Exporters

Exporters consume canonical data and return an `Artifact`; they are not AI
providers and do not call storage directly. Markdown, JSON, and Word exports are
deterministic: two exports of the same meeting are byte-identical. The Word
export is a real WordprocessingML package, not renamed HTML, delivered as a
Base64 artifact.

## OpenAI adapters

The OpenAI adapters live behind a crate feature. Nothing in mock mode or the
offline pipeline enables it, so a credential-free build cannot make a paid call.
Every request passes through an injectable transport, which is what allows the
adapters to be tested end to end against recorded responses with no socket, no
credential, and no cost. Failures map onto typed provider errors by status; the
upstream response body is never placed into an error message.

## Local-AI HTTP boundary

The local service begins with:

- `GET /health`: process readiness without model download or initialization.
- `GET /capabilities`: versioned, structured capability flags.

Future transcription or identity endpoints require a versioned schema, bounded
payloads, timeouts, cancellation, redacted errors, and contract tests in both
Python and Rust before implementation.

## Compatibility

Contract changes that remove fields, change ID/timing semantics, weaken unknown
fallback, or invalidate evidence links are breaking changes. Additive optional
fields still require fixtures and round-trip tests.
