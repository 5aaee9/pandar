import { useTranslations } from "next-intl";

import type { Tenant } from "./dashboard-types";
import { EmptyState, SectionHeader } from "./dashboard-ui";

export function AdminSectionStates({
  title,
  subtitle,
  meta,
  emptyState,
}: {
  title: string;
  subtitle: string;
  meta: string;
  emptyState: { title: string; message: string };
}) {
  return (
    <section className="overflow-hidden rounded-md border border-border bg-card text-card-foreground">
      <SectionHeader title={title} subtitle={subtitle} meta={meta} />
      <EmptyState title={emptyState.title} message={emptyState.message} />
    </section>
  );
}

export function AdminSectionGuard({
  title,
  selectedTenant,
  loadError,
  unavailable,
  children,
}: {
  title: string;
  selectedTenant: Tenant | null;
  loadError: boolean;
  unavailable: boolean;
  children: (tenant: Tenant) => React.ReactNode;
}) {
  const t = useTranslations("admin");
  if (!selectedTenant) {
    return (
      <AdminSectionStates
        title={title}
        subtitle={t("subtitleNone")}
        meta={t("metaAdmin")}
        emptyState={{ title: t("noTenantTitle"), message: t("noTenantMessage") }}
      />
    );
  }
  if (loadError) {
    return (
      <AdminSectionStates
        title={title}
        subtitle={t("subtitleTenant", { name: selectedTenant.display_name })}
        meta={t("metaAdmin")}
        emptyState={{ title: t("loadErrorTitle"), message: t("loadErrorMessage") }}
      />
    );
  }
  if (unavailable) {
    return (
      <AdminSectionStates
        title={title}
        subtitle={t("subtitleUnavailable", { name: selectedTenant.display_name })}
        meta={t("metaRestricted")}
        emptyState={{ title: t("unavailableTitle"), message: t("unavailableMessage") }}
      />
    );
  }
  return <>{children(selectedTenant)}</>;
}
