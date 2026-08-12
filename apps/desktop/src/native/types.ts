/**
 * Canonical DTOs exactly as the native command boundary returns them.
 *
 * Nothing here can carry a credential: the native layer returns a
 * `configured` flag and a non-reversible hint, never the key itself.
 */

export type ProviderChoice = "mock" | "openai";
export type AnalysisMode = "fast" | "standard" | "deep";
export type ThemeChoice = "system" | "light" | "dark";
export type ProcessingState = "pending" | "processing" | "completed" | "failed";
export type SpeakerScope = "meeting" | "chunk";
export type ExportFormat = "markdown" | "json" | "docx";

export type ErrorCategory =
  | "provider"
  | "storage"
  | "invalid_provider_output"
  | "credential"
  | "quota"
  | "network"
  | "audio"
  | "configuration"
  | "canceled";

export type ProcessingStage =
  | "preparing_audio"
  | "uploading"
  | "transcribing"
  | "analyzing"
  | "saving"
  | "completed"
  | "failed";

export interface AppErrorDto {
  category: ErrorCategory;
  message: string;
  retryable: boolean;
}

export interface CredentialStatusDto {
  configured: boolean;
  hint: string | null;
}

export interface SettingsDto {
  transcriptionProvider: ProviderChoice;
  intelligenceProvider: ProviderChoice;
  analysisMode: AnalysisMode;
  theme: ThemeChoice;
  outputDirectory: string | null;
  ffmpegDirectory: string | null;
  languageHint: string | null;
}

export interface AppStatusDto {
  onboardingRequired: boolean;
  credential: CredentialStatusDto;
  settings: SettingsDto;
  supportedExtensions: string[];
  openaiAvailable: boolean;
}

export interface MeetingSummaryDto {
  id: string;
  title: string;
  startedAt: string | null;
  processingState: ProcessingState;
  durationMs: number | null;
  durationLabel: string | null;
  sourceFileName: string | null;
  errorCategory: ErrorCategory | null;
}

export interface SpeakerDto {
  id: string;
  label: string;
  renamed: boolean;
  identityResolved: boolean;
  segmentCount: number;
}

export interface SegmentDto {
  id: string;
  speakerId: string;
  startMs: number;
  endMs: number;
  timestamp: string;
  text: string;
}

export interface EvidenceItemDto {
  id: string;
  text: string;
  assignee: string | null;
  dueAt: string | null;
  evidenceSegmentIds: string[];
}

export interface AnalysisDto {
  summary: string;
  decisions: EvidenceItemDto[];
  actionItems: EvidenceItemDto[];
  openQuestions: EvidenceItemDto[];
}

export interface MeetingDetailDto {
  meeting: MeetingSummaryDto;
  speakers: SpeakerDto[];
  segments: SegmentDto[];
  analysis: AnalysisDto | null;
  speakerScope: SpeakerScope;
  speakerScopeNote: string | null;
}

export interface ProgressPayload {
  runId: string;
  meetingId: string;
  stage: ProcessingStage;
  currentPart: number | null;
  totalParts: number | null;
  errorCategory: ErrorCategory | null;
}

export const STAGE_LABELS: Record<ProcessingStage, string> = {
  preparing_audio: "Preparing audio…",
  uploading: "Uploading…",
  transcribing: "Transcribing…",
  analyzing: "Analyzing…",
  saving: "Saving…",
  completed: "Completed",
  failed: "Failed",
};

export function isAppError(value: unknown): value is AppErrorDto {
  return (
    typeof value === "object" &&
    value !== null &&
    "category" in value &&
    "message" in value &&
    typeof (value as AppErrorDto).message === "string"
  );
}

/**
 * Normalises anything thrown by the bridge into an error the UI can render.
 * A raw provider payload can never reach here, because the native layer only
 * ever returns a redacted category and message.
 */
export function toAppError(value: unknown): AppErrorDto {
  if (isAppError(value)) {
    return value;
  }

  return {
    category: "provider",
    message: "Something went wrong. Please try again.",
    retryable: true,
  };
}
