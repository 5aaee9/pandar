"use client";

import { useState } from "react";

import { AgentPairingGuidance } from "./agent-pairing-guidance";
import { SettingsView, UsersView } from "./dashboard-admin-views";
import type { AttentionItem, Health, Severity } from "./dashboard-attention";
import { JobHistory, PrinterInventory } from "./dashboard-inventory";
import { FleetStatusStrip } from "./dashboard-overview";
import { DiagnosticsSection, LinkedAgentsSection } from "./diagnostics-panel";
import { DispatchDialog } from "./dispatch-dialog";
import type {
  Agent,
  AuthMetadata,
  AuditEvent,
  Command,
  CommandResultData,
  Job,
  JoinLink,
  Printer,
  Tenant,
  TenantToken,
  User,
  UserIdentity,
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
  view: DashboardView;
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
  users: User[];
  userIdentities: UserIdentity[];
  tenantTokens: TenantToken[];
  joinLinks: JoinLink[];
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
  if (props.view === "agents") {
    return <AgentsView {...props} />;
  }
  if (props.view === "users") {
    return <UsersView {...props} />;
  }
  return <SettingsView {...props} />;
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
  nowMs,
}: DashboardViewContentProps) {
  return (
    <>
      <FleetStatusStrip
        health={health}
        attentionCount={attentionItems.length}
        topSeverity={topSeverity}
        liveState={liveState}
        lastEventAt={lastEventAt}
        fleetEmpty={fleetEmpty}
        tenantId={selectedTenant?.id}
      />
      <NeedsAttention items={attentionItems} selectedTenant={selectedTenant} />
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
  const [dispatchOpen, setDispatchOpen] = useState(false);

  return (
    <>
      <JobHistory
        canManageJobs={canManageJobs}
        agents={agents}
        jobs={jobs}
        nowMs={nowMs}
        onOpenDispatch={() => setDispatchOpen(true)}
        printers={printers}
        selectedTenant={selectedTenant}
      />
      <DispatchDialog
        open={dispatchOpen}
        onOpenChange={setDispatchOpen}
        printers={printers}
        selectedTenant={selectedTenant}
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
