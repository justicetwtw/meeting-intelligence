import type { SlackEventDto, SlackOutboundMessageDto } from "./events.js";

export type SlackEventHandler = (event: SlackEventDto) => Promise<void>;

/** Server-side boundary between Slack delivery and Meeting Core. */
export interface SlackTransport {
  readonly kind: "socket-mode" | "http-events";
  readonly implementationStatus: "not-implemented" | "future";

  start(handler: SlackEventHandler): Promise<void>;
  stop(): Promise<void>;
  postMessage(message: SlackOutboundMessageDto): Promise<void>;
}

export class SlackTransportNotImplementedError extends Error {
  constructor(transportName: string) {
    super(`${transportName} is a contract-only bootstrap skeleton`);
    this.name = "SlackTransportNotImplementedError";
  }
}

/**
 * Placeholder for the planned self-hosted transport.
 *
 * It intentionally has no Slack SDK dependency, credential loading, websocket
 * connection, event acknowledgement, or message-sending behavior. Every
 * operation rejects so callers cannot mistake the bootstrap for an integration.
 */
export class SocketModeTransport implements SlackTransport {
  readonly kind = "socket-mode" as const;
  readonly implementationStatus = "not-implemented" as const;

  start(_handler: SlackEventHandler): Promise<void> {
    return this.notImplemented();
  }

  stop(): Promise<void> {
    return this.notImplemented();
  }

  postMessage(_message: SlackOutboundMessageDto): Promise<void> {
    return this.notImplemented();
  }

  private notImplemented(): Promise<never> {
    return Promise.reject(new SlackTransportNotImplementedError("SocketModeTransport"));
  }
}

/**
 * Reserved contract marker for a future signed HTTP Events implementation.
 * No listener or request handler exists in this bootstrap.
 */
export interface HttpEventsTransport extends SlackTransport {
  readonly kind: "http-events";
  readonly implementationStatus: "future";
}

export const HTTP_EVENTS_TRANSPORT_STATUS = {
  name: "HttpEventsTransport",
  status: "future",
} as const;
