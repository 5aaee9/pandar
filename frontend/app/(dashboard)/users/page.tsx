import {
  getDashboardRequestContext,
  getMembershipForRequest,
} from "../../dashboard-data";
import {
  adminAccessLoadError,
  adminAccessUnavailable,
} from "../../membership-policy";
import { UsersPageClient } from "./users-page-client";

export default async function UsersPage() {
  const { auth, identity, selectedTenant } = await getDashboardRequestContext();

  if (!selectedTenant) {
    return <div>No tenant selected</div>;
  }

  const membership =
    auth.provider !== "none"
      ? await getMembershipForRequest(selectedTenant.id)
      : { role: null, error: null };

  return (
    <UsersPageClient
      selectedTenant={selectedTenant}
      adminUnavailable={adminAccessUnavailable(auth.provider, membership)}
      adminLoadError={adminAccessLoadError(auth.provider, membership)}
      meEmail={identity.me?.identity.email ?? null}
    />
  );
}
