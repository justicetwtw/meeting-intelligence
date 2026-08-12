import { useId } from "react";
import type { InputHTMLAttributes, ReactNode } from "react";

export interface TextFieldProps extends InputHTMLAttributes<HTMLInputElement> {
  label: string;
  /** Guidance shown under the field. */
  hint?: ReactNode;
  /** Replaces the hint and marks the field invalid. */
  error?: string;
}

export function TextField({
  label,
  hint,
  error,
  className = "",
  id,
  ...props
}: TextFieldProps) {
  const generatedId = useId();
  const fieldId = id ?? generatedId;
  const describedBy = error ? `${fieldId}-error` : hint ? `${fieldId}-hint` : undefined;

  return (
    <div className="flex flex-col gap-1.5">
      <label className="text-sm font-medium" htmlFor={fieldId}>
        {label}
      </label>
      <input
        id={fieldId}
        aria-describedby={describedBy}
        aria-invalid={error ? true : undefined}
        className={`min-h-10 rounded-control border bg-surface px-3 py-2 text-sm text-foreground placeholder:text-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent ${
          error ? "border-danger" : "border-border"
        } ${className}`}
        {...props}
      />
      {error ? (
        <p className="text-xs text-danger" id={`${fieldId}-error`}>
          {error}
        </p>
      ) : hint ? (
        <p className="text-xs text-muted" id={`${fieldId}-hint`}>
          {hint}
        </p>
      ) : null}
    </div>
  );
}
