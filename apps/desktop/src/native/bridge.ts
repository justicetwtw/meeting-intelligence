import type {
  AnalysisMode,
  AppStatusDto,
  CredentialStatusDto,
  ExportFormat,
  MeetingDetailDto,
  MeetingSummaryDto,
  ProgressPayload,
  SettingsDto,
} from "./types";

/**
 * The only way the interface reaches the native backend.
 *
 * React never imports a provider SDK, never sees a credential, and never
 * touches the local database: it calls these operations and renders DTOs.
 */
export interface NativeBridge {
  appStatus(): Promise<AppStatusDto>;
  updateSettings(settings: SettingsDto): Promise<SettingsDto>;
  saveOpenAiKey(key: string): Promise<CredentialStatusDto>;
  removeOpenAiKey(): Promise<CredentialStatusDto>;
  testOpenAiConnection(key?: string): Promise<void>;
  listMeetings(): Promise<MeetingSummaryDto[]>;
  getMeeting(meetingId: string): Promise<MeetingDetailDto>;
  importMeeting(input: {
    runId: string;
    path: string;
    title?: string;
    analysisMode?: AnalysisMode;
  }): Promise<MeetingDetailDto>;
  retryMeeting(input: { runId: string; meetingId: string }): Promise<MeetingDetailDto>;
  cancelProcessing(runId: string): Promise<boolean>;
  renameSpeaker(input: {
    meetingId: string;
    speakerId: string;
    displayName: string | null;
  }): Promise<MeetingDetailDto>;
  exportMeeting(input: {
    meetingId: string;
    format: ExportFormat;
    directory?: string;
  }): Promise<string>;
  /** Opens the OS file picker. Resolves to `null` when the user cancels. */
  pickAudioFile(extensions: string[]): Promise<string | null>;
  pickDirectory(): Promise<string | null>;
  onProgress(listener: (payload: ProgressPayload) => void): Promise<() => void>;
  /** Drag-and-drop over the window. Resolves to an unsubscribe function. */
  onFileDrop(listener: (event: FileDropEvent) => void): Promise<() => void>;
}

export interface FileDropEvent {
  hovering: boolean;
  paths: string[];
}

const PROGRESS_EVENT = "meeting://progress";

function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** Bridge backed by the real Tauri command boundary. */
export function createTauriBridge(): NativeBridge {
  const invoke = async <T>(
    command: string,
    args?: Record<string, unknown>,
  ): Promise<T> => {
    const { invoke: tauriInvoke } = await import("@tauri-apps/api/core");
    return tauriInvoke<T>(command, args);
  };

  return {
    appStatus: () => invoke<AppStatusDto>("app_status"),
    updateSettings: (settings) => invoke<SettingsDto>("update_settings", { settings }),
    saveOpenAiKey: (key) => invoke<CredentialStatusDto>("save_openai_key", { key }),
    removeOpenAiKey: () => invoke<CredentialStatusDto>("remove_openai_key"),
    testOpenAiConnection: (key) =>
      invoke<void>("test_openai_connection", { key: key ?? null }),
    listMeetings: () => invoke<MeetingSummaryDto[]>("list_meetings"),
    getMeeting: (meetingId) => invoke<MeetingDetailDto>("get_meeting", { meetingId }),
    importMeeting: ({ runId, path, title, analysisMode }) =>
      invoke<MeetingDetailDto>("import_meeting", {
        runId,
        path,
        title: title ?? null,
        analysisMode: analysisMode ?? null,
      }),
    retryMeeting: ({ runId, meetingId }) =>
      invoke<MeetingDetailDto>("retry_meeting", { runId, meetingId }),
    cancelProcessing: (runId) => invoke<boolean>("cancel_processing", { runId }),
    renameSpeaker: ({ meetingId, speakerId, displayName }) =>
      invoke<MeetingDetailDto>("rename_speaker", { meetingId, speakerId, displayName }),
    exportMeeting: ({ meetingId, format, directory }) =>
      invoke<string>("export_meeting", {
        meetingId,
        format,
        directory: directory ?? null,
      }),
    async pickAudioFile(extensions) {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({
        multiple: false,
        directory: false,
        filters: [{ name: "Audio and video", extensions }],
      });
      return typeof selected === "string" ? selected : null;
    },
    async pickDirectory() {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({ multiple: false, directory: true });
      return typeof selected === "string" ? selected : null;
    },
    async onProgress(listener) {
      const { listen } = await import("@tauri-apps/api/event");
      return listen<ProgressPayload>(PROGRESS_EVENT, (event) =>
        listener(event.payload),
      );
    },
    async onFileDrop(listener) {
      const { getCurrentWebview } = await import("@tauri-apps/api/webview");
      return getCurrentWebview().onDragDropEvent((event) => {
        const { type } = event.payload;
        if (type === "drop") {
          listener({ hovering: false, paths: event.payload.paths });
        } else if (type === "leave") {
          listener({ hovering: false, paths: [] });
        } else {
          listener({ hovering: true, paths: [] });
        }
      });
    },
  };
}

/**
 * Bridge used when the UI runs in a plain browser (`npm run dev` without
 * Tauri, and in tests). Every operation fails loudly rather than pretending to
 * work, so a browser session can never look like a working install.
 */
export function createUnavailableBridge(): NativeBridge {
  const unavailable = () =>
    Promise.reject({
      category: "configuration" as const,
      message:
        "The native backend is not running. Start the desktop app with `npm run tauri dev`.",
      retryable: false,
    });

  return {
    appStatus: unavailable,
    updateSettings: unavailable,
    saveOpenAiKey: unavailable,
    removeOpenAiKey: unavailable,
    testOpenAiConnection: unavailable,
    listMeetings: unavailable,
    getMeeting: unavailable,
    importMeeting: unavailable,
    retryMeeting: unavailable,
    cancelProcessing: unavailable,
    renameSpeaker: unavailable,
    exportMeeting: unavailable,
    pickAudioFile: unavailable,
    pickDirectory: unavailable,
    onProgress: () => Promise.resolve(() => {}),
    onFileDrop: () => Promise.resolve(() => {}),
  };
}

export function resolveBridge(): NativeBridge {
  return isTauri() ? createTauriBridge() : createUnavailableBridge();
}
