import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { App } from "./App";
import { BridgeContext } from "./native/bridge-context";
import { Onboarding } from "./screens/Onboarding";
import {
  createFakeBridge,
  detail,
  status,
  summary,
  type FakeBridge,
} from "./test/fake-bridge";

async function renderApp(bridge: FakeBridge) {
  render(<App bridge={bridge} />);
  await screen.findByText("Meeting Intelligence");
  return bridge;
}

describe("onboarding", () => {
  it("asks for a key before anything else when OpenAI is selected and none is stored", async () => {
    const bridge = createFakeBridge({
      appStatus: vi.fn(() =>
        Promise.resolve(
          status({
            onboardingRequired: true,
            credential: { configured: false, hint: null },
          }),
        ),
      ),
    });

    render(<App bridge={bridge} />);

    expect(
      await screen.findByRole("heading", { name: "Connect OpenAI" }),
    ).toBeInTheDocument();
    expect(screen.getByLabelText("OpenAI API key")).toHaveAttribute("type", "password");
  });

  it("tests the connection before the key is stored", async () => {
    const user = userEvent.setup();
    const bridge = createFakeBridge({
      appStatus: vi.fn(() =>
        Promise.resolve(
          status({
            onboardingRequired: true,
            credential: { configured: false, hint: null },
          }),
        ),
      ),
    });
    render(<App bridge={bridge} />);
    await screen.findByRole("heading", { name: "Connect OpenAI" });

    await user.type(
      screen.getByLabelText("OpenAI API key"),
      "sk-test-0123456789abcdef",
    );
    await user.click(screen.getByRole("button", { name: "Test connection" }));

    expect(await screen.findByText("Connection succeeded.")).toBeInTheDocument();
    expect(bridge.testOpenAiConnection).toHaveBeenCalledWith(
      "sk-test-0123456789abcdef",
    );
    expect(bridge.saveOpenAiKey).not.toHaveBeenCalled();
  });

  it("reports a rejected key without exposing the provider response", async () => {
    const user = userEvent.setup();
    const bridge = createFakeBridge({
      appStatus: vi.fn(() =>
        Promise.resolve(
          status({
            onboardingRequired: true,
            credential: { configured: false, hint: null },
          }),
        ),
      ),
      testOpenAiConnection: vi.fn(() =>
        Promise.reject({
          category: "credential",
          message: "The OpenAI API key was rejected. Open Settings and replace it.",
          retryable: false,
        }),
      ),
    });
    render(<App bridge={bridge} />);
    await screen.findByRole("heading", { name: "Connect OpenAI" });

    await user.type(
      screen.getByLabelText("OpenAI API key"),
      "sk-test-0123456789abcdef",
    );
    await user.click(screen.getByRole("button", { name: "Test connection" }));

    const alert = await screen.findByRole("alert");
    expect(within(alert).getByText("API key problem")).toBeInTheDocument();
    expect(alert.textContent).not.toContain("sk-test");
  });

  it("drops the typed key from the form as soon as the backend has it", async () => {
    const user = userEvent.setup();
    const bridge = createFakeBridge();
    render(
      <BridgeContext.Provider value={bridge}>
        <Onboarding onConnected={() => undefined} />
      </BridgeContext.Provider>,
    );
    const field = screen.getByLabelText("OpenAI API key");

    await user.type(field, "sk-test-0123456789abcdef");
    await user.click(screen.getByRole("button", { name: "Continue" }));

    await waitFor(() => {
      expect(bridge.saveOpenAiKey).toHaveBeenCalledWith("sk-test-0123456789abcdef");
    });
    await waitFor(() => {
      expect(field).toHaveValue("");
    });
  });

  it("moves past onboarding once a key is stored", async () => {
    const user = userEvent.setup();
    const bridge = createFakeBridge({
      appStatus: vi
        .fn()
        .mockResolvedValueOnce(
          status({
            onboardingRequired: true,
            credential: { configured: false, hint: null },
          }),
        )
        .mockResolvedValue(status()),
    });
    render(<App bridge={bridge} />);
    await screen.findByRole("heading", { name: "Connect OpenAI" });

    await user.type(
      screen.getByLabelText("OpenAI API key"),
      "sk-test-0123456789abcdef",
    );
    await user.click(screen.getByRole("button", { name: "Continue" }));

    expect(await screen.findByText("Import a recording")).toBeInTheDocument();
    expect(screen.queryByLabelText("OpenAI API key")).not.toBeInTheDocument();
  });
});

