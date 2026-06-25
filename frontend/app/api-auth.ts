import { cookies } from "next/headers";

const apiToken = process.env.APP_API_TOKEN;
const staticAuthToken = process.env.APP_AUTH_BEARER_TOKEN;
const authCookieName = process.env.APP_AUTH_COOKIE_NAME ?? "pandar_auth_token";

export type AuthSource =
  | "request_cookie"
  | "app_auth_bearer_token"
  | "app_api_token"
  | "none";

export async function apiHeaders(contentType?: string) {
  const headers: Record<string, string> = {};
  if (contentType) {
    headers["content-type"] = contentType;
  }

  const cookieStore = await cookies();
  const cookieToken = cookieStore.get(authCookieName)?.value;
  const token = cookieToken || staticAuthToken || apiToken;
  if (token) {
    headers.authorization = `Bearer ${token}`;
  }

  return Object.keys(headers).length > 0 ? headers : undefined;
}

export async function authSource(): Promise<{
  source: AuthSource;
  cookieName: string;
}> {
  const cookieStore = await cookies();
  if (cookieStore.get(authCookieName)?.value) {
    return { source: "request_cookie", cookieName: authCookieName };
  }
  if (staticAuthToken) {
    return { source: "app_auth_bearer_token", cookieName: authCookieName };
  }
  if (apiToken) {
    return { source: "app_api_token", cookieName: authCookieName };
  }
  return { source: "none", cookieName: authCookieName };
}
