import assert from "node:assert/strict";

import {
  authCookieOptions,
  clearedAuthCookieOptions,
  isAllowedDashboardJwt,
  isCompactJwt,
  readAuthCookieConfig,
} from "../../../app/auth/betterauth/cookie.ts";

const originalEnv = { ...process.env };

try {
  process.env.APP_AUTH_COOKIE_NAME = "";
  process.env.APP_AUTH_COOKIE_MAX_AGE_SECONDS = "";
  process.env.APP_BASE_URL = "http://localhost:3000";
  assert.deepEqual(readAuthCookieConfig(), {
    name: "pandar_auth_token",
    maxAgeSeconds: 43200,
    secure: false,
    issuer: null,
  });

  process.env.APP_AUTH_COOKIE_NAME = "session_token";
  process.env.APP_AUTH_COOKIE_MAX_AGE_SECONDS = "60";
  process.env.APP_BASE_URL = "https://pandar.example";
  process.env.APP_AUTH_BETTER_AUTH_BASE_URL = "https://auth.example";
  assert.deepEqual(readAuthCookieConfig(), {
    name: "session_token",
    maxAgeSeconds: 60,
    secure: true,
    issuer: "https://auth.example",
  });
  assert.deepEqual(authCookieOptions(), {
    httpOnly: true,
    sameSite: "lax",
    path: "/",
    maxAge: 60,
    secure: true,
  });
  assert.deepEqual(clearedAuthCookieOptions(), {
    httpOnly: true,
    sameSite: "lax",
    path: "/",
    maxAge: 0,
    secure: true,
  });

  assert.equal(isCompactJwt("aaa.bbb.ccc"), true);
  assert.equal(isCompactJwt("aaa.bbb"), false);
  assert.equal(isCompactJwt("aaa..ccc"), false);
  assert.equal(isCompactJwt("aaa.b bb.ccc"), false);
  assert.equal(isCompactJwt("aaa.bbb.ccc.ddd"), false);

  const validPayload = Buffer.from(
    JSON.stringify({
      iss: "https://auth.example",
      aud: "https://auth.example",
    }),
  ).toString("base64url");
  const wrongIssuerPayload = Buffer.from(
    JSON.stringify({
      iss: "https://evil.example",
      aud: "https://auth.example",
    }),
  ).toString("base64url");
  const wrongAudiencePayload = Buffer.from(
    JSON.stringify({
      iss: "https://auth.example",
      aud: "https://evil.example",
    }),
  ).toString("base64url");

  assert.equal(isAllowedDashboardJwt(`header.${validPayload}.signature`), true);
  assert.equal(
    isAllowedDashboardJwt(`header.${wrongIssuerPayload}.signature`),
    false,
  );
  assert.equal(
    isAllowedDashboardJwt(`header.${wrongAudiencePayload}.signature`),
    false,
  );
} finally {
  process.env = originalEnv;
}