describe("meeting library", () => {
  it("lists recent meetings with their state and duration", async () => {
    const bridge = createFakeBridge({
      listMeetings: vi.fn(() =>
        Promise.resolve([
          summary(),
          summary({
            id: "meeting_2",
            title: "Sales Meeting",
            startedAt: "2026-08-11T02:00:00Z",
            durationLabel: "00:48:21",
          }),
        ]),
      ),
    });
    await renderApp(bridge);

    const list = await screen.findByRole("list");
    const rows = within(list).getAllByRole("button");
    expect(rows[0]).toHaveTextContent("BCRC Meeting");
    expect(rows[0]).toHaveTextContent("2026/08/12");
    expect(rows[0]).toHaveTextContent("01:27:34");
    expect(rows[0]).toHaveTextContent("Completed");
    expect(rows[1]).toHaveTextContent("Sales Meeting");
    expect(rows[1]).toHaveTextContent("00:48:21");
  });

  it("shows an empty state before the first import", async () => {
    const bridge = createFakeBridge({ listMeetings: vi.fn(() => Promise.resolve([])) });
    await renderApp(bridge);

    expect(await screen.findByText("No meetings yet")).toBeInTheDocument();
  });

  it("imports the file chosen from the picker", async () => {
    const user = userEvent.setup();
    const bridge = await renderApp(createFakeBridge());

    await user.click(await screen.findByRole("button", { name: "Choose file" }));

    await waitFor(() => {
      expect(bridge.importMeeting).toHaveBeenCalledWith(
        expect.objectContaining({ path: "C:/recordings/bcrc.m4a" }),
      );
    });
  });

  it("imports a file dropped onto the window", async () => {
    const bridge = await renderApp(createFakeBridge());
    await screen.findByTestId("drop-zone");

    bridge.dropFiles(["C:/recordings/dropped.wav"]);

    await waitFor(() => {
      expect(bridge.importMeeting).toHaveBeenCalledWith(
        expect.objectContaining({ path: "C:/recordings/dropped.wav" }),
      );
    });
  });
});

describe("processing", () => {
  it("shows the current stage without inventing a percentage", async () => {
    const user = userEvent.setup();
    let resolveImport: ((value: ReturnType<typeof detail>) => void) | undefined;
    const bridge = createFakeBridge({
      importMeeting: vi.fn(
        () =>
          new Promise<ReturnType<typeof detail>>((resolve) => {
            resolveImport = resolve;
          }),
      ),
    });
    await renderApp(bridge);

    await user.click(await screen.findByRole("button", { name: "Choose file" }));
    await screen.findByRole("heading", { name: "Processing meeting" });

    bridge.emitProgress({
      runId: "run-1",
      meetingId: "meeting_1",
      stage: "uploading",
      currentPart: 2,
      totalParts: 4,
      errorCategory: null,
    });

    expect(await screen.findByText("part 2 of 4")).toBeInTheDocument();
    expect(screen.queryByText(/%/u)).not.toBeInTheDocument();

    resolveImport?.(detail());
    await screen.findByRole("heading", { name: "BCRC Meeting" });
  });

  it("offers a cancel action while a run is in flight", async () => {
    const user = userEvent.setup();
    const bridge = createFakeBridge({
      importMeeting: vi.fn(
        () => new Promise<ReturnType<typeof detail>>(() => undefined),
      ),
    });
    await renderApp(bridge);

    await user.click(await screen.findByRole("button", { name: "Choose file" }));
    await user.click(await screen.findByRole("button", { name: "Cancel" }));

    expect(bridge.cancelProcessing).toHaveBeenCalled();
  });

  it("shows a retryable failure with a human-readable category", async () => {
    const user = userEvent.setup();
    const bridge = createFakeBridge({
      importMeeting: vi.fn(() =>
        Promise.reject({
          category: "network",
          message:
            "OpenAI could not be reached. Check the network connection and retry.",
          retryable: true,
        }),
      ),
    });
    await renderApp(bridge);

    await user.click(await screen.findByRole("button", { name: "Choose file" }));

    const alert = await screen.findByRole("alert");
    expect(within(alert).getByText("Network problem")).toBeInTheDocument();
    expect(within(alert).getByRole("button", { name: "Retry" })).toBeInTheDocument();
  });
});

