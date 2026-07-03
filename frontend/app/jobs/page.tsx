import {
  renderDashboardView,
  type DashboardPageProps,
} from "../dashboard-data";

export default function JobsPage(props: DashboardPageProps) {
  return renderDashboardView("jobs", props);
}
