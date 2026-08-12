import { useEffect, useState } from "react";
import { FlaskConical, KeyRound, Moon, Network, Sun } from "lucide-react";

import { Button, IconToggle, StatusBadge } from "@meeting-intelligence/ui";

type Theme = "light" | "dark";

function getInitialTheme(): Theme {
  if (
    typeof window !== "undefined" &&
    window.matchMedia?.("(prefers-color-scheme: dark)").matches
  ) {
    return "dark";
  }

  return "light";
}

export function App() {
  const [theme, setTheme] = useState<Theme>(getInitialTheme);
  const [readiness, setReadiness] = useState("Waiting for a local check.");
  const isDark = theme === "dark";

  useEffect(() => {
    document.documentElement.classList.toggle("dark", isDark);
  }, [isDark]);

  return (
    <div className="min-h-screen bg-canvas text-foreground">
      <header className="border-b border-border bg-surface">
        <div className="mx-auto flex max-w-3xl items-center justify-between px-6 py-4">
          <div>
            <p className="text-sm font-semibold">Meeting Intelligence</p>
            <p className="text-xs text-muted">Early-development desktop shell</p>
          </div>
          <IconToggle
            aria-label={`Switch to ${isDark ? "light" : "dark"} theme`}
            pressed={isDark}
            onPressedChange={(pressed) => setTheme(pressed ? "dark" : "light")}
          >
            {isDark ? (
              <Sun aria-hidden="true" size={18} />
            ) : (
              <Moon aria-hidden="true" size={18} />
            )}
          </IconToggle>
        </div>
      </header>

      <main className="mx-auto max-w-3xl px-6 py-10">
        <section aria-labelledby="runtime-heading" className="space-y-6">
          <div className="flex items-start justify-between gap-4">
            <div>
              <h1 id="runtime-heading" className="text-xl font-semibold">
                Local development runtime
              </h1>
              <p className="mt-2 max-w-xl text-sm leading-6 text-muted">
                The bootstrap uses deterministic fixtures. It does not contact an AI
                provider or require credentials.
              </p>
            </div>
            <StatusBadge>
              <FlaskConical aria-hidden="true" size={14} />
              Mock mode
            </StatusBadge>
          </div>

          <dl className="grid gap-px overflow-hidden rounded-xl border border-border bg-border sm:grid-cols-3">
            <RuntimeFact
              icon={<FlaskConical aria-hidden="true" size={18} />}
              label="Pipeline"
              value="Fixture-backed"
            />
            <RuntimeFact
              icon={<KeyRound aria-hidden="true" size={18} />}
              label="API key"
              value="Not required"
            />
            <RuntimeFact
              icon={<Network aria-hidden="true" size={18} />}
              label="Provider network"
              value="Disabled"
            />
          </dl>

          <div className="flex flex-wrap items-center gap-3">
            <Button onClick={() => setReadiness("Mock fixture ready.")}>
              Check mock readiness
            </Button>
            <p aria-live="polite" className="text-sm text-muted">
              {readiness}
            </p>
          </div>
        </section>
      </main>
    </div>
  );
}

interface RuntimeFactProps {
  icon: React.ReactNode;
  label: string;
  value: string;
}

function RuntimeFact({ icon, label, value }: RuntimeFactProps) {
  return (
    <div className="bg-surface p-4">
      <div className="mb-3 text-muted">{icon}</div>
      <dt className="text-xs font-medium uppercase tracking-wide text-muted">
        {label}
      </dt>
      <dd className="mt-1 text-sm font-medium">{value}</dd>
    </div>
  );
}
