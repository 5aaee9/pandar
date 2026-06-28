import assert from "node:assert/strict";
import { pathToFileURL } from "node:url";

const cookieModuleUrl = pathToFileURL(
  new URL("./cookie.ts", import.meta.url).pathname,
);
const {
  authCookieOptions,
  clearedAuthCookieOptions,
  isCompactJwt,
  readAuthCookieConfig,
} = await import(cookieModuleUrl.href);

const originalEnv = { ...process.env };

try {
  process.env.APP_AUTH_COOKIE_NAME = "";
  process.env.APP_AUTH_COOKIE_MAX_AGE_SECONDS = "";
  process.env.APP_BASE_URL = "http://localhost:3000";
  assert.deepEqual(readAuthCookieConfig(), {
    name: "pandar_auth_token",
    maxAgeSeconds: 43200,
    secure: false,
  });

  process.env.APP_AUTH_COOKIE_NAME = "session_token";
  process.env.APP_AUTH_COOKIE_MAX_AGE_SECONDS = "60";
  process.env.APP_BASE_URL = "https://pandar.example";
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
} finally {
  process.env = originalEnv;
}
