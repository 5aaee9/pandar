import { cache } from "react";
import { cookies } from "next/headers";

import { apiHeaders, authSource } from "./api-auth";
import { authProviderConfig } from "./auth-provider";
import { TENANT_COOKIE } from "./tenant-cookie";
import type {
  FetchResult,
  MeResponse,
  Tenant,
  TenantList,
} from "./dashboard-types";

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

export async function getSelectedTenantId() {
  return (await cookies()).get(TENANT_COOKIE)?.value;
}

export function resolveSelectedTenant(
  tenantId: string | undefined,
  effectiveTenants: Tenant[],
): Tenant | null {
  if (tenantId) {
    const found = effectiveTenants.find((t) => t.id === tenantId);
    if (found) {
      return found;
    }
  }

  return effectiveTenants[0] ?? null;
}

export type DashboardPageProps = {
  searchParams?: Promise<{
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

export function firstParam(value?: string | string[]) {
  return Array.isArray(value) ? value[0] : value;
}
