const dashboardStatePattern = /^[A-Za-z0-9_-]{43}$/;

export function normalizeDashboardState(value: string | string[] | undefined) {
  const state = Array.isArray(value) ? value[0] : value;
  return state && dashboardStatePattern.test(state) ? state : null;
}

export function withDashboardState(target: string, state: string | null) {
  if (!state) {
    return target;
  }
  const url = new URL(target, "http://pandar.local");
  url.searchParams.set("state", state);
  return target.startsWith("/")
    ? `${url.pathname}${url.search}`
    : url.toString();
}
