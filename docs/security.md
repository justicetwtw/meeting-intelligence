# Security architecture

This document explains implementation boundaries. See the root
[`SECURITY.md`](../SECURITY.md) for vulnerability reporting and scanner policy.

## Credential flow

Desktop credentials follow one path:

```text
UI -> allow-listed native command -> OS-native secure storage -> provider
```

React code may request that the native backend store, test, or remove a
credential. It must not receive the raw value back, persist it in state,
`localStorage`, IndexedDB, logs, crash reports, or settings JSON.

Self-hosted credentials follow:

```text
Slack/Desktop -> authenticated Meeting Server -> provider credential
```

The credential remains on the server. A client receives provider capability and
status information, never the secret.

## Logging and errors

Do not log provider keys, Slack tokens, `Authorization` headers, cookies, raw
credentials, full sensitive requests, recordings, or complete transcripts.
Structured events should use operation names, stable opaque IDs, duration,
bounded sizes, result categories, and redacted provider errors.

Error messages crossing into an interface must not echo request bodies or
provider response bodies. Development debug output follows the same rule.

## Input and network controls

- Treat audio metadata, filenames, transcript text, Slack events, REST payloads,
  provider output, export names, and local-AI responses as untrusted.
- Bound payload sizes, segment counts, text length, timeouts, and retries before
  production transports are enabled.
- Validate evidence IDs against the input transcript.
- Make network use explicit by provider configuration; mock mode remains offline.
- Bind the local-AI development service to loopback by default. Exposing it to a
  network requires authentication and deployment hardening.

## Dependency and release controls

Dependabot monitors supported ecosystems. Pull-request CI uses no repository
secrets. The tag release foundation grants only `contents: write` and produces
unsigned bootstrap artifacts. Windows code signing, updater signing, provenance,
and a release approval process are required before production distribution.

## Future security gates

Before production capability is claimed, add threat-modelled authentication and
authorization, secure credential storage integration, database migration and
isolation tests, content-retention controls, transport replay validation,
security review for network providers, release signing, and incident-response
documentation.