describe("meeting detail", () => {
  async function openMeeting() {
    const user = userEvent.setup();
    const bridge = await renderApp(createFakeBridge());
    await user.click(await screen.findByRole("button", { name: /BCRC Meeting/u }));
    await screen.findByRole("heading", { name: "BCRC Meeting" });
    return { user, bridge };
  }

  it("shows the analysis with its evidence references", async () => {
    await openMeeting();

    expect(screen.getByText("The team agreed to ship on Friday.")).toBeInTheDocument();
    expect(screen.getByText("Ship on Friday.")).toBeInTheDocument();
    expect(screen.getByText("seg_00003")).toBeInTheDocument();
    expect(
      screen.getByText("Confirm next week's price with procurement."),
    ).toBeInTheDocument();
    expect(screen.getByText("None recorded.")).toBeInTheDocument();
  });

  it("renders the transcript with timestamps and speaker labels", async () => {
    const { user } = await openMeeting();

    await user.click(screen.getByRole("tab", { name: "Transcript" }));

    expect(screen.getByText("00:14:25")).toBeInTheDocument();
    expect(
      screen.getByText("Please ask procurement to confirm next week's price."),
    ).toBeInTheDocument();
  });

  it("applies a speaker rename across the whole transcript", async () => {
    const { user, bridge } = await openMeeting();
    await user.click(screen.getByRole("tab", { name: "Transcript" }));

    await user.click(screen.getByRole("button", { name: /Speaker 2/u }));
    await user.type(screen.getByLabelText("Name for Speaker 2"), "Mr. Wang");
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      expect(bridge.renameSpeaker).toHaveBeenCalledWith({
        meetingId: "meeting_1",
        speakerId: "speaker_02",
        displayName: "Mr. Wang",
      });
    });
    expect(await screen.findAllByText("Mr. Wang")).not.toHaveLength(0);
    expect(screen.queryByText("Speaker 2:")).not.toBeInTheDocument();
  });

  it("filters the transcript by a search term", async () => {
    const { user } = await openMeeting();
    await user.click(screen.getByRole("tab", { name: "Transcript" }));

    await user.type(screen.getByLabelText("Search transcript"), "procurement");

    expect(
      screen.getByText("Please ask procurement to confirm next week's price."),
    ).toBeInTheDocument();
    expect(
      screen.queryByText("Let us keep the pipeline deterministic."),
    ).not.toBeInTheDocument();
  });

  it("exports in each supported format", async () => {
    const { user, bridge } = await openMeeting();

    await user.selectOptions(screen.getByLabelText("Export as"), "docx");
    await user.click(screen.getByRole("button", { name: "Export" }));

    await waitFor(() => {
      expect(bridge.exportMeeting).toHaveBeenCalledWith({
        meetingId: "meeting_1",
        format: "docx",
        directory: "C:/exports",
      });
    });
    expect(
      await screen.findByText("Exported to C:/exports/meeting.docx"),
    ).toBeInTheDocument();
  });

  it("offers a retry for a meeting that failed to process", async () => {
    const user = userEvent.setup();
    const failed = detail({
      meeting: summary({ processingState: "failed", errorCategory: "network" }),
      analysis: null,
    });
    const bridge = createFakeBridge({
      listMeetings: vi.fn(() =>
        Promise.resolve([
          summary({ processingState: "failed", errorCategory: "network" }),
        ]),
      ),
      getMeeting: vi.fn(() => Promise.resolve(failed)),
    });
    await renderApp(bridge);

    await user.click(await screen.findByRole("button", { name: /BCRC Meeting/u }));
    await user.click(await screen.findByRole("button", { name: "Retry processing" }));

    await waitFor(() => {
      expect(bridge.retryMeeting).toHaveBeenCalledWith(
        expect.objectContaining({ meetingId: "meeting_1" }),
      );
    });
  });

  it("warns when speaker labels only apply within one part of the recording", async () => {
    const user = userEvent.setup();
    const bridge = createFakeBridge({
      getMeeting: vi.fn(() =>
        Promise.resolve(
          detail({
            speakerScope: "chunk",
            speakerScopeNote:
              "This recording was transcribed in several parts. Speaker labels are reliable within one part only.",
          }),
        ),
      ),
    });
    await renderApp(bridge);

    await user.click(await screen.findByRole("button", { name: /BCRC Meeting/u }));

    expect(
      await screen.findByText(/reliable within one part only/u),
    ).toBeInTheDocument();
  });
});

