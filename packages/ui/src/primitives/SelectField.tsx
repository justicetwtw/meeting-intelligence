import { useId } from "react";
import type { ReactNode, SelectHTMLAttributes } from "react";

export interface SelectFieldProps extends SelectHTMLAttributes<HTMLSelectElement> {
  label: string;
  hint?: ReactNode;
}

export function SelectField({
  label,
  hint,
  className = "",
  id,
  children,
  ...props
}: SelectFieldProps) {
  const generatedId = useId();
  const fieldId = id ?? generatedId;

  return (
    <div className="flex flex-col gap-1.5">
      <label className="text-sm font-medium" htmlFor={fieldId}>
        {label}
      </label>
      <select
        id={fieldId}
        aria-describedby={hint ? `${fieldId}-hint` : undefined}
        className={`min-h-10 rounded-control border border-border bg-surface px-3 py-2 text-sm text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent ${className}`}
        {...props}
      >
        {children}
      </select>
      {hint ? (
        <p className="text-xs text-muted" id={`${fieldId}-hint`}>
          {hint}
        </p>
      ) : null}
    </div>
  );
}
