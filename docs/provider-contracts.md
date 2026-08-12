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

## `TranscriptionProvider`

Input:

- an audio source reference (not necessarily a local path)
- zero or more language hints
- optional known context

Output: a canonical, speaker-aware `Transcript`. The provider must preserve
ordered millisecond timing and stable segment IDs. A network implementation must
be explicitly selected and disabled in offline mock mode.

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
IDs present in the input transcript. Providers must not invent or silently
discard evidence references.

## `StorageProvider`

The storage contract owns meeting, job, transcript, speaker profile, analysis,
and artifact persistence. It exposes domain operations rather than SQL. UI code
and provider code cannot depend on SQLite types, schema details, or connections.

SQLite is the first local implementation foundation. Schema migrations must be
explicit, idempotently tracked, and tested before real meeting data is stored.

## Exporters

Exporters consume canonical data and return an `Artifact`; they are not AI
providers and do not call storage directly. Bootstrap includes deterministic
Markdown/JSON behavior. Word export is planned behind the same artifact model.

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
