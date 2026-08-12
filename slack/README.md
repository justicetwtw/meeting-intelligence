# Slack foundation

This directory defines a transport boundary and normalized event DTO for a
future server-side Slack integration. It does **not** connect to Slack.

## Bootstrap contents

- `manifest.example.yml` is an example Socket Mode manifest. It does not create
  or install an app and contains no workspace IDs, URLs, or credentials.
- `src/events.ts` contains the normalized inbound and outbound DTOs.
- `src/transport.ts` defines `SlackTransport` and the planned transports.
- `fixtures/mock-event.json` is synthetic test data and contains no real user or
  workspace content.

`SocketModeTransport` is intentionally not implemented: `start`, `stop`, and
`postMessage` always reject with `SlackTransportNotImplementedError`. There is
no Slack SDK and no network connection in this bootstrap. `HttpEventsTransport`
is a type-only future marker; there is no HTTP listener.

## Credential boundary

When a later milestone implements a transport, Slack app tokens, bot tokens,
and signing secrets must be injected only into the server-side process through
a secret manager or runtime environment. They must never be committed, placed
in the manifest, exposed to desktop/web client bundles, included in DTOs, or
written to logs.

An HTTP Events implementation must verify Slack signatures and timestamps
before parsing a request, reject replays, acknowledge within Slack's deadline,
and deduplicate event IDs. A Socket Mode implementation must likewise
acknowledge envelopes and deduplicate retries. These are future requirements,
not claimed capabilities.

The example requests only `app_mentions:read` and `chat:write`. Any additional
scope must be justified when the corresponding product behavior is implemented.
