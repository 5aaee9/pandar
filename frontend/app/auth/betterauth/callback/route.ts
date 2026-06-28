import { NextResponse } from "next/server";

import {
  authCookieOptions,
  isAllowedDashboardJwt,
  readAuthCookieConfig,
} from "../cookie";

export function GET() {
  return new NextResponse(callbackHtml(), {
    headers: {
      "content-type": "text/html; charset=utf-8",
      "cache-control": "no-store",
    },
  });
}

export async function POST(request: Request) {
  const token = (await request.text()).trim();
  if (!isAllowedDashboardJwt(token)) {
    return new NextResponse("malformed token", { status: 400 });
  }

  const response = NextResponse.redirect(new URL("/", request.url), 303);
  response.cookies.set(readAuthCookieConfig().name, token, authCookieOptions());
  return response;
}

function callbackHtml() {
  return `<!doctype html>
<html lang="en">
<head><meta charset="utf-8"><title>Signing in</title></head>
<body>
<script>
(async () => {
  const params = new URLSearchParams(location.hash.slice(1));
  const token = params.get("token") || "";
  const response = await fetch(location.pathname + location.search, {
    method: "POST",
    headers: { "content-type": "text/plain;charset=UTF-8" },
    body: token
  });
  if (response.ok) {
    location.replace(response.url || "/");
    return;
  }
  document.body.textContent = "Sign-in failed.";
})();
</script>
</body>
</html>`;
}
