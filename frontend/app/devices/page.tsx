import {
  dashboardSidebarDefaultOpen,
  renderDashboardView,
  type DashboardPageProps,
} from "../dashboard-data";

export default async function DevicesPage(props: DashboardPageProps) {
  return renderDashboardView("devices", {
    ...props,
    sidebarDefaultOpen: await dashboardSidebarDefaultOpen(),
  });
}
