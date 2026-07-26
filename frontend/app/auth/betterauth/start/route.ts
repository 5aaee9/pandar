import { randomBytes } from "node:crypto";
import { NextResponse } from "next/server";

import { authProviderConfig } from "../../../auth-provider";
import { readAuthCookieConfig } from "../cookie";

export const runtime = "nodejs";

export function GET(request: Request) {
  const provider = authProviderConfig();
  if (provider.provider !== "betterauth" || !provider.betterAuthBaseUrl) {
    return new NextResponse("Better Auth is not configured", { status: 503 });
  }

  const requestUrl = new URL(request.url);
  const target = new URL("/sign-in", provider.betterAuthBaseUrl);
  const returnTo = requestUrl.searchParams.get("return_to");
  if (returnTo) {
    target.searchParams.set("return_to", returnTo);
  }
  const state = randomBytes(32).toString("base64url");
  target.searchParams.set("state", state);

  const response = NextResponse.redirect(target, 303);
  const secure = readAuthCookieConfig().secure;
  response.cookies.set("pandar_auth_state", state, {
    httpOnly: true,
    sameSite: secure ? "none" : "lax",
    secure,
    path: "/auth/betterauth/callback",
    maxAge: 600,
  });
  response.headers.set("cache-control", "no-store");
  response.headers.set("referrer-policy", "no-referrer");
  return response;
}
