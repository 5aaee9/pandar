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
  const auth = await authSource();
  const authProvider = authProviderConfig();
  return {
    ...auth,
    signInUrl: authProvider.signInUrl,
    signOutUrl: authProvider.signOutUrl,
  };
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
  { searchParams }: DashboardPageProps,
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
      ? Promise.resolve<FetchResult<TenantList>>({ data: { tenants: [] }, error: null })
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
  const externalTenants = meResult.data?.tenants.map((tenant) => ({
    id: tenant.tenant_id,
    slug: tenant.tenant_slug,
    display_name: tenant.display_name,
    created_at: "",
  })) ?? [];
  const tenants = auth.provider === "none" ? (tenantsResult.data?.tenants ?? []) : externalTenants;
  const params = await searchParams;
  const requestedTenant = firstParam(params?.tenant);
  const requestedCommand = firstParam(params?.command);
  const selectedTenant = configuredTenantId
    ? { id: configuredTenantId, slug: configuredTenantId, display_name: configuredTenantId, created_at: "" }
    : (tenants.find((tenant) => tenant.id === requestedTenant) ?? tenants[0] ?? null);
  const selectedTenantSegment = selectedTenant ? apiIdSegment(selectedTenant.id, "tenant_id") : null;
  const loadFleet = view !== "users";
  const loadJobs = view === "devices" || view === "jobs";
  const loadUsers = view === "users";
  const loadJoinLinks = view === "users";
  const loadSettingsAdmin = view === "settings";
  const [
    _printersResult,
    _agentsResult,
    _jobsResult,
    _usersResult,
    _tenantTokensResult,
    _joinLinksResult,
    _auditEventsResult,
  ] = await Promise.all([
    selectedTenantSegment && loadFleet ? fetchJson<PrinterList>(`/api/v1/tenants/${selectedTenantSegment}/printers`, "Printers") : null,
    selectedTenantSegment && loadFleet ? fetchJson<AgentList>(`/api/v1/tenants/${selectedTenantSegment}/agents`, "Agents") : null,
    selectedTenantSegment && loadJobs ? fetchJson<JobList>(`/api/v1/tenants/${selectedTenantSegment}/jobs`, "Jobs") : null,
    selectedTenantSegment && loadUsers ? fetchJson<UserList>(`/api/v1/tenants/${selectedTenantSegment}/users`, "Users") : null,
    selectedTenantSegment && loadSettingsAdmin ? fetchJson<TenantTokenList>(`/api/v1/tenants/${selectedTenantSegment}/tenant-tokens`, "Tenant tokens") : null,
    selectedTenantSegment && loadJoinLinks ? fetchJson<JoinLinkList>(`/api/v1/tenants/${selectedTenantSegment}/join-links`, "Join links") : null,
    selectedTenantSegment && loadSettingsAdmin ? fetchJson<AuditEventList>(`/api/v1/tenants/${selectedTenantSegment}/audit-events?limit=20`, "Audit events") : null,
  ]);
  await (selectedTenantSegment && requestedCommand && view === "agents"
    ? fetchJson<Command>(`/api/v1/tenants/${selectedTenantSegment}/commands/${apiIdSegment(requestedCommand, "command_id")}`, "Command")
    : Promise.resolve(null));

  return meResult.data && tenants.length === 0 ? <OnboardingPanel me={meResult.data} /> : null;
}

export function firstParam(value?: string | string[]) {
  return Array.isArray(value) ? value[0] : value;
}

export async function loadDevicesRoute(tenantId: string) {
  const [printersResult, agentsResult, jobsResult] = await Promise.all([
    fetchJson<PrinterList>(`/api/v1/tenants/${tenantId}/printers`, "Printers"),
    fetchJson<AgentList>(`/api/v1/tenants/${tenantId}/agents`, "Agents"),
    fetchJson<JobList>(`/api/v1/tenants/${tenantId}/jobs`, "Jobs"),
  ]);
  return {
    printers: printersResult.data?.printers ?? [],
    agents: agentsResult.data?.agents ?? [],
    jobs: jobsResult.data?.jobs ?? [],
    error: printersResult.error ?? agentsResult.error ?? jobsResult.error,
  };
}

export async function loadJobsRoute(tenantId: string) {
  const [jobsResult, printersResult, agentsResult] = await Promise.all([
    fetchJson<JobList>(`/api/v1/tenants/${tenantId}/jobs`, "Jobs"),
    fetchJson<PrinterList>(`/api/v1/tenants/${tenantId}/printers`, "Printers"),
    fetchJson<AgentList>(`/api/v1/tenants/${tenantId}/agents`, "Agents"),
  ]);
  return {
    jobs: jobsResult.data?.jobs ?? [],
    printers: printersResult.data?.printers ?? [],
    agents: agentsResult.data?.agents ?? [],
    error: jobsResult.error ?? printersResult.error ?? agentsResult.error,
  };
}

export async function loadAgentsRoute(tenantId: string, commandId: string | null) {
  const [agentsResult, printersResult, commandResult] = await Promise.all([
    fetchJson<AgentList>(`/api/v1/tenants/${tenantId}/agents`, "Agents"),
    fetchJson<PrinterList>(`/api/v1/tenants/${tenantId}/printers`, "Printers"),
    commandId
      ? fetchJson<Command>(`/api/v1/tenants/${tenantId}/commands/${commandId}`, "Command")
      : Promise.resolve({ data: null, error: null }),
  ]);
  return {
    agents: agentsResult.data?.agents ?? [],
    printers: printersResult.data?.printers ?? [],
    command: commandResult.data ?? null,
    commandData: commandResult.data ? parseCommandResult(commandResult.data) : null,
    error: agentsResult.error ?? printersResult.error ?? commandResult.error,
  };
}

export async function loadUsersRoute(tenantId: string) {
  const [usersResult, joinLinksResult] = await Promise.all([
    fetchJson<UserList>(`/api/v1/tenants/${tenantId}/users`, "Users"),
    fetchJson<JoinLinkList>(`/api/v1/tenants/${tenantId}/join-links`, "Join links"),
  ]);
  return {
    users: usersResult.data?.users ?? [],
    identities: usersResult.data?.identities ?? [],
    joinLinks: joinLinksResult.data?.join_links ?? [],
    adminError: usersResult.error ?? joinLinksResult.error,
  };
}

export async function loadSettingsRoute(tenantId: string) {
  const [tenantTokensResult, agentsResult, printersResult, auditEventsResult] = await Promise.all([
    fetchJson<TenantTokenList>(`/api/v1/tenants/${tenantId}/tenant-tokens`, "Tenant tokens"),
    fetchJson<AgentList>(`/api/v1/tenants/${tenantId}/agents`, "Agents"),
    fetchJson<PrinterList>(`/api/v1/tenants/${tenantId}/printers`, "Printers"),
    fetchJson<AuditEventList>(`/api/v1/tenants/${tenantId}/audit-events?limit=20`, "Audit events"),
  ]);
  return {
    tenantTokens: tenantTokensResult.data?.tenant_tokens ?? [],
    agents: agentsResult.data?.agents ?? [],
    printers: printersResult.data?.printers ?? [],
    auditEvents: auditEventsResult.data?.audit_events ?? [],
    adminError: tenantTokensResult.error ?? auditEventsResult.error,
  };
}
