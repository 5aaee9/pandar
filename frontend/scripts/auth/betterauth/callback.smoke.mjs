import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

import {
  betterAuthCallbackTarget,
  dashboardCallbackRedirectUrl,
  decodePluginSignInReturnTarget,
  encodePluginSignInReturnTarget,
} from "../../../app/auth/betterauth/callback-redirect.ts";

const pluginTarget =
  "/plugin-sign-in?redirect_url=http://localhost:13618/callback";
const encodedPluginTarget = encodePluginSignInReturnTarget(pluginTarget);

assert.equal(
  betterAuthCallbackTarget(
    `https://pandar.example/auth/betterauth/callback?return_to=${encodedPluginTarget}`,
  ),
  pluginTarget,
);
assert.equal(decodePluginSignInReturnTarget(encodedPluginTarget), pluginTarget);

assert.equal(
  dashboardCallbackRedirectUrl(
    "/",
    "https://0.0.0.0:3000/auth/betterauth/callback",
    "https://pandar.example",
  ).toString(),
  "https://pandar.example/",
);
assert.equal(
  dashboardCallbackRedirectUrl(
    "/",
    "https://0.0.0.0:3000/auth/betterauth/callback",
    "",
  ).toString(),
  "https://0.0.0.0:3000/",
);

for (const returnTo of [
  "https://evil.example/plugin-sign-in",
  "//evil.example/plugin-sign-in",
  "/other",
  "/plugin-sign-in#fragment",
  "/\\evil.example/plugin-sign-in",
]) {
  assert.equal(
    betterAuthCallbackTarget(
      `https://pandar.example/auth/betterauth/callback?return_to=${encodePluginSignInReturnTarget(returnTo)}`,
    ),
    "/",
  );
}
for (const malformed of [null, "", "v1.", "v1.not+base64", "legacy-token"]) {
  assert.equal(decodePluginSignInReturnTarget(malformed), null);
}

const routeSource = await readFile(
  new URL("../../../app/auth/betterauth/callback/route.ts", import.meta.url),
  "utf8",
);

assert.match(
  routeSource,
  /export\s+async\s+function\s+POST\(request:\s*Request\)/,
);
assert.doesNotMatch(routeSource, /export\s+(?:async\s+)?function\s+GET/);
assert.match(
  routeSource,
  /content-type[\s\S]*application\/x-www-form-urlencoded/,
);
assert.match(routeSource, /bodyBytes\s*>\s*maxCallbackBytes/);
assert.match(routeSource, /form\.get\("token"\)/);
assert.match(routeSource, /form\.get\("state"\)/);
assert.match(routeSource, /cookieStore\.get\("pandar_auth_state"\)/);
assert.match(routeSource, /sameState\(state, expectedState\)/);
assert.match(routeSource, /isAllowedDashboardJwt\(token\)/);
assert.match(
  routeSource,
  /dashboardCallbackRedirectUrl\(\s*betterAuthCallbackTarget\(request\.url\),\s*request\.url,?\s*\)/s,
);
assert.match(
  routeSource,
  /response\.cookies\.set\(readAuthCookieConfig\(\)\.name, token, authCookieOptions\(\)\)/,
);
assert.match(routeSource, /response\.cookies\.set\("pandar_auth_state", "",/);
assert.match(routeSource, /timingSafeEqual\(actualBytes, expectedBytes\)/);
assert.match(
  routeSource,
  /response\.headers\.set\("cache-control", "no-store"\)/,
);
assert.match(
  routeSource,
  /response\.headers\.set\("referrer-policy", "no-referrer"\)/,
);
assert.doesNotMatch(routeSource, /searchParams\.get\("token"\)/);
