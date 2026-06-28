import { NextResponse } from "next/server";

import { authProviderConfig } from "../../../auth-provider";
import { clearedAuthCookieOptions, readAuthCookieConfig } from "../cookie";
import { safeSignOutRedirectTarget } from "../sign-out-redirect";

export function GET(request: Request) {
  const requestUrl = new URL(request.url);
  const target = safeSignOutRedirectTarget(
    requestUrl.searchParams.get("next"),
    authProviderConfig().signInUrl,
  );
  const response = NextResponse.redirect(new URL(target, request.url));
  response.cookies.set(
    readAuthCookieConfig().name,
    "",
    clearedAuthCookieOptions(),
  );
  return response;
}
