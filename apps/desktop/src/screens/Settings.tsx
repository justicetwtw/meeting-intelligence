import { useState } from "react";
import { CheckCircle2, FolderOpen, ShieldCheck } from "lucide-react";

import { Button, SelectField, Spinner, TextField } from "@meeting-intelligence/ui";

import { ErrorNotice } from "../components/ErrorNotice";
import { useBridge } from "../native/bridge-context";
import {
  toAppError,
  type AnalysisMode,
  type AppErrorDto,
  type AppStatusDto,
  type ProviderChoice,
  type SettingsDto,
  type ThemeChoice,
} from "../native/types";

export interface SettingsProps {
  status: AppStatusDto;
  onStatusChange: (status: AppStatusDto) => void;
}

export function Settings({ status, onStatusChange }: SettingsProps) {
  const bridge = useBridge();
  const [settings, setSettings] = useState<SettingsDto>(status.settings);
  const [key, setKey] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<AppErrorDto | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  async function persist(next: SettingsDto) {
    setSettings(next);
    setError(null);
    try {
      const saved = await bridge.updateSettings(next);
      setSettings(saved);
      onStatusChange({ ...status, settings: saved });
    } catch (thrown) {
      setError(toAppError(thrown));
    }
  }

  async function run(name: string, action: () => Promise<void>) {
    setBusy(name);
    setError(null);
    setNotice(null);
    try {
      await action();
    } catch (thrown) {
      setError(toAppError(thrown));
    } finally {
      setBusy(null);
    }
  }

  return (
    <div className="max-w-2xl space-y-10">
      <h1 className="text-xl font-semibold">Settings</h1>

      {error ? <ErrorNotice error={error} onDismiss={() => setError(null)} /> : null}
      {notice ? (
        <p className="flex items-center gap-2 text-sm text-success" role="status">
          <CheckCircle2 aria-hidden="true" size={16} />
          {notice}
        </p>
      ) : null}

      <section aria-labelledby="connection-heading" className="space-y-4">
        <h2
          className="text-sm font-semibold uppercase tracking-wide text-muted"
          id="connection-heading"
        >
          OpenAI connection
        </h2>
        <p className="text-sm text-muted">
          {status.credential.configured
            ? `A key is configured (${status.credential.hint ?? "stored"}). It is kept in the Windows Credential Manager and is never displayed again.`
            : "No key is configured."}
        </p>

        <TextField
          autoComplete="off"
          label={status.credential.configured ? "Replace API key" : "OpenAI API key"}
          onChange={(event) => setKey(event.target.value)}
          placeholder="sk-…"
          spellCheck={false}
          type="password"
          value={key}
        />

        <div className="flex flex-wrap items-center gap-3">
          <Button
            disabled={busy !== null}
            onClick={() =>
              void run("test", async () => {
                await bridge.testOpenAiConnection(
                  key.trim().length > 0 ? key : undefined,
                );
                setNotice("Connection succeeded.");
              })
            }
            variant="secondary"
          >
            Test connection
          </Button>
          <Button
            disabled={key.trim().length === 0 || busy !== null}
            onClick={() =>
              void run("save", async () => {
                const credential = await bridge.saveOpenAiKey(key);
                setKey("");
                onStatusChange({ ...status, credential, onboardingRequired: false });
                setNotice("Key saved.");
              })
            }
          >
            Save key
          </Button>
          {status.credential.configured ? (
            <Button
              disabled={busy !== null}
              onClick={() =>
                void run("remove", async () => {
                  const credential = await bridge.removeOpenAiKey();
                  onStatusChange({ ...status, credential });
                  setNotice("Key removed.");
                })
              }
              variant="danger"
            >
              Remove key
            </Button>
          ) : null}
          {busy ? <Spinner label="Working…" /> : null}
        </div>
      </section>

      <section aria-labelledby="providers-heading" className="space-y-4">
        <h2
          className="text-sm font-semibold uppercase tracking-wide text-muted"
          id="providers-heading"
        >
          Providers
        </h2>
        <div className="grid gap-4 sm:grid-cols-2">
          <SelectField
            hint="Turns speech into a speaker-aware transcript."
            label="Transcription provider"
            onChange={(event) =>
              void persist({
                ...settings,
                transcriptionProvider: event.target.value as ProviderChoice,
              })
            }
            value={settings.transcriptionProvider}
          >
            <option value="openai">OpenAI</option>
            <option value="mock">Mock (offline fixtures)</option>
          </SelectField>
          <SelectField
            hint="Produces the summary, decisions, actions, and questions."
            label="Intelligence provider"
            onChange={(event) =>
              void persist({
                ...settings,
                intelligenceProvider: event.target.value as ProviderChoice,
              })
            }
            value={settings.intelligenceProvider}
          >
            <option value="openai">OpenAI</option>
            <option value="mock">Mock (offline fixtures)</option>
          </SelectField>
        </div>

        <SelectField
          hint="Deep spends more time and tokens on harder meetings."
          label="Analysis mode"
          onChange={(event) =>
            void persist({
              ...settings,
              analysisMode: event.target.value as AnalysisMode,
            })
          }
          value={settings.analysisMode}
        >
          <option value="fast">Fast</option>
          <option value="standard">Standard</option>
          <option value="deep">Deep</option>
        </SelectField>

        <TextField
          hint="Optional. For example zh or en. Leave empty to let the provider detect it."
          label="Language hint"
          onChange={(event) =>
            void persist({
              ...settings,
              languageHint:
                event.target.value.trim().length > 0 ? event.target.value : null,
            })
          }
          value={settings.languageHint ?? ""}
        />
      </section>

      <section aria-labelledby="files-heading" className="space-y-4">
        <h2
          className="text-sm font-semibold uppercase tracking-wide text-muted"
          id="files-heading"
        >
          Files
        </h2>
        <div className="flex items-end gap-2">
          <TextField
            className="flex-1"
            hint="Where exports are written by default."
            label="Output folder"
            onChange={(event) =>
              void persist({
                ...settings,
                outputDirectory:
                  event.target.value.trim().length > 0 ? event.target.value : null,
              })
            }
            value={settings.outputDirectory ?? ""}
          />
          <Button
            onClick={() =>
              void run("output", async () => {
                const directory = await bridge.pickDirectory();
                if (directory) {
                  await persist({ ...settings, outputDirectory: directory });
                }
              })
            }
            variant="secondary"
          >
            <FolderOpen aria-hidden="true" size={14} />
            Browse
          </Button>
        </div>

        <TextField
          hint="Only needed when ffmpeg and ffprobe are not on PATH."
          label="FFmpeg folder"
          onChange={(event) =>
            void persist({
              ...settings,
              ffmpegDirectory:
                event.target.value.trim().length > 0 ? event.target.value : null,
            })
          }
          value={settings.ffmpegDirectory ?? ""}
        />
      </section>

      <section aria-labelledby="appearance-heading" className="space-y-4">
        <h2
          className="text-sm font-semibold uppercase tracking-wide text-muted"
          id="appearance-heading"
        >
          Appearance
        </h2>
        <SelectField
          label="Theme"
          onChange={(event) =>
            void persist({ ...settings, theme: event.target.value as ThemeChoice })
          }
          value={settings.theme}
        >
          <option value="system">Match system</option>
          <option value="light">Light</option>
          <option value="dark">Dark</option>
        </SelectField>
      </section>

      <section
        aria-labelledby="privacy-heading"
        className="space-y-2 border-t border-border pt-6"
      >
        <h2
          className="text-sm font-semibold uppercase tracking-wide text-muted"
          id="privacy-heading"
        >
          Privacy
        </h2>
        <p className="flex items-start gap-2 text-sm leading-6 text-muted">
          <ShieldCheck aria-hidden="true" className="mt-1 shrink-0" size={16} />
          <span>
            Cloud mode: the audio and transcript needed for the AI processing you select
            are sent to the configured provider. Your meeting history, transcripts, and
            exports stay on this device. No telemetry or analytics is collected.
          </span>
        </p>
      </section>
    </div>
  );
}
