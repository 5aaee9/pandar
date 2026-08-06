import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

/**
 * Shared Tailwind class strings for the Pandar frontend.
 * Keep these in one place to avoid repeating the same long class chains.
 */

/** Focus ring classes for native form controls (select, input). */
export const focusRingClasses =
  "focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50";

/** Standard select/input styling for compact inline controls. */
export const inputSmClasses = `h-8 rounded-md border border-input bg-background px-2 text-xs text-foreground ${focusRingClasses} disabled:cursor-not-allowed disabled:opacity-60`;

/** Monospace identifier text (IDs, serial numbers, job codes). */
export const monoIdClasses = "font-mono text-xs text-muted-foreground";

/** Hover state for dense list/table rows. */
export const rowHoverClasses =
  "transition-colors duration-150 ease-out hover:bg-muted/40";

/** Scrollable table container. */
export const tableScrollClasses = "overflow-x-auto";

/** Muted background for section headers and form panels. */
export const mutedBgClasses = "bg-muted/20";

/** Muted background for subsection headers. */
export const mutedBgSubtleClasses = "bg-muted/30";

/** Standard form input/select styling (h-9, full width, theme colors). */
export const inputClasses =
  "h-9 w-full rounded-md border border-input bg-background px-2 text-sm text-foreground shadow-xs outline-none transition-[color,box-shadow] placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50 disabled:cursor-not-allowed disabled:opacity-60";
