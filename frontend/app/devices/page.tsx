import {
  renderDashboardView,
  type DashboardPageProps,
} from "../dashboard-data";

export default function DevicesPage(props: DashboardPageProps) {
  return renderDashboardView("devices", props);
}
