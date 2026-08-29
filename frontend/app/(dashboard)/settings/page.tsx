import {
  getDashboardRequestContext,
  getMembershipForRequest,
} from "../../dashboard-data";
import { SettingsPageClient } from "./settings-page-client";

export default async function SettingsPage() {
  const { auth, selectedTenant } = await getDashboardRequestContext();

  if (!selectedTenant) {
    return <div>No tenant selected</div>;
  }

  const membership =
    auth.provider !== "none"
      ? await getMembershipForRequest(selectedTenant.id)
      : { role: null, error: null };

  return (
    <SettingsPageClient
      auth={auth}
      selectedTenant={selectedTenant}
      membership={membership}
    />
  );
}
