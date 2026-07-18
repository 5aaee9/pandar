import {
  dashboardSidebarDefaultOpen,
  renderDashboardView,
  type DashboardPageProps,
} from "../dashboard-data";

export default async function AgentsPage(props: DashboardPageProps) {
  return renderDashboardView("agents", {
    ...props,
    sidebarDefaultOpen: await dashboardSidebarDefaultOpen(),
  });
}
