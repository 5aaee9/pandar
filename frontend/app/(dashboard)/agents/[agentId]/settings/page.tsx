import { getTranslations } from "next-intl/server";

import { getDashboardRequestContext } from "../../../../dashboard-data";
import { EmptyState } from "../../../../dashboard-ui";
import { AgentSettingsPageClient } from "./settings-page-client";

export default async function AgentSettingsPage({
  params,
}: {
  params: Promise<{ agentId: string }>;
}) {
  const [{ agentId }, { selectedTenant }] = await Promise.all([
    params,
    getDashboardRequestContext(),
  ]);

  if (!selectedTenant) {
    const t = await getTranslations("agents");
    return (
      <EmptyState title={t("noTenantTitle")} message={t("noTenantMessage")} />
    );
  }

  return (
    <AgentSettingsPageClient
      agentId={agentId}
      selectedTenant={selectedTenant}
    />
  );
}
