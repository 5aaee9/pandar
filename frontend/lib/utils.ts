import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

/**
 * Shared Tailwind class strings for the Pandar frontend.
 * Keep these in one place to avoid repeating the same long class chains.
 */

/** Small action button (border + background) used in dense tables/lists. */
export const actionButtonSm =
  "h-8 rounded-md border border-border bg-background px-2 text-xs font-medium text-foreground transition-colors duration-150 ease-out hover:bg-muted focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50 disabled:cursor-not-allowed disabled:opacity-60";

/** Danger-styled small action button for destructive operations. */
export const actionButtonSmDanger =
  "h-8 rounded-md border border-destructive/40 px-2 text-xs font-medium text-destructive transition-colors duration-150 ease-out hover:bg-destructive/10 disabled:cursor-not-allowed disabled:opacity-50";

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

/** Card panel used in Settings, Users, and admin sections. */
export const cardPanelClasses =
  "rounded-md border border-border bg-card px-4 py-3 transition-colors duration-150 ease-out hover:border-border/80";

/** Badge/pill styling for status indicators. */
export const badgeClasses =
  "inline-flex items-center gap-1 rounded-md border px-2 py-0.5 text-xs font-medium";

/** Scrollable table container. */
export const tableScrollClasses = "overflow-x-auto";
