import {
  dashboardSidebarDefaultOpen,
  renderDashboardView,
  type DashboardPageProps,
} from "../dashboard-data";

export default async function SettingsPage(props: DashboardPageProps) {
  return renderDashboardView("settings", {
    ...props,
    sidebarDefaultOpen: await dashboardSidebarDefaultOpen(),
  });
}
