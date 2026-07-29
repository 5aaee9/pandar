"use client";

import { useTranslations } from "next-intl";

import { AdminSectionGuard } from "./admin-section-states";
import type {
  JoinLink,
  Tenant,
  User,
  UserIdentity,
} from "./dashboard-types";
import { InvitesSection } from "./users-invites";
import { MembersSection } from "./users-members";

export function UsersDashboard({
  selectedTenant,
  adminUnavailable,
  adminLoadError,
  users,
  identities,
  joinLinks,
  meEmail,
}: {
  selectedTenant: Tenant | null;
  adminUnavailable: boolean;
  adminLoadError: boolean;
  users: User[];
  identities: UserIdentity[];
  joinLinks: JoinLink[];
  meEmail: string | null;
}) {
  const t = useTranslations("usersPage");

  return (
    <AdminSectionGuard
      title={t("pageTitle")}
      selectedTenant={selectedTenant}
      loadError={adminLoadError}
      unavailable={adminUnavailable}
    >
      {(tenant) => (
        <div className="grid gap-4">
          <MembersSection
            identities={identities}
            meEmail={meEmail}
            tenant={tenant}
            users={users}
          />
          <InvitesSection joinLinks={joinLinks} tenant={tenant} />
        </div>
      )}
    </AdminSectionGuard>
  );
}
