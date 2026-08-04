import { NextResponse } from "next/server";

export function rejectTrustedAuthSignOutMutation(request: Request) {
  const authBaseUrl = process.env.APP_AUTH_BETTER_AUTH_BASE_URL;
  if (authBaseUrl) {
    try {
      if (request.headers.get("origin") === new URL(authBaseUrl).origin) {
        return null;
      }
    } catch {
      // Provider configuration validation reports the malformed URL.
    }
  }
  return rejectCrossOriginMutation(request);
}

export function rejectCrossOriginMutation(request: Request) {
  const origin = request.headers.get("origin");
  const baseUrl = process.env.APP_BASE_URL || request.url;
  if (!URL.canParse(baseUrl)) {
    return NextResponse.json(
      { error: "cross_origin_request" },
      { status: 403 },
    );
  }
  const expectedOrigin = new URL(baseUrl).origin;
  const fetchSite = request.headers.get("sec-fetch-site");
  if (origin !== expectedOrigin || (fetchSite && fetchSite !== "same-origin")) {
    return NextResponse.json(
      { error: "cross_origin_request" },
      { status: 403 },
    );
  }
  return null;
}
