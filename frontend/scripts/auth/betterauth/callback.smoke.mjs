import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

import {
  betterAuthCallbackRedirect,
  dashboardCallbackRedirectUrl,
  encodePluginSignInReturnTarget,
} from "../../../app/auth/betterauth/callback-redirect.ts";

const validToken = "header.payload.signature";
const acceptsOnlyValidToken = (token) => token === validToken;

assert.deepEqual(
  betterAuthCallbackRedirect(
    `https://pandar.example/auth/betterauth/callback?token=${validToken}&return_to=${encodePluginSignInReturnTarget("/plugin-sign-in?redirect_url=http://localhost:13618/callback")}`,
    acceptsOnlyValidToken,
  ),
  {
    ok: true,
    token: validToken,
    target: "/plugin-sign-in?redirect_url=http://localhost:13618/callback",
    status: 303,
  },
);

assert.equal(
  dashboardCallbackRedirectUrl(
    "/",
    `https://0.0.0.0:3000/auth/betterauth/callback?token=${validToken}`,
    "https://pandar.example",
  ).toString(),
  "https://pandar.example/",
);

assert.equal(
  dashboardCallbackRedirectUrl(
    "/",
    `https://0.0.0.0:3000/auth/betterauth/callback?token=${validToken}`,
    "",
  ).toString(),
  "https://0.0.0.0:3000/",
);

assert.deepEqual(
  betterAuthCallbackRedirect(
    `https://pandar.example/auth/betterauth/callback?token=%20${validToken}%20`,
    acceptsOnlyValidToken,
  ),
  { ok: true, token: validToken, target: "/", status: 303 },
);

for (const returnTo of [
  "https://evil.example/plugin-sign-in",
  "//evil.example/plugin-sign-in",
  "/other",
  "/plugin-sign-in#fragment",
  "/\\evil.example/plugin-sign-in",
]) {
  assert.deepEqual(
    betterAuthCallbackRedirect(
      `https://pandar.example/auth/betterauth/callback?token=${validToken}&return_to=${encodePluginSignInReturnTarget(returnTo)}`,
      acceptsOnlyValidToken,
    ),
    { ok: true, token: validToken, target: "/", status: 303 },
  );
}

assert.deepEqual(
  betterAuthCallbackRedirect(
    "https://pandar.example/auth/betterauth/callback",
    acceptsOnlyValidToken,
  ),
  { ok: false, body: "malformed token", status: 400 },
);

assert.deepEqual(
  betterAuthCallbackRedirect(
    "https://pandar.example/auth/betterauth/callback?token=%20%20",
    () => true,
  ),
  { ok: false, body: "malformed token", status: 400 },
);

assert.deepEqual(
  betterAuthCallbackRedirect(
    "https://pandar.example/auth/betterauth/callback?token=bad-token",
    acceptsOnlyValidToken,
  ),
  { ok: false, body: "malformed token", status: 400 },
);

const routeSource = await readFile(
  new URL("../../../app/auth/betterauth/callback/route.ts", import.meta.url),
  "utf8",
);

assert.match(
  routeSource,
  /betterAuthCallbackRedirect\(request\.url,\s*isAllowedDashboardJwt\)/s,
);
assert.match(
  routeSource,
  /NextResponse\.redirect\([\s\S]*dashboardCallbackRedirectUrl\(result\.target, request\.url\),[\s\S]*result\.status,[\s\S]*\)/,
);
assert.doesNotMatch(routeSource, /new URL\(result\.target, request\.url\)/);
assert.match(
  routeSource,
  /response\.cookies\.set\(\s*readAuthCookieConfig\(\)\.name,\s*result\.token,\s*authCookieOptions\(\)/s,
);
assert.match(
  routeSource,
  /response\.headers\.set\("cache-control", "no-store"\)/,
);
assert.match(
  routeSource,
  /response\.headers\.set\("referrer-policy", "no-referrer"\)/,
);
assert.doesNotMatch(
  routeSource,
  /export\s+async\s+function\s+POST|export\s+function\s+POST/,
);
assert.doesNotMatch(
  routeSource,
  /callbackHtml|location\.hash|method:\s*"POST"|method:\s*'POST'/,
);
