import { Check } from "lucide-react";

import { Button, Spinner } from "@meeting-intelligence/ui";

import { ErrorNotice } from "../components/ErrorNotice";
import { STAGE_LABELS, type AppErrorDto, type ProcessingStage } from "../native/types";

export interface ProcessingProps {
  fileName: string;
  stage: ProcessingStage;
  currentPart: number | null;
  totalParts: number | null;
  error: AppErrorDto | null;
  onCancel: () => void;
  onRetry: () => void;
  onBack: () => void;
}

/**
 * The pipeline cannot measure meaningful fractional progress, so the screen
 * shows the current stage and, when a recording had to be split, which part is
 * being uploaded. It never shows an invented percentage.
 */
const ORDER: ProcessingStage[] = [
  "preparing_audio",
  "uploading",
  "transcribing",
  "analyzing",
  "saving",
];

export function Processing({
  fileName,
  stage,
  currentPart,
  totalParts,
  error,
  onCancel,
  onRetry,
  onBack,
}: ProcessingProps) {
  const activeIndex = ORDER.indexOf(stage);
  const failed = stage === "failed" || error !== null;

  return (
    <div className="mx-auto max-w-xl space-y-8">
      <header className="space-y-1">
        <h1 className="text-xl font-semibold">Processing meeting</h1>
        <p className="truncate font-mono text-xs text-muted">{fileName}</p>
      </header>

      <ol className="space-y-3">
        {ORDER.map((item, index) => {
          const done = activeIndex > index || stage === "completed";
          const active = activeIndex === index && !failed;
          return (
            <li
              key={item}
              aria-current={active ? "step" : undefined}
              className="flex items-center gap-3"
            >
              <span
                aria-hidden="true"
                className={`flex size-6 items-center justify-center rounded-full border text-xs ${
                  done
                    ? "border-success bg-success/10 text-success"
                    : active
                      ? "border-accent text-accent"
                      : "border-border text-muted"
                }`}
              >
                {done ? <Check size={14} /> : index + 1}
              </span>
              <span
                className={`text-sm ${
                  done || active ? "text-foreground" : "text-muted"
                }`}
              >
                {STAGE_LABELS[item]}
                {active && item === "uploading" && currentPart && totalParts ? (
                  <span className="ml-2 font-mono text-xs text-muted">
                    part {currentPart} of {totalParts}
                  </span>
                ) : null}
              </span>
            </li>
          );
        })}
      </ol>

      <div aria-live="polite" className="min-h-6">
        {stage === "completed" ? (
          <p className="text-sm text-success">Completed.</p>
        ) : failed ? null : (
          <Spinner label={STAGE_LABELS[stage]} />
        )}
      </div>

      {error ? <ErrorNotice error={error} onRetry={onRetry} /> : null}

      <div className="flex flex-wrap gap-3">
        {failed || stage === "completed" ? (
          <Button onClick={onBack} variant="secondary">
            Back to meetings
          </Button>
        ) : (
          <Button onClick={onCancel} variant="secondary">
            Cancel
          </Button>
        )}
      </div>
    </div>
  );
}
