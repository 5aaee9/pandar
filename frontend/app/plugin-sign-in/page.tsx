import type { ReactNode } from "react";
import { getTranslations } from "next-intl/server";

import { createPluginTicket } from "../actions";
import { authSource } from "../api-auth";
import { authProviderConfig } from "../auth-provider";
import { firstParam, getSignInTenantContext } from "../dashboard-data";
import {
  TenantTicketSelection,
  TicketSignInEmptyState,
  TicketSignInFrame,
} from "../ticket-sign-in-ui";
import { pluginAuthSignInUrl, pluginSignInReturnTarget } from "./auth-return";
import { fetchExternalAuthStatus } from "./external-auth-status";
import { PluginTicketForm } from "./plugin-ticket-form";

const defaultRedirectUrl = "http://localhost:32200/callback";

type PageProps = {
  searchParams?: Promise<{
    tenant?: string | string[];
    redirect_url?: string | string[];
  }>;
};

export default async function PluginSignInPage({ searchParams }: PageProps) {
  const [t, auth, params] = await Promise.all([
    getTranslations("signIn"),
    authSource(),
    searchParams,
  ]);
  const provider = authProviderConfig();
  const [tenantResult, readiness] = await Promise.all([
    getSignInTenantContext(provider.provider),
    fetchExternalAuthStatus(),
  ]);
  const requestedTenant = firstParam(params?.tenant);
  const callbackUrl = firstParam(params?.redirect_url) ?? defaultRedirectUrl;
  const selectedTenant =
    tenantResult.tenants.find((tenant) => tenant.id === requestedTenant) ??
    (!requestedTenant && tenantResult.tenants.length === 1
      ? tenantResult.tenants[0]
      : null);
  const localNoAuthMode =
    auth.source === "none" &&
    provider.provider === "none" &&
    !readiness.externalAuthEnabled &&
    !tenantResult.error;
  const authSignInUrl = pluginAuthSignInUrl(
    provider,
    pluginSignInReturnTarget(requestedTenant, callbackUrl),
  );

  let content: ReactNode;
  if (readiness.error) {
    content = (
      <TicketSignInEmptyState
        title={t("externalUnavailableTitle")}
        message={t("readinessCheckMessage")}
        statusLabel={t("actionRequired")}
        detail={readiness.error}
        detailLabel={t("developerDetails")}
        actions={[
          { href: "/#diagnostics", label: t("openDiagnostics") },
          { href: "/", label: t("returnDashboard") },
        ]}
      />
    );
  } else if (!readiness.externalAuthEnabled && !localNoAuthMode) {
    content = (
      <TicketSignInEmptyState
        title={t("externalUnavailableTitle")}
        message={t("externalConfigMessage")}
        statusLabel={t("actionRequired")}
        actions={[
          { href: "/#admin", label: t("openAdmin") },
          { href: "/", label: t("returnDashboard") },
        ]}
      />
    );
  } else if (auth.source === "none" && !localNoAuthMode) {
    content = (
      <TicketSignInEmptyState
        title={t("authUnavailableTitle")}
        message={t("authMessage")}
        statusLabel={t("actionRequired")}
        actions={[
          ...(authSignInUrl
            ? [{ href: authSignInUrl, label: t("signIn") }]
            : []),
          { href: "/", label: t("returnDashboard") },
        ]}
      />
    );
  } else if (tenantResult.error) {
    content = (
      <TicketSignInEmptyState
        title={t("tenantLookupTitle")}
        message={t("tenantLookupMessage")}
        statusLabel={t("actionRequired")}
        detail={tenantResult.error}
        detailLabel={t("developerDetails")}
        actions={[
          { href: "/plugin-sign-in", label: t("retry") },
          { href: "/", label: t("returnDashboard") },
        ]}
      />
    );
  } else if (tenantResult.tenants.length === 0) {
    content = (
      <TicketSignInEmptyState
        title={t("noTenantsTitle")}
        message={t("noTenantsMessage")}
        statusLabel={t("actionRequired")}
        actions={[
          { href: "/#admin", label: t("openAdmin") },
          { href: "/", label: t("returnDashboard") },
        ]}
      />
    );
  } else if (!selectedTenant) {
    content = (
      <TenantTicketSelection
        action="/plugin-sign-in"
        continueLabel={t("continue")}
        hiddenFields={{ redirect_url: callbackUrl }}
        selectionTitle={t("selectTenant")}
        tenantLabel={t("tenant")}
        tenants={tenantResult.tenants}
      />
    );
  } else {
    content = (
      <PluginTicketForm
        action={createPluginTicket}
        autoSelectedTenant={
          !requestedTenant && tenantResult.tenants.length === 1
        }
        redirectUrl={callbackUrl}
        selectedTenant={selectedTenant}
      />
    );
  }

  return (
    <TicketSignInFrame
      title={t("title")}
      subtitle={t("subtitle")}
      meta={t("meta")}
    >
      {content}
    </TicketSignInFrame>
  );
}
