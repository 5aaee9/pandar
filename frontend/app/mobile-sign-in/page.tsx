import type { ReactNode } from "react";
import { getTranslations } from "next-intl/server";

import { createMobileTicket } from "../actions";
import { authSource } from "../api-auth";
import { authProviderConfig } from "../auth-provider";
import { firstParam, getSignInTenantContext } from "../dashboard-data";
import {
  TenantTicketSelection,
  TicketSignInEmptyState,
  TicketSignInFrame,
} from "../ticket-sign-in-ui";
import { MobileTicketForm } from "./mobile-ticket-form";

const defaultRedirectUrl = "zip.iptables.pandar.android://auth/callback";

type PageProps = {
  searchParams?: Promise<{
    tenant?: string | string[];
    redirect_url?: string | string[];
    code_challenge?: string | string[];
    state?: string | string[];
  }>;
};

export default async function MobileSignInPage({ searchParams }: PageProps) {
  const [t, auth, params] = await Promise.all([
    getTranslations("signIn"),
    authSource(),
    searchParams,
  ]);
  const provider = authProviderConfig();
  const tenantResult = await getSignInTenantContext(provider.provider);
  const requestedTenant = firstParam(params?.tenant);
  const redirectUrl = firstParam(params?.redirect_url) ?? defaultRedirectUrl;
  const codeChallenge = firstParam(params?.code_challenge) ?? "";
  const state = firstParam(params?.state) ?? "";
  const selectedTenant =
    tenantResult.tenants.find((tenant) => tenant.id === requestedTenant) ??
    (!requestedTenant && tenantResult.tenants.length === 1
      ? tenantResult.tenants[0]
      : null);
  const localNoAuthMode =
    auth.source === "none" &&
    provider.provider === "none" &&
    !tenantResult.error;

  let content: ReactNode;
  if (auth.source === "none" && !localNoAuthMode) {
    content = (
      <TicketSignInEmptyState
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
          { href: "/mobile-sign-in", label: t("retry") },
          { href: "/", label: t("returnDashboard") },
        ]}
      />
    );
  } else if (tenantResult.tenants.length === 0) {
    content = (
      <TicketSignInEmptyState
        title={t("noTenantsTitle")}
        message={t("mobileNoTenantsMessage")}
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
        action="/mobile-sign-in"
        continueLabel={t("continue")}
        hiddenFields={{
          redirect_url: redirectUrl,
          code_challenge: codeChallenge,
          state,
        }}
        selectionTitle={t("selectTenant")}
        tenantLabel={t("tenant")}
        tenants={tenantResult.tenants}
      />
    );
  } else {
    content = (
      <MobileTicketForm
        action={createMobileTicket}
        autoSelectedTenant={
          !requestedTenant && tenantResult.tenants.length === 1
        }
        codeChallenge={codeChallenge}
        redirectUrl={redirectUrl}
        selectedTenant={selectedTenant}
        state={state}
      />
    );
  }

  return (
    <TicketSignInFrame
      title={t("mobileTitle")}
      subtitle={t("mobileSubtitle")}
      meta={t("mobileMeta")}
    >
      {content}
    </TicketSignInFrame>
  );
}
