"use client";

import Link from "next/link";
import { useTranslations } from "next-intl";
import {
  ArrowUpRightIcon,
  Building2Icon,
  KeyRoundIcon,
  MonitorCogIcon,
  PaletteIcon,
  ServerCogIcon,
  ShieldCheckIcon,
  UserRoundIcon,
} from "lucide-react";

import { FormattedDate } from "../components/formatted-date";
import { LanguageSwitcher } from "../components/language-switcher";
import { ThemeSwitcher } from "../components/theme-switcher";
import { OFFLINE_PRINTER_STATUSES } from "./dashboard-attention";
import { CreateAgentPairingForm } from "./create-agent-pairing-form";
import { TenantAuditPanel } from "./tenant-audit-panel";
import { TenantTokensTable } from "./admin-settings-token-list";
import type {
  Agent,
  AuditEvent,
  AuthMetadata,
  Printer,
  Tenant,
  TenantToken,
} from "./dashboard-types";
import { dashboardSidebarHref } from "./dashboard-shell";
import { SettingsAccountSection } from "./settings-account-section";
import { WorkspaceRenameForm } from "./settings-rename-form";
import { SettingsSection } from "./settings-section";
import { useScrollSpy } from "./use-scroll-spy";

type SettingsDashboardProps = {
  auth: AuthMetadata;
  selectedTenant: Tenant;
  membershipRole: string | null;
  canAdmin: boolean;
  agents: Agent[];
  printers: Printer[];
  tenantTokens: TenantToken[];
  auditEvents: AuditEvent[];
  adminUnavailable: boolean;
  adminLoadError: boolean;
  adminLoading: boolean;
  nowMs: number;
};

const SETTINGS_SECTIONS = [
  { id: "workspace", icon: Building2Icon, key: "workspaceTitle" },
  { id: "appearance", icon: PaletteIcon, key: "appearanceTitle" },
  { id: "access", icon: KeyRoundIcon, key: "accessTitle" },
  { id: "account", icon: UserRoundIcon, key: "accountTitle" },
] as const;

const SETTINGS_SECTION_IDS = SETTINGS_SECTIONS.map((section) => section.id);

export function SettingsDashboard(props: SettingsDashboardProps) {
  const t = useTranslations("settingsPage");
  const tRoles = useTranslations("tokens");
  const activeSection = useScrollSpy(SETTINGS_SECTION_IDS);
  const roleLabel = props.membershipRole
    ? tRoles(props.membershipRole)
    : props.auth.provider === "none"
      ? t("localAdministrator")
      : t("roleUnavailable");

  return (
    <div className="mx-auto max-w-5xl pb-10">
      <header className="flex flex-wrap items-center justify-between gap-3 pb-6">
        <div>
          <h2 className="text-2xl font-semibold tracking-tight text-foreground">
            {t("title")}
          </h2>
          <p className="mt-1 text-sm text-muted-foreground">
            {t("subtitle", { name: props.selectedTenant.display_name })}
          </p>
        </div>
        <span className="inline-flex items-center gap-1.5 rounded-full border border-border bg-card px-3 py-1.5 text-xs font-medium text-muted-foreground">
          <ShieldCheckIcon aria-hidden="true" className="size-3.5 text-success" />
          {roleLabel}
        </span>
      </header>

      <div className="grid items-start gap-6 lg:grid-cols-[13rem_minmax(0,1fr)]">
        <nav
          aria-label={t("sectionNavigation")}
          className="flex gap-1 overflow-x-auto lg:sticky lg:top-20 lg:flex-col"
        >
          {SETTINGS_SECTIONS.map((item) => {
            const Icon = item.icon;
            const active = item.id === activeSection;
            return (
              <a
                key={item.id}
                aria-current={active ? "location" : undefined}
                className={`flex shrink-0 items-center gap-2 rounded-lg px-3 py-2 text-sm transition-colors ${
                  active
                    ? "bg-muted font-medium text-foreground"
                    : "text-muted-foreground hover:bg-muted/60 hover:text-foreground"
                }`}
                href={`#${item.id}`}
              >
                <Icon aria-hidden="true" className="size-4" />
                {t(item.key)}
              </a>
            );
          })}
        </nav>

        <div className="min-w-0 space-y-6">
          <WorkspaceSection {...props} />
          <AppearanceSection />
          <AccessSection {...props} />
          <SettingsAccountSection auth={props.auth} />
        </div>
      </div>
    </div>
  );
}

function WorkspaceSection(props: SettingsDashboardProps) {
  const t = useTranslations("settingsPage");
  const connectedAgents = props.agents.filter(
    (agent) => agent.status.toLowerCase() === "online",
  ).length;
  const onlinePrinters = props.printers.filter(
    (printer) => !OFFLINE_PRINTER_STATUSES.has(printer.status.toLowerCase()),
  ).length;

  return (
    <SettingsSection
      description={t("workspaceDescription")}
      icon={Building2Icon}
      id="workspace"
      title={t("workspaceTitle")}
    >
      {props.canAdmin ? (
        <WorkspaceRenameForm tenant={props.selectedTenant} />
      ) : null}
      <dl className="grid gap-px border-b border-border bg-border sm:grid-cols-2">
        {props.canAdmin ? null : (
          <WorkspaceFact label={t("workspaceName")} value={props.selectedTenant.display_name} />
        )}
        <WorkspaceFact label={t("workspaceSlug")} value={props.selectedTenant.slug} mono />
        <WorkspaceFact
          label={t("workspaceCreated")}
          value={<FormattedDate value={props.selectedTenant.created_at} />}
        />
        <WorkspaceFact label={t("workspaceId")} value={props.selectedTenant.id} mono />
        <WorkspaceFact label={t("authProvider")} value={props.auth.provider} />
      </dl>
      <div className="grid gap-4 p-4 sm:grid-cols-2 sm:p-5">
        <WorkspaceLink
          description={t("agentsDescription", {
            connected: connectedAgents,
            total: props.agents.length,
          })}
          href={dashboardSidebarHref("agents")}
          icon={ServerCogIcon}
          title={t("manageAgents")}
        />
        <WorkspaceLink
          description={t("printersDescription", {
            online: onlinePrinters,
            total: props.printers.length,
          })}
          href={dashboardSidebarHref("devices")}
          icon={MonitorCogIcon}
          title={t("managePrinters")}
        />
      </div>
    </SettingsSection>
  );
}

