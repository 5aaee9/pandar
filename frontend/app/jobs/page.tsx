import {
  dashboardSidebarDefaultOpen,
  renderDashboardView,
  type DashboardPageProps,
} from "../dashboard-data";

export default async function JobsPage(props: DashboardPageProps) {
  return renderDashboardView("jobs", {
    ...props,
    sidebarDefaultOpen: await dashboardSidebarDefaultOpen(),
  });
}
