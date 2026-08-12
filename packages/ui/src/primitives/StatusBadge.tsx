import type { HTMLAttributes } from "react";

export type StatusBadgeProps = HTMLAttributes<HTMLSpanElement>;

export function StatusBadge({ className = "", ...props }: StatusBadgeProps) {
  return (
    <span
      role="status"
      className={`inline-flex shrink-0 items-center gap-1.5 rounded-full border border-border bg-subtle px-2.5 py-1 text-xs font-semibold text-foreground ${className}`}
      {...props}
    />
  );
}
