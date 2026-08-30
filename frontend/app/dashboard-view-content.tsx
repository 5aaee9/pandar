"use client";

import dynamic from "next/dynamic";
import { useState } from "react";

import { AgentsSection } from "./agents-section";
import { DISCOVERY_COMMAND_KIND } from "./command-status";
import type { AttentionItem, Health, Severity } from "./dashboard-attention";
import { JobHistory, PrinterInventory } from "./dashboard-inventory";
import { FleetStatusStrip } from "./dashboard-overview";
import type {
  Agent,
  Command,
  CommandResultData,
  DiscoveryResultData,
  Job,
  Printer,
  Tenant,
} from "./dashboard-types";
import type { LiveState } from "./dashboard-runtime-helpers";
import { DiscoverySection } from "./discovery-section";
import { DispatchDialog } from "./dispatch-dialog";
import { NeedsAttention } from "./needs-attention";
import { PrinterMismatchCoordinator } from "./printer-mismatch-dialog";

const DiagnosticsSection = dynamic(
  () => import("./diagnostics-section").then((mod) => mod.DiagnosticsSection),
  {
    loading: () => (
      <div className="rounded-md border border-border bg-card p-4">
        <div className="h-4 w-32 animate-pulse rounded bg-muted" />
        <div className="mt-2 h-3 w-48 animate-pulse rounded bg-muted" />
      </div>
    ),
  },
);

export type DevicesViewProps = {
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
};

export function DevicesView({
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
}: DevicesViewProps) {
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

export type JobsViewProps = {
  selectedTenant: Tenant | null;
  printers: Printer[];
  agents: Agent[];
  jobs: Job[];
  nowMs: number;
  canManageJobs: boolean;
};

export function JobsView({
  selectedTenant,
  printers,
  agents,
  jobs,
  nowMs,
  canManageJobs,
}: JobsViewProps) {
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

export type AgentsViewProps = {
  selectedTenant: Tenant | null;
  agents: Agent[];
  printers: Printer[];
  selectedCommand: Command | null;
  commandData: CommandResultData | null;
  discoveryCommand?: Command | null;
  discoveryData?: DiscoveryResultData | null;
  adminUnavailable: boolean;
};

export function AgentsView({
  selectedTenant,
  agents,
  printers,
  selectedCommand,
  commandData,
  discoveryCommand = null,
  discoveryData = null,
  adminUnavailable,
}: AgentsViewProps) {
  const selectedIsDiscovery =
    selectedCommand?.kind === DISCOVERY_COMMAND_KIND;
  return (
    <div className="grid gap-4">
      <AgentsSection
        adminUnavailable={adminUnavailable}
        agents={agents}
        printers={printers}
        selectedTenant={selectedTenant}
      />
      {selectedTenant && discoveryCommand ? (
        <DiscoverySection
          agents={agents}
          command={discoveryCommand}
          data={discoveryData}
          printers={printers}
          selectedTenant={selectedTenant}
        />
      ) : null}
      <DiagnosticsSection
        selectedTenant={selectedTenant}
        printers={printers}
        selectedCommand={selectedIsDiscovery ? null : selectedCommand}
        commandData={selectedIsDiscovery ? null : commandData}
      />
    </div>
  );
}
