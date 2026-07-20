import { cache } from "react";
import { cookies } from "next/headers";
import { redirect } from "next/navigation";

import { apiHeaders, authSource } from "./api-auth";
import { dashboardAuthRedirectTarget } from "./auth-redirect";
import { authProviderConfig } from "./auth-provider";
import { apiIdSegment } from "./api-path";
import { parseCommandResult } from "./command-result-parser";
import type {
  AgentList,
  AuditEventList,
  Command,
  FetchResult,
  JoinLinkList,
  JobList,
  MeResponse,
  PrinterList,
  Tenant,
  TenantList,
  TenantTokenList,
  UserList,
} from "./dashboard-types";
import { DashboardRuntime } from "./dashboard-runtime";
import type { DashboardView } from "./dashboard-shell";
import { OnboardingPanel } from "./onboarding-panel";

const apiUrl = process.env.APP_API_URL ?? "http://localhost:8080";
const configuredTenantId = process.env.APP_TENANT_ID;

export const getTenantsForRequest = cache(async () => {
  const auth = await authSource();
  const useExternalOnboarding = auth.provider !== "none" && !configuredTenantId;

  if (configuredTenantId || useExternalOnboarding) {
    return { tenants: [], error: null };
  }

  const result = await fetchJson<TenantList>("/api/v1/tenants", "Tenants");
  return { tenants: result.data?.tenants ?? [], error: result.error };
});

export const getIdentityForRequest = cache(async () => {
  const auth = await authSource();
  if (auth.provider === "none") {
    return { me: null, error: null, status: null };
  }

  const result = await fetchJson<MeResponse>("/api/v1/me", "Current identity");
  return { me: result.data, error: result.error, status: result.status };
});

export const getMembershipForRequest = cache(async (tenantId: string) => {
  const { me, error: identityError } = await getIdentityForRequest();
  if (identityError) {
    return { role: null, error: identityError };
  }
  const membership = me?.tenants.find((t) => t.tenant_id === tenantId);
  return { role: membership?.role ?? null, error: null };
});

export const getAuthForRequest = cache(async () => {
  return await authSource();
});

export function resolveEffectiveTenants(
  tenants: Tenant[],
  identity: MeResponse | null,
  configuredTenantId: string | undefined,
  authProvider: string,
): Tenant[] {
  if (configuredTenantId) {
    return [{
      id: configuredTenantId,
      slug: configuredTenantId,
      display_name: configuredTenantId,
      created_at: "",
    }];
  }

  if (authProvider === "none") {
    return tenants;
  }

  const externalTenants = identity?.tenants.map((tenant) => ({
    id: tenant.tenant_id,
    slug: tenant.tenant_slug,
    display_name: tenant.display_name,
    created_at: "",
  })) ?? [];

  if (externalTenants.length > 0) {
    return externalTenants;
  }

  return tenants;
}

export function resolveSelectedTenant(
  searchParams: { tenant?: string | string[] },
  effectiveTenants: Tenant[],
): Tenant | null {
  const tenantParam = Array.isArray(searchParams.tenant)
    ? searchParams.tenant[0]
    : searchParams.tenant;

  if (tenantParam) {
    const found = effectiveTenants.find((t) => t.id === tenantParam);
    if (found) {
      return found;
    }
  }

  return effectiveTenants[0] ?? null;
}

export type DashboardPageProps = {
  searchParams?: Promise<{
    tenant?: string | string[];
    command?: string | string[];
    status?: string | string[];
  }>;
  sidebarDefaultOpen?: boolean;
};

export async function dashboardSidebarDefaultOpen() {
  return (await cookies()).get("sidebar_state")?.value !== "false";
}

async function fetchJson<T>(
  path: string,
  label: string,
): Promise<FetchResult<T>> {
  try {
    const response = await fetch(`${apiUrl}${path}`, {
      cache: "no-store",
      headers: await apiHeaders(),
    });
    if (!response.ok) {
      return {
        data: null,
        error: `${label} returned ${response.status}`,
        status: response.status,
      };
    }

    return {
      data: (await response.json()) as T,
      error: null,
      status: response.status,
    };
  } catch (error) {
    return {
      data: null,
      error: `${label} failed: ${error instanceof Error ? error.message : "unknown error"}`,
    };
  }
}

