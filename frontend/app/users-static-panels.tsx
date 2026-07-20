import { useTranslations } from "next-intl";

import { SectionHeader } from "./dashboard-ui";

export function UsersStaticPanels({
  usersPanel,
  emptyState,
}: {
  usersPanel: React.ReactNode;
  emptyState: React.ReactNode;
}) {
  const t = useTranslations("admin");

  return (
    <section className="overflow-hidden rounded-md border border-border bg-card">
      <SectionHeader
        title={t("users")}
        subtitle={t("subtitleTenant", { name: "..." })}
        meta={t("usersMeta", { count: 0 })}
      />
      {usersPanel ?? emptyState}
    </section>
  );
}
