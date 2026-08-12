import { AlertTriangle } from "lucide-react";

import { Button } from "@meeting-intelligence/ui";

import type { AppErrorDto } from "../native/types";

export interface ErrorNoticeProps {
  error: AppErrorDto;
  onRetry?: () => void;
  onDismiss?: () => void;
}

const CATEGORY_TITLES: Record<AppErrorDto["category"], string> = {
  credential: "API key problem",
  quota: "No remaining credit",
  network: "Network problem",
  audio: "Recording problem",
  configuration: "Setup needed",
  invalid_provider_output: "Unexpected provider result",
  storage: "Storage problem",
  provider: "Processing failed",
  canceled: "Cancelled",
};

export function ErrorNotice({ error, onRetry, onDismiss }: ErrorNoticeProps) {
  return (
    <div
      role="alert"
      className="flex items-start gap-3 rounded-control border border-danger/40 bg-danger/5 px-4 py-3"
    >
      <AlertTriangle
        aria-hidden="true"
        className="mt-0.5 shrink-0 text-danger"
        size={18}
      />
      <div className="flex-1 space-y-2">
        <p className="text-sm font-semibold text-foreground">
          {CATEGORY_TITLES[error.category]}
        </p>
        <p className="text-sm text-muted">{error.message}</p>
        {(onRetry ?? onDismiss) ? (
          <div className="flex flex-wrap gap-2 pt-1">
            {onRetry && error.retryable ? (
              <Button onClick={onRetry} size="sm" variant="secondary">
                Retry
              </Button>
            ) : null}
            {onDismiss ? (
              <Button onClick={onDismiss} size="sm" variant="ghost">
                Dismiss
              </Button>
            ) : null}
          </div>
        ) : null}
      </div>
    </div>
  );
}
