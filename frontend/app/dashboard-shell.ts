export const DASHBOARD_VIEWS = [
  "devices",
  "jobs",
  "agents",
  "users",
  "settings",
] as const;

export type DashboardView = (typeof DASHBOARD_VIEWS)[number];

export type DashboardQuery = {
  command?: string;
  status?: string;
};

export function dashboardViewTitleKey(view: DashboardView) {
  return view;
}

export function dashboardRootRedirectTarget(query: DashboardQuery) {
  return dashboardPath(query.command ? "agents" : "devices", query);
}

export function dashboardSidebarHref(view: DashboardView) {
  return `/${view}`;
}

export function agentSettingsHref(agentId: string) {
  return `/agents/${encodeURIComponent(agentId)}/settings`;
}

function dashboardPath(view: DashboardView, query: DashboardQuery = {}) {
  const params = new URLSearchParams();
  if (query.command) params.set("command", query.command);
  if (query.status) params.set("status", query.status);
  const suffix = params.toString();
  return suffix ? `/${view}?${suffix}` : `/${view}`;
}

export function logoutHref({ signOutUrl }: { signOutUrl: string | null }) {
  return signOutUrl;
}