export async function renderDashboardView(
  view: DashboardView,
  { searchParams, sidebarDefaultOpen }: DashboardPageProps,
) {
  const auth = await authSource();
  const authProvider = authProviderConfig();
  const initialRedirect = dashboardAuthRedirectTarget({
    source: auth.source,
    provider: authProvider,
  });
  if (initialRedirect) {
    redirect(initialRedirect);
  }

  const useExternalOnboarding = auth.provider !== "none" && !configuredTenantId;
  const [tenantsResult, meResult] = await Promise.all([
    configuredTenantId || useExternalOnboarding
      ? Promise.resolve<FetchResult<TenantList>>({
          data: { tenants: [] },
          error: null,
        })
      : fetchJson<TenantList>("/api/v1/tenants", "Tenants"),
    auth.provider === "none"
      ? Promise.resolve<FetchResult<MeResponse>>({ data: null, error: null })
      : fetchJson<MeResponse>("/api/v1/me", "Current identity"),
  ]);
  const meRedirect = dashboardAuthRedirectTarget({
    source: auth.source,
    provider: authProvider,
    meStatus: meResult.status,
  });
  if (meRedirect) {
    redirect(meRedirect);
  }

  const externalTenants =
    meResult.data?.tenants.map((tenant) => ({
      id: tenant.tenant_id,
      slug: tenant.tenant_slug,
      display_name: tenant.display_name,
      created_at: "",
    })) ?? [];
  const tenants =
    auth.provider === "none"
      ? (tenantsResult.data?.tenants ?? [])
      : externalTenants;
  const params = await searchParams;
  const requestedTenant = firstParam(params?.tenant);
  const requestedCommand = firstParam(params?.command);
  const actionStatus = firstParam(params?.status);
  const selectedTenant = configuredTenantId
    ? {
        id: configuredTenantId,
        slug: configuredTenantId,
        display_name: configuredTenantId,
        created_at: "",
      }
    : (tenants.find((tenant) => tenant.id === requestedTenant) ??
      tenants[0] ??
      null);
  const selectedTenantSegment = selectedTenant
    ? apiIdSegment(selectedTenant.id, "tenant_id")
    : null;
  const loadFleet = view !== "users";
  const loadJobs = view === "devices" || view === "jobs";
  const loadUsers = view === "users";
  const loadJoinLinks = view === "users";
  const loadSettingsAdmin = view === "settings";
  const [
    printersResult,
    agentsResult,
    jobsResult,
    usersResult,
    tenantTokensResult,
    joinLinksResult,
    auditEventsResult,
  ] = await Promise.all([
    selectedTenantSegment && loadFleet
      ? fetchJson<PrinterList>(
          `/api/v1/tenants/${selectedTenantSegment}/printers`,
          "Printers",
        )
      : null,
    selectedTenantSegment && loadFleet
      ? fetchJson<AgentList>(
          `/api/v1/tenants/${selectedTenantSegment}/agents`,
          "Agents",
        )
      : null,
    selectedTenantSegment && loadJobs
      ? fetchJson<JobList>(
          `/api/v1/tenants/${selectedTenantSegment}/jobs`,
          "Jobs",
        )
      : null,
    selectedTenantSegment && loadUsers
      ? fetchJson<UserList>(
          `/api/v1/tenants/${selectedTenantSegment}/users`,
          "Users",
        )
      : null,
    selectedTenantSegment && loadSettingsAdmin
      ? fetchJson<TenantTokenList>(
          `/api/v1/tenants/${selectedTenantSegment}/tenant-tokens`,
          "Tenant tokens",
        )
      : null,
    selectedTenantSegment && loadJoinLinks
      ? fetchJson<JoinLinkList>(
          `/api/v1/tenants/${selectedTenantSegment}/join-links`,
          "Join links",
        )
      : null,
    selectedTenantSegment && loadSettingsAdmin
      ? fetchJson<AuditEventList>(
          `/api/v1/tenants/${selectedTenantSegment}/audit-events?limit=20`,
          "Audit events",
        )
      : null,
  ]);
  const commandResult =
    selectedTenantSegment && requestedCommand && view === "agents"
      ? await fetchJson<Command>(
          `/api/v1/tenants/${selectedTenantSegment}/commands/${apiIdSegment(requestedCommand, "command_id")}`,
          "Command",
        )
      : null;
  const printers = printersResult?.data?.printers ?? [];
  const agents = agentsResult?.data?.agents ?? [];
  const jobs = jobsResult?.data?.jobs ?? [];
  const users = usersResult?.data?.users ?? [];
  const userIdentities = usersResult?.data?.identities ?? [];
  const tenantTokens = tenantTokensResult?.data?.tenant_tokens ?? [];
  const joinLinks = joinLinksResult?.data?.join_links ?? [];
  const auditEvents = auditEventsResult?.data?.audit_events ?? [];
  const selectedMembership = meResult.data?.tenants.find(
    (tenant) => tenant.tenant_id === selectedTenant?.id,
  );
  const lacksAdminRole =
    auth.provider !== "none" && selectedMembership?.role !== "tenant_admin";
  const adminUnavailable =
    lacksAdminRole ||
    Boolean(
      usersResult?.error ||
      tenantTokensResult?.error ||
      joinLinksResult?.error ||
      auditEventsResult?.error,
    );
  const adminLoadError =
    !lacksAdminRole &&
    Boolean(
      usersResult?.error ||
      tenantTokensResult?.error ||
      joinLinksResult?.error ||
      auditEventsResult?.error,
    );
  const canManageJobs = selectedMembership?.role !== "viewer";
  const selectedCommand = commandResult?.data ?? null;
  const commandData = parseCommandResult(selectedCommand);
  const errors = [
    tenantsResult.error,
    meResult.error && tenants.length === 0 ? meResult.error : null,
    printersResult?.error,
    agentsResult?.error,
    jobsResult?.error,
    commandResult?.error,
  ].filter((error): error is string => Boolean(error));

  return meResult.data && tenants.length === 0 ? (
    <OnboardingPanel me={meResult.data} />
  ) : (
    <DashboardRuntime
      apiUrl={apiUrl}
      configuredTenantId={configuredTenantId}
      view={view}
      tenants={tenants}
      selectedTenant={selectedTenant}
      initialPrinters={printers}
      agents={agents}
      initialJobs={jobs}
      users={users}
      userIdentities={userIdentities}
      tenantTokens={tenantTokens}
      joinLinks={joinLinks}
      auditEvents={auditEvents}
      adminUnavailable={adminUnavailable}
      adminLoadError={adminLoadError}
      canManageJobs={canManageJobs}
      actionStatus={actionStatus}
      selectedCommand={selectedCommand}
      selectedCommandId={requestedCommand}
      commandData={commandData}
      errors={errors}
      sidebarDefaultOpen={sidebarDefaultOpen}
      auth={{
        source: auth.source,
        cookieName: auth.cookieName,
        provider: auth.provider,
        signInUrl: authProvider.signInUrl,
        signOutUrl: authProvider.signOutUrl,
      }}
    />
  );
}

export function firstParam(value?: string | string[]) {
  return Array.isArray(value) ? value[0] : value;
}
