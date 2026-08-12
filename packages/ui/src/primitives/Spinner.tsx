export interface SpinnerProps {
  /** Announced to assistive technology; also used as the accessible label. */
  label: string;
  className?: string;
}

export function Spinner({ label, className = "" }: SpinnerProps) {
  return (
    <span className={`inline-flex items-center gap-2 ${className}`} role="status">
      <span
        aria-hidden="true"
        className="size-4 animate-spin rounded-full border-2 border-border border-t-accent motion-reduce:animate-none"
      />
      <span className="text-sm text-muted">{label}</span>
    </span>
  );
}
