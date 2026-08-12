import { useCallback, useEffect, useState } from "react";
import { Home as HomeIcon, Moon, Settings as SettingsIcon, Sun } from "lucide-react";

import { Button, IconToggle, Spinner } from "@meeting-intelligence/ui";

import { ErrorNotice } from "./components/ErrorNotice";
import type { NativeBridge } from "./native/bridge";
import { BridgeContext, useBridge } from "./native/bridge-context";
import {
  toAppError,
  type AppErrorDto,
  type AppStatusDto,
  type MeetingDetailDto,
  type MeetingSummaryDto,
  type ProcessingStage,
  type ThemeChoice,
} from "./native/types";
import { Home } from "./screens/Home";
import { MeetingDetail } from "./screens/MeetingDetail";
import { Onboarding } from "./screens/Onboarding";
import { Processing } from "./screens/Processing";
import { Settings } from "./screens/Settings";

type View =
  | { name: "home" }
  | { name: "processing"; runId: string; fileName: string }
  | { name: "meeting"; meetingId: string }
  | { name: "settings" };

export interface AppProps {
  /** Injected in tests; production resolves the real Tauri bridge. */
  bridge?: NativeBridge;
}

export function App({ bridge }: AppProps = {}) {
  return (
    <BridgeContext.Provider value={bridge ?? null}>
      <AppShell />
    </BridgeContext.Provider>
  );
}

