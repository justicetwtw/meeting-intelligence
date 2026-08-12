// @vitest-environment node

import { readFile } from "node:fs/promises";

import { describe, expect, it, vi } from "vitest";

import type { SlackEventDto } from "../src/events.js";
import {
  HTTP_EVENTS_TRANSPORT_STATUS,
  SlackTransportNotImplementedError,
  SocketModeTransport,
} from "../src/transport.js";

describe("Slack transport foundation", () => {
  it("loads the synthetic event fixture into the canonical DTO", async () => {
    const fixtureUrl = new URL("../fixtures/mock-event.json", import.meta.url);
    const event = JSON.parse(await readFile(fixtureUrl, "utf8")) as SlackEventDto;

    expect(event.schemaVersion).toBe("1");
    expect(event.source).toBe("slack");
    expect(event.eventId).toBeTruthy();
    expect(event.text).toContain("mock");
  });

  it("fails closed instead of connecting Socket Mode", async () => {
    const transport = new SocketModeTransport();

    await expect(transport.start(vi.fn())).rejects.toBeInstanceOf(
      SlackTransportNotImplementedError,
    );
    await expect(
      transport.postMessage({ channelId: "channel_mock", text: "synthetic" }),
    ).rejects.toBeInstanceOf(SlackTransportNotImplementedError);
  });

  it("marks HTTP Events as future-only", () => {
    expect(HTTP_EVENTS_TRANSPORT_STATUS).toEqual({
      name: "HttpEventsTransport",
      status: "future",
    });
  });
});
