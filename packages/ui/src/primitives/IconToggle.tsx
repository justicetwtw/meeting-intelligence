import * as Toggle from "@radix-ui/react-toggle";
import type { ComponentPropsWithoutRef } from "react";

export type IconToggleProps = ComponentPropsWithoutRef<typeof Toggle.Root>;

export function IconToggle({ className = "", ...props }: IconToggleProps) {
  return (
    <Toggle.Root
      className={`inline-flex size-10 items-center justify-center rounded-control border border-border bg-canvas text-muted transition-colors hover:bg-subtle hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent data-[state=on]:bg-subtle data-[state=on]:text-foreground ${className}`}
      {...props}
    />
  );
}
