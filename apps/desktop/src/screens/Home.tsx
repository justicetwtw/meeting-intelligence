import { useEffect, useState } from "react";
import { FileAudio, Upload } from "lucide-react";

import { Button, StatusBadge } from "@meeting-intelligence/ui";

import { useBridge } from "../native/bridge-context";
import type { MeetingSummaryDto, ProcessingState } from "../native/types";

export interface HomeProps {
  meetings: MeetingSummaryDto[];
  supportedExtensions: string[];
  onOpenMeeting: (meetingId: string) => void;
  onImport: (path: string) => void;
}

const STATE_LABELS: Record<ProcessingState, string> = {
  pending: "Pending",
  processing: "Processing",
  completed: "Completed",
  failed: "Failed",
};

export function Home({
  meetings,
  supportedExtensions,
  onOpenMeeting,
  onImport,
}: HomeProps) {
  const bridge = useBridge();
  const [hovering, setHovering] = useState(false);

  useEffect(() => {
    let unsubscribe: (() => void) | undefined;
    let active = true;

    void bridge
      .onFileDrop((event) => {
        setHovering(event.hovering);
        const [first] = event.paths;
        if (first) {
          onImport(first);
        }
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
  }, [bridge, onImport]);

  async function chooseFile() {
    const path = await bridge.pickAudioFile(supportedExtensions);
    if (path) {
      onImport(path);
    }
  }

  return (
    <div className="space-y-10">
      <section aria-labelledby="import-heading" className="space-y-4">
        <h1 className="text-xl font-semibold" id="import-heading">
          Import a recording
        </h1>
        <div
          data-testid="drop-zone"
          className={`flex flex-col items-center justify-center gap-3 rounded-control border-2 border-dashed px-6 py-12 text-center transition-colors ${
            hovering ? "border-accent bg-accent/5" : "border-border bg-surface"
          }`}
        >
          <Upload aria-hidden="true" className="text-muted" size={24} />
          <p className="text-sm font-medium">Drop an audio file here</p>
          <p className="text-xs text-muted">
            {supportedExtensions
              .slice(0, 6)
              .map((extension) => `.${extension}`)
              .join("  ·  ")}
          </p>
          <Button
            className="mt-2"
            onClick={() => void chooseFile()}
            variant="secondary"
          >
            Choose file
          </Button>
        </div>
      </section>

      <section aria-labelledby="recent-heading" className="space-y-4">
        <h2
          className="text-sm font-semibold uppercase tracking-wide text-muted"
          id="recent-heading"
        >
          Recent meetings
        </h2>

        {meetings.length === 0 ? (
          <div className="flex flex-col items-start gap-2 rounded-control border border-border bg-surface px-5 py-8">
            <FileAudio aria-hidden="true" className="text-muted" size={20} />
            <p className="text-sm font-medium">No meetings yet</p>
            <p className="text-sm text-muted">
              Import a recording to build your local meeting library.
            </p>
          </div>
        ) : (
          <ul className="divide-y divide-border overflow-hidden rounded-control border border-border bg-surface">
            {meetings.map((meeting) => (
              <li key={meeting.id}>
                <button
                  className="flex w-full items-center gap-4 px-4 py-3 text-left transition-colors hover:bg-subtle focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent"
                  onClick={() => onOpenMeeting(meeting.id)}
                  type="button"
                >
                  <span className="w-28 shrink-0 font-mono text-xs text-muted">
                    {formatDate(meeting.startedAt)}
                  </span>
                  <span className="flex-1 truncate text-sm font-medium">
                    {meeting.title}
                  </span>
                  <StatusBadge>{STATE_LABELS[meeting.processingState]}</StatusBadge>
                  <span className="w-20 shrink-0 text-right font-mono text-xs text-muted">
                    {meeting.durationLabel ?? "—"}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}

function formatDate(startedAt: string | null): string {
  if (!startedAt) {
    return "—";
  }
  // Timestamps arrive as RFC 3339 in UTC; only the date is shown in the list.
  return startedAt.slice(0, 10).replace(/-/gu, "/");
}
