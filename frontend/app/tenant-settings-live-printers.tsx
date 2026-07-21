"use client";

import type { Printer, Tenant } from "./dashboard-types";
import { useDashboardShellStore } from "./dashboard-shell-store";

export function TenantSettingsLivePrinters({
  initialPrinters,
  selectedTenant,
}: {
  initialPrinters: Printer[];
  selectedTenant: Tenant | null;
}) {
  const livePrinters = useDashboardShellStore((state) => state.livePrinters);
  const liveTenantId = useDashboardShellStore((state) => state.liveTenantId);
  const isLiveDataValid = liveTenantId === selectedTenant?.id;
  const printers = isLiveDataValid ? livePrinters : initialPrinters;

  if (printers.length === 0) {
    return <div className="mt-2 text-sm text-muted-foreground">No printers reported</div>;
  }

  return (
    <div className="mt-2 grid gap-2 md:grid-cols-2">
      {printers.map((printer) => (
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
