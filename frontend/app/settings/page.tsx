import {
  renderDashboardView,
  type DashboardPageProps,
} from "../dashboard-data";

export default function SettingsPage(props: DashboardPageProps) {
  return renderDashboardView("settings", props);
}
