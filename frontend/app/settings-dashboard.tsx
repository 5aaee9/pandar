"use client";

import Link from "next/link";
import { useTranslations } from "next-intl";
import {
  ActivityIcon,
  ArrowUpRightIcon,
  Building2Icon,
  KeyRoundIcon,
  LogOutIcon,
  MonitorCogIcon,
  PaletteIcon,
  ServerCogIcon,
  ShieldCheckIcon,
  UserRoundIcon,
} from "lucide-react";

import { FormattedDate } from "../components/formatted-date";
import { LanguageSwitcher } from "../components/language-switcher";
import { ThemeSwitcher } from "../components/theme-switcher";
import { Button } from "../components/ui/button";
import { OFFLINE_PRINTER_STATUSES } from "./dashboard-attention";
import { CreateAgentPairingForm, TenantAuditPanel } from "./admin-settings-panel";
import { TenantTokensTable } from "./admin-settings-token-list";
import type {
  Agent,
  AuditEvent,
  AuthMetadata,
  Printer,
  Tenant,
  TenantToken,
} from "./dashboard-types";
import { formatAuthSource } from "./dashboard-runtime-helpers";
import { dashboardSidebarHref, logoutHref } from "./dashboard-shell";

type SettingsDashboardProps = {
  auth: AuthMetadata;
  selectedTenant: Tenant;
  membershipRole: string | null;
  agents: Agent[];
  printers: Printer[];
  tenantTokens: TenantToken[];
  auditEvents: AuditEvent[];
  adminUnavailable: boolean;
  adminLoadError: boolean;
  adminLoading: boolean;
  nowMs: number;
};

const navItems = [
  { id: "appearance", icon: PaletteIcon, key: "appearanceTitle" },
  { id: "workspace", icon: Building2Icon, key: "workspaceTitle" },
  { id: "access", icon: KeyRoundIcon, key: "accessTitle" },
  { id: "account", icon: UserRoundIcon, key: "accountTitle" },
] as const;

