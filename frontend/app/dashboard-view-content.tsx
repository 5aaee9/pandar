"use client";

import { useState } from "react";
import { useTranslations } from "next-intl";

import { LanguageSwitcher } from "../components/language-switcher";
import { ThemeSwitcher } from "../components/theme-switcher";
import { AgentPairingGuidance } from "./agent-pairing-guidance";
import { DiagnosticsSection, LinkedAgentsSection } from "./diagnostics-panel";
import { DispatchDialog } from "./dispatch-dialog";
import { LinkPrinterForm } from "./link-printer-form";
import {
  CreateAgentPairingForm,
  TenantAuditPanel,
  TenantSecretsPanel,
} from "./admin-settings-panel";
import { CreateJoinLinkForm, TenantUsersPanel } from "./admin-users-panel";
import type { AttentionItem, Health, Severity } from "./dashboard-attention";
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
import { JobHistory, PrinterInventory } from "./dashboard-inventory";
import { FleetStatusStrip } from "./dashboard-overview";
import { NeedsAttention } from "./needs-attention";
import {
  RuntimeStatusPanel,
  TenantSettings,
} from "./dashboard-runtime-sections";
import { logoutHref, type DashboardView } from "./dashboard-shell";
import { EmptyState, SectionHeader } from "./dashboard-ui";
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

function UsersView({
  auth,
  selectedTenant,
  users,
  userIdentities,
  joinLinks,
  adminUnavailable,
}: DashboardViewContentProps) {
  return (
    <>
      <LogoutPanel auth={auth} />
      <UsersAdminSection
        selectedTenant={selectedTenant}
        users={users}
        userIdentities={userIdentities}
        joinLinks={joinLinks}
        unavailable={adminUnavailable}
      />
    </>
  );
}

function SettingsView({
  auth,
  liveState,
  lastEventAt,
  notifications,
  selectedTenant,
  agents,
  printers,
  tenantTokens,
  auditEvents,
  adminUnavailable,
  nowMs,
}: DashboardViewContentProps) {
  return (
    <>
      <LanguageSettingsPanel />
      <ThemeSettingsPanel />
      <TenantSettings
        auth={auth}
        selectedTenant={selectedTenant}
        agents={agents}
        printers={printers}
      />
      <SettingsAdminSection
        selectedTenant={selectedTenant}
        tenantTokens={tenantTokens}
        agents={agents}
        auditEvents={auditEvents}
        unavailable={adminUnavailable}
        nowMs={nowMs}
      />
      <RuntimeStatusPanel
        auth={auth}
        liveState={liveState}
        lastEventAt={lastEventAt}
        notifications={notifications}
        selectedTenant={selectedTenant}
      />
    </>
  );
}

function LanguageSettingsPanel() {
  const t = useTranslations("dashboardShell");
  return (
    <section className="rounded-md border border-border bg-card px-4 py-3">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <h2 className="text-base font-semibold text-card-foreground">
            {t("languageTitle")}
          </h2>
          <p className="mt-0.5 text-sm text-muted-foreground">
            {t("languageDescription")}
          </p>
        </div>
        <LanguageSwitcher />
      </div>
    </section>
  );
}

function ThemeSettingsPanel() {
  const t = useTranslations("dashboardShell");
  return (
    <section className="rounded-md border border-border bg-card px-4 py-3">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <h2 className="text-base font-semibold text-card-foreground">
            {t("themeTitle")}
          </h2>
          <p className="mt-0.5 text-sm text-muted-foreground">
            {t("themeDescription")}
          </p>
        </div>
        <ThemeSwitcher />
      </div>
    </section>
  );
}

