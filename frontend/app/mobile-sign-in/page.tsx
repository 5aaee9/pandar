import { getTranslations } from "next-intl/server";

import { createMobileTicket } from "../actions";
import { apiHeaders, authSource } from "../api-auth";
import { authProviderConfig } from "../auth-provider";
import type { Tenant, TenantList } from "../dashboard-types";
import { LanguageSwitcher } from "../../components/language-switcher";
import { MobileTicketForm } from "./mobile-ticket-form";

const apiUrl = process.env.APP_API_URL ?? "http://localhost:8080";
const defaultRedirectUrl = "zip.iptables.pandar.android:/auth/callback";

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

export default async function MobileSignInPage({ searchParams }: PageProps) {
  const [t, auth, params] = await Promise.all([
    getTranslations("signIn"),
    authSource(),
    searchParams,
  ]);
  const provider = authProviderConfig();
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
    tenants.find((tenant) => tenant.id === requestedTenant) ??
    (!requestedTenant && tenants.length === 1 ? tenants[0] : null);
  const localNoAuthMode =
    auth.source === "none" &&
    provider.provider === "none" &&
    !readiness.externalAuthEnabled &&
    !tenantResult.error;

  return (
    <main className="min-h-screen bg-background px-4 py-5 text-slate-950 sm:px-6 lg:px-8">
      <section className="mx-auto max-w-2xl overflow-hidden rounded-md border border-slate-300 bg-white">
        <div className="flex flex-col gap-3 border-b border-slate-200 px-4 py-3 sm:flex-row sm:items-start sm:justify-between">
          <div>
            <h1 className="text-base font-semibold">{t("mobileTitle")}</h1>
            <p className="mt-0.5 text-sm text-slate-600">
              {t("mobileSubtitle")}
            </p>
          </div>
          <div className="flex items-center gap-2 self-start text-sm text-slate-600">
            <span>{t("mobileMeta")}</span>
            <LanguageSwitcher />
          </div>
        </div>

        {readiness.error ? (
          <MobileEmptyState
            title={t("externalUnavailableTitle")}
            message={t("mobileReadinessCheckMessage")}
            statusLabel={t("actionRequired")}
            detail={readiness.error}
            detailLabel={t("developerDetails")}
            actions={[
              { href: "/#diagnostics", label: t("openDiagnostics") },
              { href: "/", label: t("returnDashboard") },
            ]}
          />
        ) : !readiness.externalAuthEnabled && !localNoAuthMode ? (
          <MobileEmptyState
            title={t("externalUnavailableTitle")}
            message={t("mobileExternalConfigMessage")}
            statusLabel={t("actionRequired")}
            actions={[
              { href: "/#admin", label: t("openAdmin") },
              { href: "/", label: t("returnDashboard") },
            ]}
          />
        ) : auth.source === "none" && !localNoAuthMode ? (
          <MobileEmptyState
            title={t("authUnavailableTitle")}
            message={t("mobileAuthMessage")}
            statusLabel={t("actionRequired")}
            actions={[
              ...(provider.signInUrl
                ? [{ href: provider.signInUrl, label: t("signIn") }]
                : []),
              { href: "/", label: t("returnDashboard") },
            ]}
          />
        ) : tenantResult.error ? (
          <MobileEmptyState
            title={t("tenantLookupTitle")}
            message={t("tenantLookupMessage")}
            statusLabel={t("actionRequired")}
            detail={tenantResult.error}
            detailLabel={t("developerDetails")}
            actions={[
              { href: "/mobile-sign-in", label: t("retry") },
              { href: "/", label: t("returnDashboard") },
            ]}
          />
        ) : tenants.length === 0 ? (
          <MobileEmptyState
            title={t("noTenantsTitle")}
            message={t("mobileNoTenantsMessage")}
            statusLabel={t("actionRequired")}
            actions={[
              { href: "/#admin", label: t("openAdmin") },
              { href: "/", label: t("returnDashboard") },
            ]}
          />
        ) : !selectedTenant ? (
          <div className="grid gap-3 px-4 py-4">
            <div className="text-sm font-semibold text-slate-950">
              {t("selectTenant")}
            </div>
            <form className="grid gap-3" action="/mobile-sign-in">
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
                  className="h-9 rounded-md border border-slate-300 bg-white px-2 text-sm text-slate-950 hover:border-slate-400"
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
                className="h-9 rounded-md bg-primary px-3 text-sm font-medium text-primary-foreground hover:bg-primary/80"
                type="submit"
              >
                {t("continue")}
              </button>
            </form>
          </div>
        ) : (
          <MobileTicketForm
            action={createMobileTicket}
            autoSelectedTenant={!requestedTenant && tenants.length === 1}
            redirectUrl={redirectUrl ?? defaultRedirectUrl}
            selectedTenant={selectedTenant}
          />
        )}
      </section>
    </main>
  );
}

function MobileEmptyState({
  actions,
  detail,
  detailLabel,
  message,
  statusLabel,
  title,
}: {
  actions: { href: string; label: string }[];
  detail?: string;
  detailLabel?: string;
  message: string;
  statusLabel: string;
  title: string;
}) {
  return (
    <div className="grid gap-5 px-4 py-10 text-center sm:px-6">
      <div>
        <div className="mb-2 inline-flex items-center rounded-md bg-red-50 px-2.5 py-1 text-xs font-semibold text-red-700">
          {statusLabel}
        </div>
        <h2 className="text-2xl font-semibold leading-8 text-slate-950">
          {title}
        </h2>
        <p className="mx-auto mt-2 max-w-md text-sm leading-6 text-slate-600">
          {message}
        </p>
      </div>
      <div className="flex flex-wrap justify-center gap-2">
        {actions.map((action, index) => (
          <a
            className={
              index === 0
                ? "inline-flex min-h-10 items-center rounded-md bg-primary px-3.5 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/80"
                : "inline-flex min-h-10 items-center rounded-md border border-slate-300 px-3.5 py-2 text-sm font-medium text-slate-700 hover:bg-slate-50"
            }
            href={action.href}
            key={`${action.href}-${action.label}`}
          >
            {action.label}
          </a>
        ))}
      </div>
      {detail ? (
        <details className="mx-auto w-full max-w-lg text-left text-xs text-slate-600">
          <summary className="cursor-pointer text-center font-medium text-slate-700">
            {detailLabel}
          </summary>
          <div className="mt-2 break-words rounded-md bg-slate-100 px-3 py-2 font-mono leading-5 text-slate-700">
            {detail}
          </div>
        </details>
      ) : null}
    </div>
  );
}
