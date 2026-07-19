"use client";

import { useTranslations } from "next-intl";
import { LogOutIcon } from "lucide-react";

import { LanguageSwitcher } from "../components/language-switcher";
import { ThemeSwitcher } from "../components/theme-switcher";
import { AdminSectionGuard } from "./admin-section-states";
import {
  CreateAgentPairingForm,
  TenantAuditPanel,
  TenantSecretsPanel,
} from "./admin-settings-panel";
import { CreateJoinLinkForm, TenantUsersPanel } from "./admin-users-panel";
import type { DashboardViewContentProps } from "./dashboard-view-content";
import type {
  Agent,
  AuditEvent,
  AuthMetadata,
  JoinLink,
  Tenant,
  TenantToken,
  User,
  UserIdentity,
} from "./dashboard-types";
import {
  RuntimeStatusPanel,
  TenantSettings,
} from "./dashboard-runtime-sections";
import { logoutHref } from "./dashboard-shell";
import { SectionHeader } from "./dashboard-ui";

export function UsersView({
  auth,
  selectedTenant,
  users,
  userIdentities,
  joinLinks,
  adminUnavailable,
  adminLoadError,
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
        loadError={adminLoadError}
      />
    </>
  );
}

export function SettingsView({
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
  adminLoadError,
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
        loadError={adminLoadError}
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

function PreferencePanel({
  title,
  description,
  children,
}: {
  title: string;
  description: string;
  children: React.ReactNode;
}) {
  return (
    <section className="rounded-md border border-border bg-card px-4 py-3 transition-colors duration-150 ease-out hover:border-border/80">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <h2 className="text-base font-semibold text-card-foreground">
            {title}
          </h2>
          <p className="mt-0.5 text-sm text-muted-foreground">
            {description}
          </p>
        </div>
        {children}
      </div>
    </section>
  );
}

function LanguageSettingsPanel() {
  const t = useTranslations("dashboardShell");
  return (
    <PreferencePanel title={t("languageTitle")} description={t("languageDescription")}>
      <LanguageSwitcher />
    </PreferencePanel>
  );
}

function ThemeSettingsPanel() {
  const t = useTranslations("dashboardShell");
  return (
    <PreferencePanel title={t("themeTitle")} description={t("themeDescription")}>
      <ThemeSwitcher />
    </PreferencePanel>
  );
}

function UsersAdminSection({
  selectedTenant,
  users,
  userIdentities,
  joinLinks,
  unavailable,
  loadError,
}: {
  selectedTenant: Tenant | null;
  users: User[];
  userIdentities: UserIdentity[];
  joinLinks: JoinLink[];
  unavailable: boolean;
  loadError: boolean;
}) {
  const t = useTranslations("admin");
  return (
    <AdminSectionGuard
      title={t("users")}
      selectedTenant={selectedTenant}
      loadError={loadError}
      unavailable={unavailable}
    >
      {(tenant) => (
        <section className="overflow-hidden rounded-md border border-border bg-card">
          <SectionHeader
            title={t("users")}
            subtitle={t("subtitleTenant", { name: tenant.display_name })}
            meta={t("usersMeta", { count: users.length })}
          />
          <div className="border-b border-border bg-muted/20 px-4 py-4">
            <CreateJoinLinkForm tenantId={tenant.id} />
          </div>
          <TenantUsersPanel
            selectedTenant={tenant}
            users={users}
            userIdentities={userIdentities}
            joinLinks={joinLinks}
          />
        </section>
      )}
    </AdminSectionGuard>
  );
}

function SettingsAdminSection({
  selectedTenant,
  tenantTokens,
  agents,
  auditEvents,
  unavailable,
  loadError,
  nowMs,
}: {
  selectedTenant: Tenant | null;
  tenantTokens: TenantToken[];
  agents: Agent[];
  auditEvents: AuditEvent[];
  unavailable: boolean;
  loadError: boolean;
  nowMs: number;
}) {
  const t = useTranslations("admin");
  return (
    <AdminSectionGuard
      title={t("title")}
      selectedTenant={selectedTenant}
      loadError={loadError}
      unavailable={unavailable}
    >
      {(tenant) => (
        <section className="overflow-hidden rounded-md border border-border bg-card text-card-foreground">
          <SectionHeader
            title={t("title")}
            subtitle={t("subtitleTenant", { name: tenant.display_name })}
            meta={t("metaSecrets")}
          />
          <div className="border-b border-border">
            <TenantSecretsPanel
              selectedTenant={tenant}
              tenantTokens={tenantTokens}
              nowMs={nowMs}
            />
          </div>
          <div className="grid items-start gap-0 lg:grid-cols-2">
            <div className="border-b border-border lg:border-b-0 lg:border-r">
              <div className="border-b border-border bg-muted/20 p-4">
                <div className="rounded-md border border-border bg-background p-4">
                  <CreateAgentPairingForm tenantId={tenant.id} />
                </div>
              </div>
              <TenantSecretsPanel
                selectedTenant={tenant}
                agents={agents}
                nowMs={nowMs}
              />
            </div>
            <div className="min-w-0">
              <TenantAuditPanel
                selectedTenant={tenant}
                auditEvents={auditEvents}
              />
            </div>
          </div>
        </section>
      )}
    </AdminSectionGuard>
  );
}

function LogoutPanel({ auth }: { auth: AuthMetadata }) {
  const t = useTranslations("dashboardShell");
  const signOutHref = logoutHref(auth);

  return (
    <section className="rounded-md border border-border bg-card px-4 py-3 transition-colors duration-150 ease-out hover:border-border/80">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <h2 className="text-base font-semibold text-foreground">
            {t("logoutTitle")}
          </h2>
          <p className="mt-0.5 text-sm text-muted-foreground">
            {signOutHref ? t("logoutDescription") : t("logoutUnavailable")}
          </p>
        </div>
        {signOutHref ? (
          <a
            className="inline-flex h-9 items-center justify-center gap-1.5 rounded-md bg-primary px-3 text-sm font-medium text-primary-foreground transition-colors duration-150 ease-out hover:bg-primary/80 focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50"
            href={signOutHref}
          >
            <LogOutIcon aria-hidden="true" className="size-4" />
            {t("logout")}
          </a>
        ) : null}
      </div>
    </section>
  );
}
