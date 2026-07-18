"use client";

import { useTranslations } from "next-intl";

import { LanguageSwitcher } from "../components/language-switcher";
import { ThemeSwitcher } from "../components/theme-switcher";
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
import { EmptyState, SectionHeader } from "./dashboard-ui";

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
  if (!selectedTenant) {
    return (
      <section className="overflow-hidden rounded-md border border-border bg-card">
        <SectionHeader
          title={t("users")}
          subtitle={t("subtitleNone")}
          meta={t("metaAdmin")}
        />
        <EmptyState title={t("noTenantTitle")} message={t("noTenantMessage")} />
      </section>
    );
  }
  if (loadError) {
    return (
      <section className="overflow-hidden rounded-md border border-border bg-card">
        <SectionHeader
          title={t("users")}
          subtitle={t("subtitleTenant", {
            name: selectedTenant.display_name,
          })}
          meta={t("metaAdmin")}
        />
        <EmptyState
          title={t("loadErrorTitle")}
          message={t("loadErrorMessage")}
        />
      </section>
    );
  }
  if (unavailable) {
    return (
      <section className="overflow-hidden rounded-md border border-border bg-card">
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
    <section className="overflow-hidden rounded-md border border-border bg-card">
      <SectionHeader
        title={t("users")}
        subtitle={t("subtitleTenant", { name: selectedTenant.display_name })}
        meta={t("usersMeta", { count: users.length })}
      />
      <div className="border-b border-border px-4 py-4">
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
  if (!selectedTenant) {
    return (
      <section className="overflow-hidden rounded-lg border border-border bg-card text-card-foreground">
        <SectionHeader
          title={t("title")}
          subtitle={t("subtitleNone")}
          meta={t("metaSecrets")}
        />
        <EmptyState title={t("noTenantTitle")} message={t("noTenantMessage")} />
      </section>
    );
  }
  if (loadError) {
    return (
      <section className="overflow-hidden rounded-lg border border-border bg-card text-card-foreground">
        <SectionHeader
          title={t("title")}
          subtitle={t("subtitleTenant", {
            name: selectedTenant.display_name,
          })}
          meta={t("metaSecrets")}
        />
        <EmptyState
          title={t("loadErrorTitle")}
          message={t("loadErrorMessage")}
        />
      </section>
    );
  }
  if (unavailable) {
    return (
      <section className="overflow-hidden rounded-lg border border-border bg-card text-card-foreground">
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
    <section className="overflow-hidden rounded-lg border border-border bg-card text-card-foreground">
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
            <div className="rounded-lg border border-border bg-background/60 p-4">
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
    <section className="rounded-md border border-border bg-card px-4 py-3">
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
            className="inline-flex h-9 items-center justify-center rounded-md bg-primary px-3 text-sm font-medium text-primary-foreground transition-colors duration-150 ease-out hover:bg-primary/80"
            href={signOutHref}
          >
            {t("logout")}
          </a>
        ) : null}
      </div>
    </section>
  );
}