function AppearanceSection() {
  const t = useTranslations("settingsPage");

  return (
    <SettingsSection
      description={t("appearanceDescription")}
      icon={PaletteIcon}
      id="appearance"
      title={t("appearanceTitle")}
    >
      <PreferenceRow
        description={t("languageDescription")}
        icon={MonitorCogIcon}
        title={t("languageTitle")}
      >
        <LanguageSwitcher />
      </PreferenceRow>
      <PreferenceRow
        description={t("themeDescription")}
        icon={PaletteIcon}
        title={t("themeTitle")}
      >
        <ThemeSwitcher />
      </PreferenceRow>
    </SettingsSection>
  );
}

function AccessSection(props: SettingsDashboardProps) {
  const t = useTranslations("settingsPage");

  if (props.adminLoading) {
    return (
      <SettingsSection description={t("accessDescription")} icon={KeyRoundIcon} id="access" title={t("accessTitle")}>
        <div className="space-y-3 p-5" aria-label={t("loadingAccess")}>
          <div className="h-16 animate-pulse rounded-lg bg-muted" />
          <div className="h-16 animate-pulse rounded-lg bg-muted" />
        </div>
      </SettingsSection>
    );
  }

  if (props.adminUnavailable || props.adminLoadError) {
    return (
      <SettingsSection description={t("accessDescription")} icon={KeyRoundIcon} id="access" title={t("accessTitle")}>
        <div className="flex gap-3 p-5" role={props.adminLoadError ? "alert" : undefined}>
          <ShieldCheckIcon aria-hidden="true" className="mt-0.5 size-5 shrink-0 text-muted-foreground" />
          <div>
            <div className="text-sm font-medium text-foreground">
              {props.adminLoadError ? t("accessLoadErrorTitle") : t("accessRestrictedTitle")}
            </div>
            <p className="mt-1 text-sm leading-6 text-muted-foreground">
              {props.adminLoadError ? t("accessLoadErrorDescription") : t("accessRestrictedDescription")}
            </p>
          </div>
        </div>
      </SettingsSection>
    );
  }

  return (
    <SettingsSection description={t("accessDescription")} icon={KeyRoundIcon} id="access" title={t("accessTitle")}>
      <TenantTokensTable
        nowMs={props.nowMs}
        tenantId={props.selectedTenant.id}
        tokens={props.tenantTokens}
      />
      <div className="grid gap-px border-t border-border bg-border xl:grid-cols-2">
        <div className="bg-card p-4 sm:p-5">
          <h4 className="text-sm font-semibold text-foreground">{t("pairAgentTitle")}</h4>
          <p className="mt-0.5 text-sm text-muted-foreground">{t("pairAgentDescription")}</p>
          <div className="mt-4">
            <CreateAgentPairingForm tenantId={props.selectedTenant.id} />
          </div>
        </div>
        <div className="bg-card">
          <TenantAuditPanel auditEvents={props.auditEvents} />
        </div>
      </div>
    </SettingsSection>
  );
}

function PreferenceRow({
  icon: Icon,
  title,
  description,
  children,
}: {
  icon: typeof PaletteIcon;
  title: string;
  description: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex flex-col gap-3 border-b border-border px-4 py-4 last:border-b-0 sm:flex-row sm:items-center sm:justify-between sm:px-5">
      <div className="flex items-start gap-3">
        <Icon aria-hidden="true" className="mt-0.5 size-4 text-muted-foreground" />
        <div>
          <div className="text-sm font-medium text-foreground">{title}</div>
          <div className="mt-0.5 text-sm text-muted-foreground">{description}</div>
        </div>
      </div>
      <div className="shrink-0 sm:pl-4">{children}</div>
    </div>
  );
}

function WorkspaceFact({
  label,
  value,
  mono = false,
}: {
  label: string;
  value: React.ReactNode;
  mono?: boolean;
}) {
  return (
    <div className="bg-card p-4 sm:px-5">
      <dt className="text-xs font-medium text-muted-foreground">{label}</dt>
      <dd
        className={`mt-1 break-all text-sm font-medium text-foreground ${
          mono ? "font-mono text-xs" : ""
        }`}
      >
        {value}
      </dd>
    </div>
  );
}

function WorkspaceLink({
  icon: Icon,
  title,
  description,
  href,
}: {
  icon: typeof PaletteIcon;
  title: string;
  description: string;
  href: string;
}) {
  return (
    <Link
      className="group rounded-lg border border-border bg-muted/20 p-3 transition-colors hover:bg-muted/50"
      href={href}
    >
      <div className="flex items-center justify-between gap-3">
        <span className="flex items-center gap-2 text-sm font-medium text-foreground">
          <Icon aria-hidden="true" className="size-4 text-muted-foreground" />
          {title}
        </span>
        <ArrowUpRightIcon
          aria-hidden="true"
          className="size-4 text-muted-foreground transition-transform group-hover:-translate-y-0.5 group-hover:translate-x-0.5"
        />
      </div>
      <p className="mt-1.5 text-xs leading-5 text-muted-foreground">{description}</p>
    </Link>
  );
}
