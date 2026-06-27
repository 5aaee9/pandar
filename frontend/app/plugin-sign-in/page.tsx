import { getTranslations } from "next-intl/server";

import { createPluginTicket } from "../actions";
import { apiHeaders, authSource } from "../api-auth";
import type { Tenant, TenantList } from "../dashboard-types";
import { EmptyState, SectionHeader } from "../dashboard-ui";
import { PluginTicketForm } from "./plugin-ticket-form";

const apiUrl = process.env.APP_API_URL ?? "http://localhost:8080";
const defaultRedirectUrl = "http://localhost:32200/callback";

type PageProps = {
  searchParams?: Promise<{
    tenant?: string | string[];
    redirect_url?: string | string[];
  }>;
};

type TenantFetchResult = {
  tenants: Tenant[];
  error: string | null;
};

type ReadinessResult = {
  externalAuthEnabled: boolean;
  error: string | null;
};

type ReadinessResponse = {
  checks?: {
    external_auth?: {
      ready?: boolean;
      detail?: string;
    };
  };
};

async function fetchTenants(): Promise<TenantFetchResult> {
  try {
    const response = await fetch(`${apiUrl}/api/v1/tenants`, {
      cache: "no-store",
      headers: await apiHeaders(),
    });
    if (!response.ok) {
      return {
        tenants: [],
        error: `Tenant lookup returned ${response.status}`,
      };
    }
    const body = (await response.json()) as TenantList;
    return { tenants: body.tenants, error: null };
  } catch (error) {
    return {
      tenants: [],
      error: `Tenant lookup failed: ${error instanceof Error ? error.message : "unknown error"}`,
    };
  }
}

async function fetchExternalAuthStatus(): Promise<ReadinessResult> {
  try {
    const response = await fetch(`${apiUrl}/readyz`, { cache: "no-store" });
    if (!response.ok) {
      return {
        externalAuthEnabled: false,
        error: `Readiness check returned ${response.status}`,
      };
    }
    const body = (await response.json()) as ReadinessResponse;
    const externalAuth = body.checks?.external_auth;
    return {
      externalAuthEnabled:
        externalAuth?.ready === true && externalAuth.detail !== "disabled",
      error: null,
    };
  } catch (error) {
    return {
      externalAuthEnabled: false,
      error: `Readiness check failed: ${error instanceof Error ? error.message : "unknown error"}`,
    };
  }
}

export default async function PluginSignInPage({ searchParams }: PageProps) {
  const t = await getTranslations("signIn");
  const auth = await authSource();
  const params = await searchParams;
  const [tenantResult, readiness] = await Promise.all([
    fetchTenants(),
    fetchExternalAuthStatus(),
  ]);
  const tenants = tenantResult.tenants;
  const requestedTenant = Array.isArray(params?.tenant)
    ? params.tenant[0]
    : params?.tenant;
  const redirectUrl = Array.isArray(params?.redirect_url)
    ? params.redirect_url[0]
    : params?.redirect_url;
  const selectedTenant =
    tenants.find((tenant) => tenant.id === requestedTenant) ?? null;

  return (
    <main className="min-h-screen bg-slate-100 px-4 py-5 text-slate-950 sm:px-6 lg:px-8">
      <section className="mx-auto max-w-2xl overflow-hidden rounded-md border border-slate-300 bg-white">
        <SectionHeader
          title={t("title")}
          subtitle={t("subtitle")}
          meta={t("meta")}
        />

        {readiness.error ? (
          <EmptyState
            title={t("externalUnavailableTitle")}
            message={readiness.error}
          />
        ) : !readiness.externalAuthEnabled ? (
          <EmptyState
            title={t("externalUnavailableTitle")}
            message={t("externalConfigMessage")}
          />
        ) : auth.source === "none" ? (
          <EmptyState
            title={t("authUnavailableTitle")}
            message={t("authMessage")}
          />
        ) : tenantResult.error ? (
          <EmptyState
            title={t("tenantLookupTitle")}
            message={tenantResult.error}
          />
        ) : tenants.length === 0 ? (
          <EmptyState
            title={t("noTenantsTitle")}
            message={t("noTenantsMessage")}
          />
        ) : !selectedTenant ? (
          <div className="grid gap-3 px-4 py-4">
            <div className="text-sm font-semibold text-slate-950">
              {t("selectTenant")}
            </div>
            <form className="grid gap-3" action="/plugin-sign-in">
              <input
                name="redirect_url"
                type="hidden"
                value={redirectUrl ?? defaultRedirectUrl}
              />
              <label className="grid gap-1 text-sm">
                <span className="text-xs font-medium text-slate-500">
                  {t("tenant")}
                </span>
                <select
                  className="h-9 rounded-md border border-slate-300 bg-white px-2 text-sm text-slate-950"
                  name="tenant"
                >
                  {tenants.map((tenant) => (
                    <option key={tenant.id} value={tenant.id}>
                      {tenant.display_name}
                    </option>
                  ))}
                </select>
              </label>
              <button
                className="h-9 rounded-md bg-cyan-700 px-3 text-sm font-medium text-white"
                type="submit"
              >
                {t("continue")}
              </button>
            </form>
          </div>
        ) : (
          <PluginTicketForm
            action={createPluginTicket}
            redirectUrl={redirectUrl ?? defaultRedirectUrl}
            selectedTenant={selectedTenant}
          />
        )}
      </section>
    </main>
  );
}
