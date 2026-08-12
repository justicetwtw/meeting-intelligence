import { vi } from "vitest";

import type { NativeBridge } from "../native/bridge";
import type {
  AppStatusDto,
  MeetingDetailDto,
  MeetingSummaryDto,
  ProgressPayload,
} from "../native/types";

export function status(overrides: Partial<AppStatusDto> = {}): AppStatusDto {
  return {
    onboardingRequired: false,
    credential: { configured: true, hint: "…cdef" },
    settings: {
      transcriptionProvider: "openai",
      intelligenceProvider: "openai",
      analysisMode: "standard",
      theme: "light",
      outputDirectory: null,
      ffmpegDirectory: null,
      languageHint: null,
    },
    supportedExtensions: ["mp3", "m4a", "wav", "mp4"],
    openaiAvailable: true,
    ...overrides,
  };
}

export function summary(overrides: Partial<MeetingSummaryDto> = {}): MeetingSummaryDto {
  return {
    id: "meeting_1",
    title: "BCRC Meeting",
    startedAt: "2026-08-12T09:00:00Z",
    processingState: "completed",
    durationMs: 5_254_000,
    durationLabel: "01:27:34",
    sourceFileName: "bcrc-2026-08-12.m4a",
    errorCategory: null,
    ...overrides,
  };
}

export function detail(overrides: Partial<MeetingDetailDto> = {}): MeetingDetailDto {
  return {
    meeting: summary(),
    speakers: [
      {
        id: "speaker_01",
        label: "Speaker 1",
        renamed: false,
        identityResolved: false,
        segmentCount: 2,
      },
      {
        id: "speaker_02",
        label: "Speaker 2",
        renamed: false,
        identityResolved: false,
        segmentCount: 1,
      },
    ],
    segments: [
      {
        id: "seg_00001",
        speakerId: "speaker_01",
        startMs: 0,
        endMs: 4_000,
        timestamp: "00:00:00",
        text: "Let us keep the pipeline deterministic.",
      },
      {
        id: "seg_00002",
        speakerId: "speaker_02",
        startMs: 865_000,
        endMs: 883_000,
        timestamp: "00:14:25",
        text: "Please ask procurement to confirm next week's price.",
      },
      {
        id: "seg_00003",
        speakerId: "speaker_01",
        startMs: 883_000,
        endMs: 890_000,
        timestamp: "00:14:43",
        text: "Agreed, we will ship on Friday.",
      },
    ],
    analysis: {
      summary: "The team agreed to ship on Friday.",
      decisions: [
        {
          id: "decision_001",
          text: "Ship on Friday.",
          assignee: null,
          dueAt: null,
          evidenceSegmentIds: ["seg_00003"],
        },
      ],
      actionItems: [
        {
          id: "action_001",
          text: "Confirm next week's price with procurement.",
          assignee: null,
          dueAt: null,
          evidenceSegmentIds: ["seg_00002"],
        },
      ],
      openQuestions: [],
    },
    speakerScope: "meeting",
    speakerScopeNote: null,
    ...overrides,
  };
}

export interface FakeBridge extends NativeBridge {
  emitProgress: (payload: ProgressPayload) => void;
  dropFiles: (paths: string[]) => void;
}

/**
 * In-memory stand-in for the native boundary.
 *
 * Tests drive the real components through it, so no test ever needs Tauri, a
 * credential, FFmpeg, or a network call.
 */
export function createFakeBridge(overrides: Partial<NativeBridge> = {}): FakeBridge {
  let progressListener: ((payload: ProgressPayload) => void) | null = null;
  let dropListener: ((event: { hovering: boolean; paths: string[] }) => void) | null =
    null;
  let current = detail();

  const base: NativeBridge = {
    appStatus: vi.fn(() => Promise.resolve(status())),
    updateSettings: vi.fn((settings) => Promise.resolve(settings)),
    saveOpenAiKey: vi.fn(() => Promise.resolve({ configured: true, hint: "…cdef" })),
    removeOpenAiKey: vi.fn(() => Promise.resolve({ configured: false, hint: null })),
    testOpenAiConnection: vi.fn(() => Promise.resolve()),
    listMeetings: vi.fn(() => Promise.resolve([summary()])),
    getMeeting: vi.fn(() => Promise.resolve(current)),
    importMeeting: vi.fn(() => Promise.resolve(current)),
    retryMeeting: vi.fn(() => Promise.resolve(current)),
    cancelProcessing: vi.fn(() => Promise.resolve(true)),
    renameSpeaker: vi.fn(({ speakerId, displayName }) => {
      current = {
        ...current,
        speakers: current.speakers.map((speaker) =>
          speaker.id === speakerId
            ? {
                ...speaker,
                label: displayName ?? `Speaker ${speaker.id.slice(-1)}`,
                renamed: displayName !== null,
              }
            : speaker,
        ),
      };
      return Promise.resolve(current);
    }),
    exportMeeting: vi.fn(({ format }) =>
      Promise.resolve(`C:/exports/meeting.${format}`),
    ),
    pickAudioFile: vi.fn(() => Promise.resolve("C:/recordings/bcrc.m4a")),
    pickDirectory: vi.fn(() => Promise.resolve("C:/exports")),
    onProgress: vi.fn((listener) => {
      progressListener = listener;
      return Promise.resolve(() => {
        progressListener = null;
      });
    }),
    onFileDrop: vi.fn((listener) => {
      dropListener = listener;
      return Promise.resolve(() => {
        dropListener = null;
      });
    }),
  };

  return {
    ...base,
    ...overrides,
    emitProgress: (payload) => progressListener?.(payload),
    dropFiles: (paths) => dropListener?.({ hovering: false, paths }),
  };
}
