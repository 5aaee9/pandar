import {
  renderDashboardView,
  type DashboardPageProps,
} from "../dashboard-data";

export default function AgentsPage(props: DashboardPageProps) {
  return renderDashboardView("agents", props);
}
