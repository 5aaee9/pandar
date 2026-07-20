import { useTranslations } from "next-intl";

import { FormattedDate } from "../components/formatted-date";
import type { Agent, AuthMetadata, Tenant } from "./dashboard-types";
import { DetailGroup, DetailLine, SectionHeader } from "./dashboard-ui";
import {
  formatAuthSource,
} from "./dashboard-runtime-helpers";

export function TenantSettingsStatic({
  tenant,
  agents,
  auth,
  livePrintersSlot,
}: {
  tenant: Tenant | null;
  agents: Agent[];
  auth: AuthMetadata;
  livePrintersSlot: React.ReactNode;
}) {
  const t = useTranslations("tenantSettings");
  const tAuth = useTranslations("runtime.authSource");
  const tenantId = tenant?.id ?? "{tenant_id}";

  return (
    <section className="overflow-hidden rounded-md border border-border bg-muted/30">
      <SectionHeader
        title={t("title")}
        subtitle={tenant ? t("subtitleTenant", { name: tenant.display_name }) : t("subtitleNone")}
        meta={t("meta")}
      />
      <div className="grid gap-4 px-4 py-4 text-sm lg:grid-cols-3">
        <DetailGroup title={t("groupTenant")}>
          <DetailLine label={t("id")} value={tenant?.id ?? "-"} mono />
          <DetailLine label={t("slug")} value={tenant?.slug ?? "-"} />
          <DetailLine label={t("created")} value={tenant ? <FormattedDate value={tenant.created_at} /> : "-"} />
        </DetailGroup>
        <DetailGroup title={t("groupAuth")}>
          <DetailLine label={t("source")} value={formatAuthSource(auth.source, tAuth)} />
          <DetailLine label={t("provider")} value={auth.provider} />
          <DetailLine label={t("cookieName")} value={auth.cookieName} mono />
          <DetailLine label={t("secretValues")} value={t("hidden")} />
        </DetailGroup>
        <DetailGroup title={t("groupOps")}>
          <DetailLine label={t("diagnosticsLabel")} value={t("diagnosticsValue")} />
        </DetailGroup>
      </div>
      <details className="border-t border-border px-4 py-2">
        <summary className="cursor-pointer select-none text-xs font-medium text-muted-foreground transition-colors duration-150 ease-out hover:text-foreground">
          {t("developerRef")}
        </summary>
        <div className="mt-2 grid gap-1 text-sm">
          <DetailLine label={t("agentPairing")} value={`/api/v1/tenants/${tenantId}/agent-pairings`} mono />
          <DetailLine label={t("apiTokens")} value={`/api/v1/tenants/${tenantId}/users/{user_id}/api-tokens`} mono />
        </div>
      </details>
      <div className="border-t border-border px-4 py-3">
        <div className="text-xs font-medium text-muted-foreground">{t("linkedAgents")}</div>
        {agents.length === 0 ? (
          <div className="mt-2 text-sm text-muted-foreground">{t("noLinkedAgents")}</div>
        ) : (
          <div className="mt-2 flex flex-wrap gap-2">
            {agents.map((agent) => (
              <span
                key={agent.id}
                className="rounded-md border border-border bg-muted px-2 py-1 text-xs font-medium text-muted-foreground"
              >
                {agent.name} · {agent.status}
              </span>
            ))}
          </div>
        )}
      </div>
      <div className="border-t border-border px-4 py-3">
        <div className="text-xs font-medium text-muted-foreground">{t("printerCompat")}</div>
        {livePrintersSlot}
      </div>
    </section>
  );
}
