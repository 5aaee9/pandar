import {
  renderDashboardView,
  type DashboardPageProps,
} from "../dashboard-data";

export default function UsersPage(props: DashboardPageProps) {
  return renderDashboardView("users", props);
}
