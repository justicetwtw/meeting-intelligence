import { useState } from "react";
import { CheckCircle2, ShieldCheck } from "lucide-react";

import { Button, Spinner, TextField } from "@meeting-intelligence/ui";

import { ErrorNotice } from "../components/ErrorNotice";
import { useBridge } from "../native/bridge-context";
import { toAppError, type AppErrorDto } from "../native/types";

export interface OnboardingProps {
  onConnected: () => void;
  /** Shown when the operator can continue without a key (mock providers). */
  onSkip?: () => void;
}

export function Onboarding({ onConnected, onSkip }: OnboardingProps) {
  const bridge = useBridge();
  const [key, setKey] = useState("");
  const [busy, setBusy] = useState<"testing" | "saving" | null>(null);
  const [tested, setTested] = useState(false);
  const [error, setError] = useState<AppErrorDto | null>(null);

  async function testConnection() {
    setBusy("testing");
    setError(null);
    try {
      await bridge.testOpenAiConnection(key);
      setTested(true);
    } catch (thrown) {
      setTested(false);
      setError(toAppError(thrown));
    } finally {
      setBusy(null);
    }
  }

  async function saveAndContinue() {
    setBusy("saving");
    setError(null);
    try {
      await bridge.saveOpenAiKey(key);
      // The raw value is dropped as soon as the native layer has it.
      setKey("");
      onConnected();
    } catch (thrown) {
      setError(toAppError(thrown));
    } finally {
      setBusy(null);
    }
  }

  return (
    <main className="mx-auto flex min-h-screen max-w-xl flex-col justify-center gap-8 px-6 py-12">
      <header className="space-y-2">
        <h1 className="text-2xl font-semibold">Connect OpenAI</h1>
        <p className="text-sm leading-6 text-muted">
          Meeting Intelligence uses your own OpenAI account. The key is stored in the
          Windows Credential Manager and never written to a settings file, the local
          database, or a log.
        </p>
      </header>

      <form
        className="space-y-5"
        onSubmit={(event) => {
          event.preventDefault();
          void saveAndContinue();
        }}
      >
        <TextField
          autoComplete="off"
          hint="Starts with sk-. Paste it once; it is not shown again."
          label="OpenAI API key"
          onChange={(event) => {
            setKey(event.target.value);
            setTested(false);
          }}
          placeholder="sk-…"
          spellCheck={false}
          type="password"
          value={key}
        />

        {error ? <ErrorNotice error={error} onDismiss={() => setError(null)} /> : null}

        {tested ? (
          <p className="flex items-center gap-2 text-sm text-success" role="status">
            <CheckCircle2 aria-hidden="true" size={16} />
            Connection succeeded.
          </p>
        ) : null}

        <div className="flex flex-wrap items-center gap-3">
          <Button
            disabled={key.trim().length === 0 || busy !== null}
            onClick={() => void testConnection()}
            variant="secondary"
          >
            Test connection
          </Button>
          <Button disabled={key.trim().length === 0 || busy !== null} type="submit">
            Continue
          </Button>
          {busy ? (
            <Spinner label={busy === "testing" ? "Testing…" : "Saving…"} />
          ) : null}
        </div>
      </form>

      <footer className="space-y-3 border-t border-border pt-6">
        <p className="flex items-start gap-2 text-xs leading-5 text-muted">
          <ShieldCheck aria-hidden="true" className="mt-0.5 shrink-0" size={14} />
          <span>
            Cloud mode: the audio and transcript needed for the processing you request
            are sent to OpenAI. Your meeting history stays on this device.
          </span>
        </p>
        {onSkip ? (
          <Button onClick={onSkip} size="sm" variant="ghost">
            Continue without a key (mock providers only)
          </Button>
        ) : null}
      </footer>
    </main>
  );
}
