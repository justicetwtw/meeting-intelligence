import { useMemo, useState } from "react";
import { ArrowLeft, Download, Info, RotateCcw, Search } from "lucide-react";

import { Button, SelectField, Spinner, TextField } from "@meeting-intelligence/ui";

import { ErrorNotice } from "../components/ErrorNotice";
import { useBridge } from "../native/bridge-context";
import {
  toAppError,
  type AppErrorDto,
  type EvidenceItemDto,
  type ExportFormat,
  type MeetingDetailDto,
} from "../native/types";

export interface MeetingDetailProps {
  detail: MeetingDetailDto;
  onChange: (detail: MeetingDetailDto) => void;
  onBack: () => void;
  onRetry: () => void;
}

type Tab = "overview" | "transcript";

export function MeetingDetail({
  detail,
  onChange,
  onBack,
  onRetry,
}: MeetingDetailProps) {
  const [tab, setTab] = useState<Tab>("overview");
  const [error, setError] = useState<AppErrorDto | null>(null);
  const [exportedTo, setExportedTo] = useState<string | null>(null);

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div className="space-y-1">
          <Button className="-ml-2" onClick={onBack} size="sm" variant="ghost">
            <ArrowLeft aria-hidden="true" size={14} />
            Meetings
          </Button>
          <h1 className="text-xl font-semibold">{detail.meeting.title}</h1>
          <p className="font-mono text-xs text-muted">
            {detail.meeting.startedAt?.slice(0, 10) ?? "—"} ·{" "}
            {detail.meeting.durationLabel ?? "—"} ·{" "}
            {detail.meeting.sourceFileName ?? "no source file"}
          </p>
        </div>
        <ExportControls
          meetingId={detail.meeting.id}
          onError={setError}
          onExported={setExportedTo}
        />
      </div>

      {detail.meeting.processingState === "failed" ? (
        <div className="flex flex-wrap items-center gap-3 rounded-control border border-border bg-subtle px-4 py-3">
          <p className="flex-1 text-sm">This meeting did not finish processing.</p>
          <Button onClick={onRetry} size="sm" variant="secondary">
            <RotateCcw aria-hidden="true" size={14} />
            Retry processing
          </Button>
        </div>
      ) : null}

      {detail.speakerScopeNote ? (
        <p className="flex items-start gap-2 rounded-control border border-border bg-subtle px-4 py-3 text-sm text-muted">
          <Info aria-hidden="true" className="mt-0.5 shrink-0" size={16} />
          {detail.speakerScopeNote}
        </p>
      ) : null}

      {error ? <ErrorNotice error={error} onDismiss={() => setError(null)} /> : null}
      {exportedTo ? (
        <p className="text-sm text-success" role="status">
          Exported to {exportedTo}
        </p>
      ) : null}

      <div className="border-b border-border">
        <div className="flex gap-1" role="tablist">
          <TabButton current={tab} onSelect={setTab} value="overview">
            Overview
          </TabButton>
          <TabButton current={tab} onSelect={setTab} value="transcript">
            Transcript
          </TabButton>
        </div>
      </div>

      {tab === "overview" ? (
        <Overview detail={detail} />
      ) : (
        <TranscriptPanel detail={detail} onChange={onChange} onError={setError} />
      )}
    </div>
  );
}

interface TabButtonProps {
  value: Tab;
  current: Tab;
  onSelect: (tab: Tab) => void;
  children: string;
}

function TabButton({ value, current, onSelect, children }: TabButtonProps) {
  const selected = value === current;
  return (
    <button
      aria-selected={selected}
      className={`-mb-px border-b-2 px-3 py-2 text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent ${
        selected
          ? "border-accent text-foreground"
          : "border-transparent text-muted hover:text-foreground"
      }`}
      onClick={() => onSelect(value)}
      role="tab"
      type="button"
    >
      {children}
    </button>
  );
}

function Overview({ detail }: { detail: MeetingDetailDto }) {
  if (!detail.analysis) {
    return (
      <p className="rounded-control border border-border bg-surface px-5 py-8 text-sm text-muted">
        This meeting has no analysis yet.
      </p>
    );
  }

  const { analysis } = detail;
  return (
    <div className="space-y-8" role="tabpanel">
      <section aria-labelledby="summary-heading" className="space-y-2">
        <h2
          className="text-sm font-semibold uppercase tracking-wide text-muted"
          id="summary-heading"
        >
          Summary
        </h2>
        <p className="whitespace-pre-line text-sm leading-6">{analysis.summary}</p>
      </section>

      <EvidenceSection heading="Decisions" items={analysis.decisions} />
      <EvidenceSection
        heading="Action items"
        items={analysis.actionItems}
        showAssignee
      />
      <EvidenceSection heading="Open questions" items={analysis.openQuestions} />
    </div>
  );
}

interface EvidenceSectionProps {
  heading: string;
  items: EvidenceItemDto[];
  showAssignee?: boolean;
}