export function SettingsDashboard(props: SettingsDashboardProps) {
  const t = useTranslations("settingsPage");
  const tRoles = useTranslations("tokens");
  const connectedAgents = props.agents.filter(
    (agent) => agent.status.toLowerCase() === "online",
  ).length;
  const onlinePrinters = props.printers.filter(
    (printer) =>
      !OFFLINE_PRINTER_STATUSES.has(printer.status.toLowerCase()),
  ).length;
  const activeTokens = props.tenantTokens.filter(
    (token) =>
      !token.revoked_at &&
      (!token.expires_at || Date.parse(token.expires_at) > props.nowMs),
  ).length;
  const roleLabel = props.membershipRole
    ? tRoles(props.membershipRole)
    : props.auth.provider === "none"
      ? t("localAdministrator")
      : t("roleUnavailable");

  return (
    <div className="mx-auto max-w-6xl space-y-6 pb-8">
      <section className="relative overflow-hidden rounded-2xl border border-border bg-card px-5 py-6 shadow-sm sm:px-7">
        <div className="absolute inset-y-0 right-0 hidden w-1/3 bg-gradient-to-l from-muted/80 to-transparent sm:block" />
        <div className="relative flex flex-col gap-6 lg:flex-row lg:items-end lg:justify-between">
          <div className="max-w-2xl">
            <div className="mb-3 inline-flex items-center gap-2 rounded-full border border-border bg-background/80 px-2.5 py-1 text-xs font-medium text-muted-foreground">
              <ShieldCheckIcon aria-hidden="true" className="size-3.5 text-success" />
              {roleLabel}
            </div>
            <h2 className="text-2xl font-semibold tracking-tight text-foreground sm:text-3xl">
              {t("title")}
            </h2>
            <p className="mt-2 max-w-xl text-sm leading-6 text-muted-foreground">
              {t("subtitle", { name: props.selectedTenant.display_name })}
            </p>
          </div>
          <dl className="grid grid-cols-3 gap-2 sm:gap-3">
            <Metric value={`${connectedAgents}/${props.agents.length}`} label={t("agentsOnline")} />
            <Metric value={`${onlinePrinters}/${props.printers.length}`} label={t("printersOnline")} />
            <Metric
              value={props.adminUnavailable || props.adminLoadError || props.adminLoading ? "—" : String(activeTokens)}
              label={t("activeTokens")}
            />
          </dl>
        </div>
      </section>

      <div className="grid items-start gap-6 lg:grid-cols-[12rem_minmax(0,1fr)]">
        <nav
          aria-label={t("sectionNavigation")}
          className="flex gap-1 overflow-x-auto rounded-xl border border-border bg-card p-1.5 lg:sticky lg:top-20 lg:flex-col"
        >
          {navItems.map((item) => {
            const Icon = item.icon;
            return (
              <a
                key={item.id}
                className="flex shrink-0 items-center gap-2 rounded-lg px-3 py-2 text-sm font-medium text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
                href={`#${item.id}`}
              >
                <Icon aria-hidden="true" className="size-4" />
                {t(item.key)}
              </a>
            );
          })}
        </nav>

        <div className="min-w-0 space-y-6">
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

          <SettingsSection
            description={t("workspaceDescription")}
            icon={Building2Icon}
            id="workspace"
            title={t("workspaceTitle")}
          >
            <div className="grid gap-px bg-border sm:grid-cols-3">
              <WorkspaceFact label={t("workspaceName")} value={props.selectedTenant.display_name} />
              <WorkspaceFact label={t("workspaceSlug")} value={props.selectedTenant.slug} mono />
              <WorkspaceFact
                label={t("workspaceCreated")}
                value={<FormattedDate value={props.selectedTenant.created_at} />}
              />
            </div>
            <div className="grid gap-4 border-t border-border p-4 sm:grid-cols-2">
              <WorkspaceLink
                description={t("agentsDescription", { connected: connectedAgents, total: props.agents.length })}
                href={dashboardSidebarHref("agents", { tenant: props.selectedTenant.id })}
                icon={ServerCogIcon}
                title={t("manageAgents")}
              />
              <WorkspaceLink
                description={t("printersDescription", { online: onlinePrinters, total: props.printers.length })}
                href={dashboardSidebarHref("devices", { tenant: props.selectedTenant.id })}
                icon={MonitorCogIcon}
                title={t("managePrinters")}
              />
            </div>
            <details className="border-t border-border px-4 py-3">
              <summary className="cursor-pointer text-sm font-medium text-muted-foreground hover:text-foreground">
                {t("technicalDetails")}
              </summary>
              <dl className="mt-3 grid gap-3 sm:grid-cols-2">
                <WorkspaceFact label={t("workspaceId")} value={props.selectedTenant.id} mono inset />
                <WorkspaceFact label={t("authProvider")} value={props.auth.provider} inset />
              </dl>
            </details>
          </SettingsSection>

          <AccessSection {...props} />
          <AccountSection auth={props.auth} />
        </div>
      </div>
    </div>
  );
}
function Metric({ value, label }: { value: string; label: string }) {
  return (
    <div className="flex min-w-20 flex-col rounded-xl border border-border bg-background/80 px-3 py-2.5 text-center backdrop-blur">
      <dt className="order-2 text-[0.68rem] font-medium text-muted-foreground">{label}</dt>
      <dd className="order-1 text-lg font-semibold tabular-nums text-foreground">{value}</dd>
    </div>
  );
}
function SettingsSection({
  id,
  icon: Icon,
  title,
  description,
  children,
}: {
  id: string;
  icon: typeof PaletteIcon;
  title: string;
  description: string;
  children: React.ReactNode;
}) {
  return (
    <section id={id} className="scroll-mt-20 overflow-hidden rounded-xl border border-border bg-card shadow-sm">
      <div className="flex items-start gap-3 border-b border-border px-4 py-4 sm:px-5">
        <span className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-muted text-foreground">
          <Icon aria-hidden="true" className="size-4" />
        </span>
        <div>
          <h3 className="font-semibold text-foreground">{title}</h3>
          <p className="mt-0.5 text-sm text-muted-foreground">{description}</p>
        </div>
      </div>
      {children}
    </section>
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
  inset = false,
}: {
  label: string;
  value: React.ReactNode;
  mono?: boolean;
  inset?: boolean;
}) {
  return (
    <div className={inset ? "rounded-lg bg-muted/50 p-3" : "bg-card p-4"}>
      <dt className="text-xs font-medium text-muted-foreground">{label}</dt>
      <dd className={`mt-1 break-all text-sm font-medium text-foreground ${mono ? "font-mono text-xs" : ""}`}>
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
    <Link className="group rounded-lg border border-border bg-muted/20 p-3 transition-colors hover:bg-muted/50" href={href}>
      <div className="flex items-center justify-between gap-3">
        <span className="flex items-center gap-2 text-sm font-medium text-foreground">
          <Icon aria-hidden="true" className="size-4 text-muted-foreground" />
          {title}
        </span>
        <ArrowUpRightIcon aria-hidden="true" className="size-4 text-muted-foreground transition-transform group-hover:-translate-y-0.5 group-hover:translate-x-0.5" />
      </div>
      <p className="mt-1.5 text-xs leading-5 text-muted-foreground">{description}</p>
    </Link>
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
    <div id="access" className="scroll-mt-20 space-y-4">
      <div className="flex items-start gap-3 px-1">
        <KeyRoundIcon aria-hidden="true" className="mt-0.5 size-5 text-muted-foreground" />
        <div>
          <h3 className="font-semibold text-foreground">{t("accessTitle")}</h3>
          <p className="mt-0.5 text-sm text-muted-foreground">{t("accessDescription")}</p>
        </div>
      </div>
      <div className="overflow-hidden rounded-xl border border-border bg-card shadow-sm">
        <TenantTokensTable tenantId={props.selectedTenant.id} tokens={props.tenantTokens} nowMs={props.nowMs} />
      </div>
      <div className="grid items-start gap-4 xl:grid-cols-2">
        <section className="rounded-xl border border-border bg-card p-5 shadow-sm">
          <div className="mb-4 flex items-start gap-3">
            <span className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-muted">
              <ServerCogIcon aria-hidden="true" className="size-4" />
            </span>
            <div>
              <h3 className="font-semibold text-foreground">{t("pairAgentTitle")}</h3>
              <p className="mt-0.5 text-sm text-muted-foreground">{t("pairAgentDescription")}</p>
            </div>
          </div>
          <CreateAgentPairingForm tenantId={props.selectedTenant.id} />
        </section>
        <section className="overflow-hidden rounded-xl border border-border bg-card shadow-sm">
          <div className="flex items-start gap-3 border-b border-border p-4">
            <span className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-muted">
              <ActivityIcon aria-hidden="true" className="size-4" />
            </span>
            <div>
              <h3 className="font-semibold text-foreground">{t("activityTitle")}</h3>
              <p className="mt-0.5 text-sm text-muted-foreground">{t("activityDescription")}</p>
            </div>
          </div>
          <TenantAuditPanel selectedTenant={props.selectedTenant} auditEvents={props.auditEvents} />
        </section>
      </div>
    </div>
  );
}

function AccountSection({ auth }: { auth: AuthMetadata }) {
  const t = useTranslations("settingsPage");
  const tAuth = useTranslations("runtime.authSource");
  const signOutHref = logoutHref(auth);

  return (
    <SettingsSection description={t("accountDescription")} icon={UserRoundIcon} id="account" title={t("accountTitle")}>
      <div className="flex flex-col gap-4 p-5 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <div className="text-sm font-medium text-foreground">{t("currentSession")}</div>
          <p className="mt-1 text-sm text-muted-foreground">
            {t("authenticatedWith", { source: formatAuthSource(auth.source, tAuth), provider: auth.provider })}
          </p>
        </div>
        {signOutHref ? (
          <Button nativeButton={false} render={<a href={signOutHref} />} size="lg" variant="outline">
            <LogOutIcon aria-hidden="true" />
            {t("signOut")}
          </Button>
        ) : (
          <span className="text-xs text-muted-foreground">{t("signOutUnavailable")}</span>
        )}
      </div>
    </SettingsSection>
  );
}
