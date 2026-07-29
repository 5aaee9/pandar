const knownPositiveActionStatuses = new Set([
  "refresh_queued",
  "refresh_partial",
  "materials_refresh_queued",
  "agent_deleted",
  "printer_deleted",
  "printer_updated",
  "job_created",
  "jobs_cleared",
  "job_deleted",
  "tenant_created",
  "tenant_token_revoked",
  "join_link_accepted",
  "join_link_revoked",
  "user_role_updated",
  "retry_queued",
  "retry_partial",
  "reprint_queued",
  "duplicate_queued",
  "printer_control_queued",
]);

export type ActionStatusTone = "success" | "warning" | "error";

export type StatusTranslator = {
  (key: string): string;
  has(key: string): boolean;
};

export function formatActionStatus(status: string, tStatus: StatusTranslator) {
  if (tStatus.has(status)) {
    return tStatus(status);
  }
  return status
    .split("_")
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(" ");
}

export function actionStatusTone(status: string): ActionStatusTone {
  if (status.includes("partial")) {
    return "warning";
  }
  if (status.startsWith("http_") || !knownPositiveActionStatuses.has(status)) {
    return "error";
  }
  return "success";
}

export function clearStatusQueryFromUrl() {
  const url = new URL(window.location.href);
  url.searchParams.delete("status");
  const nextUrl = `${url.pathname}${url.search}${url.hash}`;
  window.history.replaceState(window.history.state, "", nextUrl);
}
