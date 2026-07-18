import { apiHeaders } from "./api-auth";

export const apiUrl = process.env.APP_API_URL ?? "http://localhost:8080";

export async function postJson(path: string, body: unknown) {
  return fetch(`${apiUrl}${path}`, {
    method: "POST",
    headers: await apiHeaders("application/json"),
    body: JSON.stringify(body),
  });
}

export function stringField(formData: FormData, name: string) {
  const value = formData.get(name);
  return typeof value === "string" ? value : "";
}

export function nullableField(formData: FormData, name: string) {
  const value = stringField(formData, name).trim();
  return value.length > 0 ? value : null;
}

export function optionalBoolean(formData: FormData, name: string) {
  return formData.has(name) ? formData.get(name) === "on" : null;
}

export function numberOrNull(formData: FormData, name: string) {
  const value = nullableField(formData, name);
  return value ? Number(value) : null;
}

export async function errorCode(response: Response) {
  try {
    const body = (await response.json()) as { error?: string };
    return body.error ?? `http_${response.status}`;
  } catch {
    return `http_${response.status}`;
  }
}

export function statusUrlForForm(
  formData: FormData,
  tenantId: string,
  status: string,
) {
  return statusUrl(tenantId, status, stringField(formData, "return_to"));
}

export function statusUrl(
  tenantId: string,
  status: string,
  returnTo?: string,
) {
  const view =
    returnTo === "jobs"
      ? "jobs"
      : returnTo === "agents"
        ? "agents"
        : returnTo === "settings"
          ? "settings"
          : "devices";
  return `/${view}?tenant=${encodeURIComponent(tenantId)}&status=${encodeURIComponent(status)}`;
}

export function agentsStatusUrl(tenantId: string, status: string) {
  return `/agents?tenant=${encodeURIComponent(tenantId)}&status=${encodeURIComponent(status)}`;
}

export function commandUrl(tenantId: string, commandId: string) {
  return `/agents?tenant=${encodeURIComponent(tenantId)}&command=${encodeURIComponent(commandId)}`;
}
