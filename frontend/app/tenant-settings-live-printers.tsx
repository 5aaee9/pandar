"use client";

import type { Printer, Tenant } from "./dashboard-types";

export function TenantSettingsLivePrinters({
  initialPrinters,
}: {
  initialPrinters: Printer[];
  selectedTenant: Tenant | null;
}) {
  if (initialPrinters.length === 0) {
    return <div className="mt-2 text-sm text-muted-foreground">No printers reported</div>;
  }

  return (
    <div className="mt-2 grid gap-2 md:grid-cols-2">
      {initialPrinters.map((printer) => (
        <div
          key={printer.id}
          className="min-w-0 rounded-md border border-border bg-muted/30 px-3 py-2.5 text-xs transition-colors duration-150 ease-out hover:bg-muted/50"
        >
          <div className="truncate font-medium text-foreground">{printer.name}</div>
          <div className="truncate text-muted-foreground">{printer.model ?? "Unknown model"}</div>
          <div className="mt-1 text-muted-foreground/80">Run diagnostics</div>
        </div>
      ))}
    </div>
  );
}