function AppShell() {
  const bridge = useBridge();
  const [status, setStatus] = useState<AppStatusDto | null>(null);
  const [meetings, setMeetings] = useState<MeetingSummaryDto[]>([]);
  const [detail, setDetail] = useState<MeetingDetailDto | null>(null);
  const [view, setView] = useState<View>({ name: "home" });
  const [stage, setStage] = useState<ProcessingStage>("preparing_audio");
  const [parts, setParts] = useState<{ current: number | null; total: number | null }>({
    current: null,
    total: null,
  });
  const [error, setError] = useState<AppErrorDto | null>(null);
  const [loading, setLoading] = useState(true);
  const [onboardingDismissed, setOnboardingDismissed] = useState(false);
  const [lastImportPath, setLastImportPath] = useState<string | null>(null);

  const refreshMeetings = useCallback(async () => {
    try {
      setMeetings(await bridge.listMeetings());
    } catch (thrown) {
      setError(toAppError(thrown));
    }
  }, [bridge]);

  useEffect(() => {
    let active = true;
    void (async () => {
      try {
        const [nextStatus, nextMeetings] = await Promise.all([
          bridge.appStatus(),
          bridge.listMeetings(),
        ]);
        if (active) {
          setStatus(nextStatus);
          setMeetings(nextMeetings);
        }
      } catch (thrown) {
        if (active) {
          setError(toAppError(thrown));
        }
      } finally {
        if (active) {
          setLoading(false);
        }
      }
    })();
    return () => {
      active = false;
    };
  }, [bridge]);

  useEffect(() => {
    let unsubscribe: (() => void) | undefined;
    let active = true;
    void bridge
      .onProgress((payload) => {
        setStage(payload.stage);
        setParts({ current: payload.currentPart, total: payload.totalParts });
      })
      .then((stop) => {
        if (active) {
          unsubscribe = stop;
        } else {
          stop();
        }
      });
    return () => {
      active = false;
      unsubscribe?.();
    };
  }, [bridge]);

  useTheme(status?.settings.theme ?? "system");

  const startImport = useCallback(
    async (path: string) => {
      const runId = `run-${String(Date.now())}`;
      const fileName = path.split(/[\\/]/u).pop() ?? path;
      setLastImportPath(path);
      setError(null);
      setStage("preparing_audio");
      setParts({ current: null, total: null });
      setView({ name: "processing", runId, fileName });

      try {
        const imported = await bridge.importMeeting({ runId, path });
        setDetail(imported);
        setStage("completed");
        await refreshMeetings();
        setView({ name: "meeting", meetingId: imported.meeting.id });
      } catch (thrown) {
        setError(toAppError(thrown));
        setStage("failed");
        await refreshMeetings();
      }
    },
    [bridge, refreshMeetings],
  );

  const openMeeting = useCallback(
    async (meetingId: string) => {
      setError(null);
      try {
        setDetail(await bridge.getMeeting(meetingId));
        setView({ name: "meeting", meetingId });
      } catch (thrown) {
        setError(toAppError(thrown));
      }
    },
    [bridge],
  );

  const retryMeeting = useCallback(
    async (meetingId: string, fileName: string) => {
      const runId = `run-${String(Date.now())}`;
      setError(null);
      setStage("preparing_audio");
      setView({ name: "processing", runId, fileName });
      try {
        const updated = await bridge.retryMeeting({ runId, meetingId });
        setDetail(updated);
        setStage("completed");
        await refreshMeetings();
        setView({ name: "meeting", meetingId });
      } catch (thrown) {
        setError(toAppError(thrown));
        setStage("failed");
        await refreshMeetings();
      }
    },
    [bridge, refreshMeetings],
  );

  if (loading) {
    return (
      <main className="flex min-h-screen items-center justify-center bg-canvas text-foreground">
        <Spinner label="Loading your meetings…" />
      </main>
    );
  }

  if (status?.onboardingRequired && !onboardingDismissed) {
    return (
      <div className="min-h-screen bg-canvas text-foreground">
        <Onboarding
          onConnected={() => {
            setOnboardingDismissed(true);
            void bridge
              .appStatus()
              .then(setStatus)
              .catch(() => undefined);
          }}
          onSkip={() => setOnboardingDismissed(true)}
        />
      </div>
    );
  }

  return (
    <div className="min-h-screen bg-canvas text-foreground">
      <header className="border-b border-border bg-surface">
        <div className="mx-auto flex max-w-4xl items-center justify-between gap-4 px-6 py-3">
          <div>
            <p className="text-sm font-semibold">Meeting Intelligence</p>
            <p className="text-xs text-muted">Local library · your own OpenAI key</p>
          </div>
          <nav className="flex items-center gap-1">
            <Button
              onClick={() => {
                setView({ name: "home" });
                void refreshMeetings();
              }}
              size="sm"
              variant="ghost"
            >
              <HomeIcon aria-hidden="true" size={14} />
              Meetings
            </Button>
            <Button
              onClick={() => setView({ name: "settings" })}
              size="sm"
              variant="ghost"
            >
              <SettingsIcon aria-hidden="true" size={14} />
              Settings
            </Button>
            <ThemeToggle
              onChange={(theme) => {
                if (!status) {
                  return;
                }
                const next = { ...status.settings, theme };
                setStatus({ ...status, settings: next });
                void bridge.updateSettings(next).catch(() => undefined);
              }}
              theme={status?.settings.theme ?? "system"}
            />
          </nav>
        </div>
      </header>

      <main className="mx-auto max-w-4xl px-6 py-8">
        {error && view.name !== "processing" ? (
          <div className="mb-6">
            <ErrorNotice error={error} onDismiss={() => setError(null)} />
          </div>
        ) : null}

        {view.name === "home" && status ? (
          <Home
            meetings={meetings}
            onImport={(path) => void startImport(path)}
            onOpenMeeting={(meetingId) => void openMeeting(meetingId)}
            supportedExtensions={status.supportedExtensions}
          />
        ) : null}

        {view.name === "processing" ? (
          <Processing
            currentPart={parts.current}
            error={error}
            fileName={view.fileName}
            onBack={() => {
              setError(null);
              setView({ name: "home" });
            }}
            onCancel={() => void bridge.cancelProcessing(view.runId)}
            onRetry={() => {
              if (lastImportPath) {
                void startImport(lastImportPath);
              }
            }}
            stage={stage}
            totalParts={parts.total}
          />
        ) : null}

        {view.name === "meeting" && detail ? (
          <MeetingDetail
            detail={detail}
            onBack={() => {
              setView({ name: "home" });
              void refreshMeetings();
            }}
            onChange={setDetail}
            onRetry={() =>
              void retryMeeting(
                detail.meeting.id,
                detail.meeting.sourceFileName ?? detail.meeting.title,
              )
            }
          />
        ) : null}

        {view.name === "settings" && status ? (
          <Settings onStatusChange={setStatus} status={status} />
        ) : null}
      </main>
    </div>
  );
}

function ThemeToggle({
  theme,
  onChange,
}: {
  theme: ThemeChoice;
  onChange: (theme: ThemeChoice) => void;
}) {
  const isDark = theme === "dark";
  return (
    <IconToggle
      aria-label={`Switch to ${isDark ? "light" : "dark"} theme`}
      onPressedChange={(pressed) => onChange(pressed ? "dark" : "light")}
      pressed={isDark}
    >
      {isDark ? (
        <Sun aria-hidden="true" size={16} />
      ) : (
        <Moon aria-hidden="true" size={16} />
      )}
    </IconToggle>
  );
}

function useTheme(theme: ThemeChoice) {
  useEffect(() => {
    const prefersDark =
      typeof window !== "undefined" &&
      (window.matchMedia?.("(prefers-color-scheme: dark)").matches ?? false);
    document.documentElement.classList.toggle(
      "dark",
      theme === "dark" || (theme === "system" && prefersDark),
    );
  }, [theme]);
}
