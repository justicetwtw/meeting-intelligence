# Security Policy

## Supported versions

Meeting Intelligence is in pre-release development and has no supported
production version yet. Security fixes are made on the default branch. Release
support policy will be defined before the first stable release.

## Reporting a vulnerability

Please report suspected vulnerabilities through GitHub's private vulnerability
reporting feature for this repository. Do not open a public issue or include
credentials, private meeting data, or unnecessary exploit data in a report.

Include the affected component, realistic attack path, impact, reproduction
steps, and a minimal proof of concept when safe. Maintainers will acknowledge a
report as soon as practical and coordinate remediation and disclosure with the
reporter.

## System and scope

This policy covers the desktop application, meeting core, REST server, provider
and storage crates, exporters, local-AI service, Slack integration foundation,
shared UI package, build and release workflows, and deployment configuration in
this repository.

Important assets include provider credentials, Slack tokens, recordings,
transcripts, speaker profiles and voiceprints, meeting analyses, exports, and
the integrity of release artifacts.

## Threat model and trust boundaries

Audio, transcript text, meeting metadata, filenames, provider responses, Slack
events, REST requests, imported artifacts, and local-AI responses are untrusted
inputs. Desktop web content is not trusted with provider credentials.

The required credential boundaries are:

```text
Desktop UI -> native command -> OS-native secret storage -> provider
Slack/Desktop -> Meeting Server -> server-held provider credentials
```

Future internet-facing transports must authenticate before sensitive reads or
mutations. Storage implementations sit behind the `StorageProvider` contract;
UI code must not access SQLite directly.

## Security invariants

- API keys and tokens never enter React persistent state, `localStorage`,
  repository configuration, fixtures, exports, or client-visible errors.
- Logs never contain OpenAI keys, Slack tokens, authorization headers, raw
  credentials, or complete sensitive requests.
- Provider and transport inputs are bounded and validated before use.
- Low-confidence speaker identity results remain unknown; the system must not
  force an identity guess.
- Meeting data is isolated by the applicable local user or authenticated
  self-hosted boundary before access or mutation.
- Network-backed providers remain replaceable and cannot silently bypass mock or
  local-only mode.
- Release workflows use least-privilege permissions and never require private
  signing material for ordinary pull-request CI.

## Reportable findings and severity context

Report realistic ways to expose or exfiltrate credentials or meeting data,
bypass an authorization boundary, cross meeting or user isolation boundaries,
execute attacker-controlled code, tamper with trusted release artifacts, leak
sensitive data through logs or exports, or make a local-only workflow contact a
remote service without informed configuration.

Severity depends on reachability, required privileges, data sensitivity,
cross-user or remote impact, and whether the vulnerable path exists in a
released or deployable configuration. A passing test or mock-only code path is
not proof that a reported control is safe.

## Known limitations

The repository currently contains bootstrap contracts and mock implementations,
not a production-ready security boundary. OS-native credential storage,
production authentication and authorization, release signing, updater signing,
and hardened network-provider implementations are required before their related
features can be represented as production-ready.
