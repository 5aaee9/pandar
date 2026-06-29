import { NextResponse } from "next/server";

import { betterAuthCallbackRedirect } from "../callback-redirect";
import {
  authCookieOptions,
  isAllowedDashboardJwt,
  readAuthCookieConfig,
} from "../cookie";

export function GET(request: Request) {
  const result = betterAuthCallbackRedirect(request.url, isAllowedDashboardJwt);
  if (!result.ok) {
    return new NextResponse(result.body, {
      status: result.status,
      headers: {
        "cache-control": "no-store",
      },
    });
  }

  const response = NextResponse.redirect(
    new URL(result.target, request.url),
    result.status,
  );
  response.cookies.set(
    readAuthCookieConfig().name,
    result.token,
    authCookieOptions(),
  );
  response.headers.set("cache-control", "no-store");
  response.headers.set("referrer-policy", "no-referrer");
  return response;
}
