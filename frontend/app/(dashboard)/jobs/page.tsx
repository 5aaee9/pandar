import {
  getDashboardRequestContext,
  getMembershipForRequest,
} from "../../dashboard-data";
import { canManageJobs } from "../../membership-policy";
import { JobsPageClient } from "./jobs-page-client";

export default async function JobsPage() {
  const { auth, selectedTenant } = await getDashboardRequestContext();

  if (!selectedTenant) {
    return <div>No tenant selected</div>;
  }

  const membership =
    auth.provider !== "none"
      ? await getMembershipForRequest(selectedTenant.id)
      : { role: null, error: null };

  return (
    <JobsPageClient
      auth={auth}
      selectedTenant={selectedTenant}
      canManageJobs={canManageJobs(auth.provider, membership)}
    />
  );
}
