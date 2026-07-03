export const DASHBOARD_VIEWS = [
  "devices",
  "jobs",
  "agents",
  "users",
  "settings",
] as const;

export type DashboardView = (typeof DASHBOARD_VIEWS)[number];

export type DashboardQuery = {
  tenant?: string;
  command?: string;
  status?: string;
};

export function dashboardViewTitleKey(view: DashboardView) {
  return view;
}

export function dashboardRootRedirectTarget(query: DashboardQuery) {
  return dashboardPath(query.command ? "agents" : "devices", query);
}

export function dashboardSidebarHref(
  view: DashboardView,
  query: DashboardQuery,
) {
  return dashboardPath(view, { tenant: query.tenant });
}

export function dashboardTenantHref(
  view: DashboardView,
  tenant: string,
  query: DashboardQuery,
) {
  return dashboardPath(view, {
    tenant,
    status: query.status,
    command: view === "agents" ? query.command : undefined,
  });
}

function dashboardPath(view: DashboardView, query: DashboardQuery = {}) {
  const params = new URLSearchParams();
  if (query.tenant) params.set("tenant", query.tenant);
  if (query.command) params.set("command", query.command);
  if (query.status) params.set("status", query.status);
  const suffix = params.toString();
  return suffix ? `/${view}?${suffix}` : `/${view}`;
}

export function logoutHref({ signOutUrl }: { signOutUrl: string | null }) {
  return signOutUrl;
}