function EvidenceSection({
  heading,
  items,
  showAssignee = false,
}: EvidenceSectionProps) {
  return (
    <section aria-label={heading} className="space-y-3">
      <h2 className="text-sm font-semibold uppercase tracking-wide text-muted">
        {heading}
      </h2>
      {items.length === 0 ? (
        <p className="text-sm text-muted">None recorded.</p>
      ) : (
        <ul className="space-y-3">
          {items.map((item) => (
            <li className="space-y-1 border-l-2 border-border pl-4" key={item.id}>
              <p className="text-sm leading-6">{item.text}</p>
              <p className="flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted">
                {showAssignee ? (
                  <span>Owner: {item.assignee ?? "unassigned"}</span>
                ) : null}
                {showAssignee ? <span>Due: {item.dueAt ?? "not set"}</span> : null}
                <span className="font-mono">
                  {item.evidenceSegmentIds.length > 0
                    ? item.evidenceSegmentIds.join(", ")
                    : "no evidence"}
                </span>
              </p>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

interface TranscriptPanelProps {
  detail: MeetingDetailDto;
  onChange: (detail: MeetingDetailDto) => void;
  onError: (error: AppErrorDto | null) => void;
}

function TranscriptPanel({ detail, onChange, onError }: TranscriptPanelProps) {
  const bridge = useBridge();
  const [query, setQuery] = useState("");
  const [editing, setEditing] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const [saving, setSaving] = useState(false);

  const labels = useMemo(
    () => new Map(detail.speakers.map((speaker) => [speaker.id, speaker.label])),
    [detail.speakers],
  );

  const segments = useMemo(() => {
    const needle = query.trim().toLocaleLowerCase();
    if (needle.length === 0) {
      return detail.segments;
    }
    return detail.segments.filter(
      (segment) =>
        segment.text.toLocaleLowerCase().includes(needle) ||
        (labels.get(segment.speakerId) ?? "").toLocaleLowerCase().includes(needle),
    );
  }, [detail.segments, labels, query]);

  async function saveRename(speakerId: string) {
    setSaving(true);
    onError(null);
    try {
      const updated = await bridge.renameSpeaker({
        meetingId: detail.meeting.id,
        speakerId,
        displayName: draft.trim().length > 0 ? draft.trim() : null,
      });
      onChange(updated);
      setEditing(null);
      setDraft("");
    } catch (thrown) {
      onError(toAppError(thrown));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="space-y-6" role="tabpanel">
      <section aria-labelledby="speakers-heading" className="space-y-3">
        <h2
          className="text-sm font-semibold uppercase tracking-wide text-muted"
          id="speakers-heading"
        >
          Speakers
        </h2>
        <ul className="flex flex-wrap gap-2">
          {detail.speakers.map((speaker) => (
            <li key={speaker.id}>
              {editing === speaker.id ? (
                <form
                  className="flex items-end gap-2"
                  onSubmit={(event) => {
                    event.preventDefault();
                    void saveRename(speaker.id);
                  }}
                >
                  <TextField
                    autoFocus
                    label={`Name for ${speaker.label}`}
                    onChange={(event) => setDraft(event.target.value)}
                    value={draft}
                  />
                  <Button disabled={saving} size="sm" type="submit">
                    Save
                  </Button>
                  <Button
                    disabled={saving}
                    onClick={() => setEditing(null)}
                    size="sm"
                    variant="ghost"
                  >
                    Cancel
                  </Button>
                  {saving ? <Spinner label="Saving…" /> : null}
                </form>
              ) : (
                <Button
                  onClick={() => {
                    setEditing(speaker.id);
                    setDraft(speaker.renamed ? speaker.label : "");
                  }}
                  size="sm"
                  variant="secondary"
                >
                  {speaker.label}
                  <span className="font-mono text-[0.65rem] text-muted">
                    {speaker.segmentCount}
                  </span>
                </Button>
              )}
            </li>
          ))}
        </ul>
      </section>

      <label className="flex items-center gap-2 rounded-control border border-border bg-surface px-3">
        <Search aria-hidden="true" className="text-muted" size={16} />
        <input
          aria-label="Search transcript"
          className="min-h-10 flex-1 bg-transparent text-sm text-foreground placeholder:text-muted focus-visible:outline-none"
          onChange={(event) => setQuery(event.target.value)}
          placeholder="Search transcript"
          type="search"
          value={query}
        />
      </label>

      {segments.length === 0 ? (
        <p className="text-sm text-muted">No transcript lines match that search.</p>
      ) : (
        <ol className="space-y-4">
          {segments.map((segment) => (
            <li className="grid grid-cols-[5rem_1fr] gap-4" key={segment.id}>
              <span className="pt-0.5 font-mono text-xs text-muted">
                {segment.timestamp}
              </span>
              <div className="space-y-1">
                <p className="text-xs font-semibold text-muted">
                  {labels.get(segment.speakerId) ?? segment.speakerId}
                </p>
                <p className="text-sm leading-6">{segment.text}</p>
              </div>
            </li>
          ))}
        </ol>
      )}
    </div>
  );
}

interface ExportControlsProps {
  meetingId: string;
  onExported: (path: string) => void;
  onError: (error: AppErrorDto | null) => void;
}

function ExportControls({ meetingId, onExported, onError }: ExportControlsProps) {
  const bridge = useBridge();
  const [format, setFormat] = useState<ExportFormat>("markdown");
  const [busy, setBusy] = useState(false);

  async function exportMeeting() {
    setBusy(true);
    onError(null);
    try {
      const directory = await bridge.pickDirectory();
      const path = await bridge.exportMeeting(
        directory === null ? { meetingId, format } : { meetingId, format, directory },
      );
      onExported(path);
    } catch (thrown) {
      onError(toAppError(thrown));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex items-end gap-2">
      <SelectField
        label="Export as"
        onChange={(event) => setFormat(event.target.value as ExportFormat)}
        value={format}
      >
        <option value="markdown">Markdown</option>
        <option value="json">JSON</option>
        <option value="docx">Word (.docx)</option>
      </SelectField>
      <Button disabled={busy} onClick={() => void exportMeeting()} variant="secondary">
        <Download aria-hidden="true" size={14} />
        Export
      </Button>
    </div>
  );
}
