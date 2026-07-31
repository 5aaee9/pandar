"use client";

import { useState } from "react";

import { AgentPairingGuidance } from "./agent-pairing-guidance";


import type { AttentionItem, Health, Severity } from "./dashboard-attention";
import { JobHistory, PrinterInventory } from "./dashboard-inventory";
import { FleetStatusStrip } from "./dashboard-overview";
import dynamic from "next/dynamic";

const DiagnosticsSection = dynamic(
  () => import("./diagnostics-section").then((mod) => mod.DiagnosticsSection),
  {
    loading: () => (
      <div className="rounded-md border border-border bg-card p-4">
        <div className="h-4 w-32 animate-pulse rounded bg-muted" />
        <div className="mt-2 h-3 w-48 animate-pulse rounded bg-muted" />
      </div>
    ),
  }
);
import { LinkedAgentsSection } from "./diagnostics-panel";
import { DispatchDialog } from "./dispatch-dialog";
import type {
  Agent,
  AuthMetadata,
  AuditEvent,
  Command,
  CommandResultData,
  Job,
  Printer,
  Tenant,
  TenantToken,
} from "./dashboard-types";
import type {
  LiveState,
  RuntimeNotification,
} from "./dashboard-runtime-helpers";
import type { DashboardView } from "./dashboard-shell";
import { LinkPrinterForm } from "./link-printer-form";
import { NeedsAttention } from "./needs-attention";
import { PrinterMismatchCoordinator } from "./printer-mismatch-dialog";

export type DashboardViewContentProps = {
  view: Exclude<DashboardView, "users" | "settings">;
  auth: AuthMetadata;
  selectedTenant: Tenant | null;
  health: Health;
  attentionItems: AttentionItem[];
  topSeverity: Severity | null;
  liveState: LiveState;
  lastEventAt: string | null;
  fleetEmpty: boolean;
  printers: Printer[];
  agents: Agent[];
  jobs: Job[];
  nowMs: number;
  selectedCommand: Command | null;
  commandData: CommandResultData | null;
  notifications: RuntimeNotification[];
  tenantTokens: TenantToken[];
  auditEvents: AuditEvent[];
  adminUnavailable: boolean;
  adminLoadError: boolean;
  canManageJobs: boolean;
};

export function DashboardViewContent(props: DashboardViewContentProps) {
  if (props.view === "devices") {
    return <DevicesView {...props} />;
  }
  if (props.view === "jobs") {
    return <JobsView {...props} />;
  }
  return <AgentsView {...props} />;
}

function DevicesView({
  health,
  attentionItems,
  topSeverity,
  liveState,
  lastEventAt,
  fleetEmpty,
  selectedTenant,
  printers,
  agents,
  jobs,
  nowMs,
}: DashboardViewContentProps) {
  const [reprintJob, setReprintJob] = useState<Job | null>(null);

  return (
    <>
      <div className="space-y-4">
        <FleetStatusStrip
          health={health}
          attentionCount={attentionItems.length}
          topSeverity={topSeverity}
          liveState={liveState}
          lastEventAt={lastEventAt}
          fleetEmpty={fleetEmpty}
          tenantId={selectedTenant?.id}
        />
        <NeedsAttention
          items={attentionItems}
          onOpenReprint={(jobId) => {
            const job = jobs.find((candidate) => candidate.id === jobId);
            if (job) setReprintJob(job);
          }}
          selectedTenant={selectedTenant}
        />
        <PrinterMismatchCoordinator
          key={selectedTenant?.id ?? "no-tenant"}
          printers={printers}
        >
          <PrinterInventory
            selectedTenant={selectedTenant}
            printers={printers}
            agents={agents}
            nowMs={nowMs}
          />
        </PrinterMismatchCoordinator>
      </div>
      <DispatchDialog
        onOpenChange={(open) => {
          if (!open) setReprintJob(null);
        }}
        open={reprintJob !== null}
        printers={printers}
        selectedTenant={selectedTenant}
        sourceJob={reprintJob}
      />
    </>
  );
}

function JobsView({
  selectedTenant,
  printers,
  agents,
  jobs,
  nowMs,
  canManageJobs,
}: DashboardViewContentProps) {
  const [dispatch, setDispatch] = useState<{
    open: boolean;
    sourceJob: Job | null;
  }>({ open: false, sourceJob: null });

  return (
    <>
      <JobHistory
        canManageJobs={canManageJobs}
        agents={agents}
        jobs={jobs}
        nowMs={nowMs}
        onOpenDispatch={() => setDispatch({ open: true, sourceJob: null })}
        onOpenReprint={(sourceJob) => setDispatch({ open: true, sourceJob })}
        printers={printers}
        selectedTenant={selectedTenant}
      />
      <DispatchDialog
        open={dispatch.open}
        onOpenChange={(open) =>
          setDispatch((current) => ({
            open,
            sourceJob: open ? current.sourceJob : null,
          }))
        }
        printers={printers}
        selectedTenant={selectedTenant}
        sourceJob={dispatch.sourceJob}
      />
    </>
  );
}

function AgentsView({
  selectedTenant,
  agents,
  printers,
  selectedCommand,
  commandData,
  adminUnavailable,
}: DashboardViewContentProps) {
  return (
    <>
      <AgentPairingGuidance
        selectedTenant={selectedTenant}
        restricted={adminUnavailable}
      />
      <LinkPrinterForm selectedTenant={selectedTenant} agents={agents} />
      <LinkedAgentsSection selectedTenant={selectedTenant} agents={agents} />
      <DiagnosticsSection
        selectedTenant={selectedTenant}
        printers={printers}
        selectedCommand={selectedCommand}
        commandData={commandData}
      />
    </>
  );
}
