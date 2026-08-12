/** Normalized inbound Slack event passed from a transport to Meeting Core. */
export interface SlackEventDto {
  readonly schemaVersion: "1";
  readonly source: "slack";
  readonly eventId: string;
  readonly teamId: string;
  readonly eventType: "app_mention" | "message";
  readonly channelId: string;
  readonly userId: string | null;
  readonly text: string;
  readonly eventTs: string;
  readonly receivedAt: string;
  readonly threadTs: string | null;
}

/** Minimal outbound contract. Transports must never log message content or credentials. */
export interface SlackOutboundMessageDto {
  readonly channelId: string;
  readonly text: string;
  readonly threadTs?: string;
}
