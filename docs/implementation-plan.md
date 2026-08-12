# Implementation plan

Each milestone preserves mock mode and lands with deterministic tests. A
milestone cannot claim completion from interface screenshots alone.

## 0. Repository bootstrap (current)

- Canonical data model and provider traits
- Mock providers, synthetic fixtures, and offline pipeline
- Minimal Tauri/React, server, local-AI, and Slack foundations
- CI, dependency updates, security policy, and Windows release workflow
- Architecture, product, privacy, development, and agent documentation

Exit gate: clean-clone build and tests for Rust, TypeScript, and Python; offline
pipeline; hosted CI.

## 1. Local meeting core

- Explicit job state machine and cancellation
- Versioned SQLite schema and migrations behind `StorageProvider`
- Transcript correction and evidence-link integrity
- Markdown/JSON production contracts and Word exporter foundation

Exit gate: migration, restart, failure recovery, evidence, and export round-trip
tests with synthetic content.

## 2. Desktop native boundary

- Tauri commands for canonical workflows
- OS-native credential storage adapter
- File/audio selection with bounded validation
- Accessible workflow UI using shared design tokens

Exit gate: command allow-list, credential non-disclosure, mock workflow, error,
keyboard, light/dark, and persistence tests.

## 3. Local AI adapters

- Versioned transcription and speaker-identity HTTP contracts
- Explicit model lifecycle and capability reporting
- Timeouts, cancellation, payload bounds, and unknown-speaker fallback

Exit gate: cross-language contract tests and CPU-compatible synthetic fixtures.
Large models and GPU setup remain opt-in and outside default CI.

## 4. BYOK network providers

- Native/server-held credential flows
- OpenAI transcription and intelligence adapters
- Redaction, rate-limit, retry, timeout, and cost-aware controls

Exit gate: mock-server contract tests, explicit network-mode consent, redaction
tests, and no credential exposure to UI or logs. CI never calls paid APIs.

## 5. Self-hosted server and Slack

- Authenticated REST API and tenant/data boundaries
- Socket Mode transport using shared application services
- HTTP Events adapter after verification and replay design

Exit gate: authorization, replay, idempotency, transport equivalence, isolation,
and end-to-end mock tests.

## 6. Production readiness

- Retention/deletion and speaker-profile lifecycle controls
- Threat model, security review, operational recovery, and privacy UX
- Windows signing, updater signing, artifact provenance, and release approvals
- Support/versioning policy and stable migration guarantees

Exit gate: signed release rehearsal, clean upgrade/rollback evidence, security and
privacy acceptance, and documented operator runbooks.
