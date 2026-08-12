# Privacy

Meeting data can contain voices, biometric-like speaker features, names,
confidential discussion, decisions, and action ownership. The product therefore
defaults to data minimization and explicit provider choice.

## Data classes

- Audio and recordings
- Speaker labels, profiles, and future voiceprints
- Transcripts and evidence links
- Meeting metadata, summaries, decisions, actions, and open questions
- Exports and local database records
- Provider and Slack credentials

## Development rules

- Repository fixtures are synthetic and contain no real meeting content.
- Recordings, audio, private fixtures, databases, models, and credentials are
  ignored by Git and must not be force-added.
- CI operates entirely on deterministic text fixtures.
- No telemetry or analytics is part of bootstrap.
- Mock mode performs no AI or Slack network request.

## Planned product controls

Before handling real meetings, each interface must clearly disclose the selected
processing mode and data destination. Users need explicit controls for capture,
retention, export, deletion, provider selection, speaker-profile enrollment, and
profile removal. Recording and consent requirements differ by jurisdiction and
deployment; operators remain responsible for obtaining appropriate consent.

Local mode should keep content on the user's selected host except for deliberate
interface traffic. BYOK mode sends only the minimum required content to the
chosen provider. Self-hosted mode keeps provider credentials on the server.

## Logs and support

Logs use opaque IDs and bounded metadata, not raw meeting content or credentials.
Support and security reports should include the smallest synthetic reproduction
possible. Do not upload private recordings or transcripts to public issues.
