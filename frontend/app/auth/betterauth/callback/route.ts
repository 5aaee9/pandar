import { timingSafeEqual } from "node:crypto";
import { cookies } from "next/headers";
import { NextResponse } from "next/server";

import {
  betterAuthCallbackTarget,
  dashboardCallbackRedirectUrl,
} from "../callback-redirect";
import {
  authCookieOptions,
  isAllowedDashboardJwt,
  readAuthCookieConfig,
} from "../cookie";

export const runtime = "nodejs";

const maxCallbackBytes = 16 * 1024;

export async function POST(request: Request) {
  const contentLength = Number(request.headers.get("content-length"));
  if (Number.isFinite(contentLength) && contentLength > maxCallbackBytes) {
    return new NextResponse("authentication callback is too large", {
      status: 413,
      headers: { "cache-control": "no-store" },
    });
  }
  if (
    !request.headers
      .get("content-type")
      ?.startsWith("application/x-www-form-urlencoded") ||
    request.headers.has("content-encoding")
  ) {
    return new NextResponse("invalid authentication callback", {
      status: 415,
      headers: { "cache-control": "no-store" },
    });
  }
  const reader = request.body?.getReader();
  if (!reader) {
    return new NextResponse("invalid authentication callback", {
      status: 400,
      headers: { "cache-control": "no-store" },
    });
  }
  const chunks: Uint8Array[] = [];
  let bodyBytes = 0;
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    bodyBytes += value.byteLength;
    if (bodyBytes > maxCallbackBytes) {
      await reader.cancel();
      return new NextResponse("authentication callback is too large", {
        status: 413,
        headers: { "cache-control": "no-store" },
      });
    }
    chunks.push(value);
  }
  const form = new URLSearchParams(
    Buffer.concat(chunks.map((chunk) => Buffer.from(chunk))).toString("utf8"),
  );
  const token = form.get("token");
  const state = form.get("state");
  const cookieStore = await cookies();
  const expectedState = cookieStore.get("pandar_auth_state")?.value;
  if (
    typeof token !== "string" ||
    typeof state !== "string" ||
    !expectedState ||
    !sameState(state, expectedState) ||
    !isAllowedDashboardJwt(token)
  ) {
    return new NextResponse("invalid authentication callback", {
      status: 400,
      headers: {
        "cache-control": "no-store",
      },
    });
  }

  const response = NextResponse.redirect(
    dashboardCallbackRedirectUrl(
      betterAuthCallbackTarget(request.url),
      request.url,
    ),
    303,
  );
  response.cookies.set(readAuthCookieConfig().name, token, authCookieOptions());
  response.cookies.set("pandar_auth_state", "", {
    httpOnly: true,
    sameSite: "lax",
    secure: readAuthCookieConfig().secure,
    path: "/auth/betterauth/callback",
    maxAge: 0,
  });
  response.headers.set("cache-control", "no-store");
  response.headers.set("referrer-policy", "no-referrer");
  return response;
}

function sameState(actual: string, expected: string) {
  const actualBytes = Buffer.from(actual);
  const expectedBytes = Buffer.from(expected);
  return (
    actualBytes.length === expectedBytes.length &&
    timingSafeEqual(actualBytes, expectedBytes)
  );
}
