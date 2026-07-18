import {
  dashboardSidebarDefaultOpen,
  renderDashboardView,
  type DashboardPageProps,
} from "../dashboard-data";

export default async function UsersPage(props: DashboardPageProps) {
  return renderDashboardView("users", {
    ...props,
    sidebarDefaultOpen: await dashboardSidebarDefaultOpen(),
  });
}