describe("settings", () => {
  it("never shows a stored key, only a hint", async () => {
    const user = userEvent.setup();
    await renderApp(createFakeBridge());

    await user.click(screen.getByRole("button", { name: "Settings" }));

    expect(
      await screen.findByText(/A key is configured \(…cdef\)/u),
    ).toBeInTheDocument();
    expect(screen.getByLabelText("Replace API key")).toHaveValue("");
  });

  it("keeps transcription and intelligence providers independent", async () => {
    const user = userEvent.setup();
    const bridge = await renderApp(createFakeBridge());

    await user.click(screen.getByRole("button", { name: "Settings" }));
    await user.selectOptions(
      await screen.findByLabelText("Transcription provider"),
      "mock",
    );

    await waitFor(() => {
      expect(bridge.updateSettings).toHaveBeenCalledWith(
        expect.objectContaining({
          transcriptionProvider: "mock",
          intelligenceProvider: "openai",
        }),
      );
    });
  });

  it("exposes the three analysis modes and no vendor reasoning parameter", async () => {
    const user = userEvent.setup();
    const bridge = await renderApp(createFakeBridge());

    await user.click(screen.getByRole("button", { name: "Settings" }));
    const mode = await screen.findByLabelText("Analysis mode");

    expect(
      Array.from(mode.querySelectorAll("option")).map((option) => option.textContent),
    ).toEqual(["Fast", "Standard", "Deep"]);
    expect(screen.queryByText(/effort/iu)).not.toBeInTheDocument();

    await user.selectOptions(mode, "deep");
    await waitFor(() => {
      expect(bridge.updateSettings).toHaveBeenCalledWith(
        expect.objectContaining({ analysisMode: "deep" }),
      );
    });
  });

  it("can remove the stored key", async () => {
    const user = userEvent.setup();
    const bridge = await renderApp(createFakeBridge());

    await user.click(screen.getByRole("button", { name: "Settings" }));
    await user.click(await screen.findByRole("button", { name: "Remove key" }));

    await waitFor(() => {
      expect(bridge.removeOpenAiKey).toHaveBeenCalled();
    });
    expect(await screen.findByText("Key removed.")).toBeInTheDocument();
  });

  it("states the cloud/local data boundary", async () => {
    const user = userEvent.setup();
    await renderApp(createFakeBridge());

    await user.click(screen.getByRole("button", { name: "Settings" }));

    expect(
      await screen.findByText(
        /meeting history, transcripts, and exports stay on this device/iu,
      ),
    ).toBeInTheDocument();
  });
});

describe("appearance", () => {
  it("supports a user-controlled dark theme", async () => {
    const user = userEvent.setup();
    const bridge = await renderApp(createFakeBridge());

    await user.click(screen.getByRole("button", { name: "Switch to dark theme" }));

    await waitFor(() => {
      expect(document.documentElement).toHaveClass("dark");
    });
    expect(bridge.updateSettings).toHaveBeenCalledWith(
      expect.objectContaining({ theme: "dark" }),
    );
  });
});
