import { NextResponse } from "next/server";

import { clearedAuthCookieOptions, readAuthCookieConfig } from "../cookie";

export function GET(request: Request) {
  const response = NextResponse.redirect(new URL("/", request.url));
  response.cookies.set(
    readAuthCookieConfig().name,
    "",
    clearedAuthCookieOptions(),
  );
  return response;
}