function UsersAdminSection({
  selectedTenant,
  users,
  userIdentities,
  joinLinks,
  unavailable,
}: {
  selectedTenant: Tenant | null;
  users: User[];
  userIdentities: UserIdentity[];
  joinLinks: JoinLink[];
  unavailable: boolean;
}) {
  const t = useTranslations("admin");
  if (!selectedTenant) {
    return (
      <section className="overflow-hidden rounded-md border border-slate-300 bg-slate-50">
        <SectionHeader
          title={t("users")}
          subtitle={t("subtitleNone")}
          meta={t("metaAdmin")}
        />
        <EmptyState title={t("noTenantTitle")} message={t("noTenantMessage")} />
      </section>
    );
  }
  if (unavailable) {
    return (
      <section className="overflow-hidden rounded-md border border-slate-300 bg-slate-50">
        <SectionHeader
          title={t("users")}
          subtitle={t("subtitleUnavailable", {
            name: selectedTenant.display_name,
          })}
          meta={t("metaRestricted")}
        />
        <EmptyState
          title={t("unavailableTitle")}
          message={t("unavailableMessage")}
        />
      </section>
    );
  }
  return (
    <section className="overflow-hidden rounded-md border border-slate-300 bg-slate-50">
      <SectionHeader
        title={t("users")}
        subtitle={t("subtitleTenant", { name: selectedTenant.display_name })}
        meta={t("usersMeta", { count: users.length })}
      />
      <div className="border-b border-slate-200 px-4 py-4">
        <CreateJoinLinkForm tenantId={selectedTenant.id} />
      </div>
      <TenantUsersPanel
        selectedTenant={selectedTenant}
        users={users}
        userIdentities={userIdentities}
        joinLinks={joinLinks}
      />
    </section>
  );
}

function SettingsAdminSection({
  selectedTenant,
  tenantTokens,
  agents,
  auditEvents,
  unavailable,
  nowMs,
}: {
  selectedTenant: Tenant | null;
  tenantTokens: TenantToken[];
  agents: Agent[];
  auditEvents: AuditEvent[];
  unavailable: boolean;
  nowMs: number;
}) {
  const t = useTranslations("admin");
  if (!selectedTenant) {
    return (
      <section className="overflow-hidden rounded-lg border border-border bg-card text-card-foreground shadow-sm">
        <SectionHeader
          title={t("title")}
          subtitle={t("subtitleNone")}
          meta={t("metaSecrets")}
        />
        <EmptyState title={t("noTenantTitle")} message={t("noTenantMessage")} />
      </section>
    );
  }
  if (unavailable) {
    return (
      <section className="overflow-hidden rounded-lg border border-border bg-card text-card-foreground shadow-sm">
        <SectionHeader
          title={t("title")}
          subtitle={t("subtitleUnavailable", {
            name: selectedTenant.display_name,
          })}
          meta={t("metaRestricted")}
        />
        <EmptyState
          title={t("unavailableTitle")}
          message={t("unavailableMessage")}
        />
      </section>
    );
  }
  return (
    <section className="overflow-hidden rounded-lg border border-border bg-card text-card-foreground shadow-sm">
      <SectionHeader
        title={t("title")}
        subtitle={t("subtitleTenant", { name: selectedTenant.display_name })}
        meta={t("metaSecrets")}
      />
      <div className="border-b border-border">
        <TenantSecretsPanel
          selectedTenant={selectedTenant}
          tenantTokens={tenantTokens}
          nowMs={nowMs}
        />
      </div>
      <div className="grid items-start gap-0 lg:grid-cols-2">
        <div className="border-b border-border lg:border-b-0 lg:border-r">
          <div className="border-b border-border bg-muted/20 p-4">
            <div className="rounded-lg border border-border bg-background/60 p-4 shadow-xs">
              <CreateAgentPairingForm tenantId={selectedTenant.id} />
            </div>
          </div>
          <TenantSecretsPanel
            selectedTenant={selectedTenant}
            agents={agents}
            nowMs={nowMs}
          />
        </div>
        <div className="min-w-0">
          <TenantAuditPanel
            selectedTenant={selectedTenant}
            auditEvents={auditEvents}
          />
        </div>
      </div>
    </section>
  );
}

function LogoutPanel({ auth }: { auth: AuthMetadata }) {
  const t = useTranslations("dashboardShell");
  const signOutHref = logoutHref(auth);

  return (
    <section className="rounded-md border border-slate-300 bg-white px-4 py-3">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <h2 className="text-base font-semibold text-slate-950">
            {t("logoutTitle")}
          </h2>
          <p className="mt-0.5 text-sm text-slate-600">
            {signOutHref ? t("logoutDescription") : t("logoutUnavailable")}
          </p>
        </div>
        {signOutHref ? (
          <a
            className="inline-flex h-9 items-center justify-center rounded-md bg-slate-950 px-3 text-sm font-medium text-white hover:bg-slate-800"
            href={signOutHref}
          >
            {t("logout")}
          </a>
        ) : null}
      </div>
    </section>
  );
}
