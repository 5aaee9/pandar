import { getDashboardRequestContext } from "../../dashboard-data";
import { DevicesPageClient } from "./devices-page-client";

export default async function DevicesPage() {
  const { auth, selectedTenant } = await getDashboardRequestContext();

  if (!selectedTenant) {
    return <div>No tenant selected</div>;
  }

  return <DevicesPageClient auth={auth} selectedTenant={selectedTenant} />;
}
