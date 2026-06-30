import assert from "node:assert/strict";

import { dashboardAuthRedirectTarget } from "../app/auth-redirect.ts";
import { safeSignOutRedirectTarget } from "../app/auth/betterauth/sign-out-redirect.ts";

const betterAuth = {
  provider: "betterauth",
  signInUrl: "https://auth.example/sign-in",
};
const logto = {
  provider: "logto",
  signInUrl: "https://logto.example/sign-in",
};
const clerk = {
  provider: "clerk",
  signInUrl: "/sign-in",
};

assert.equal(
  dashboardAuthRedirectTarget({ source: "none", provider: betterAuth }),
  "https://auth.example/sign-in",
);
assert.equal(
  dashboardAuthRedirectTarget({ source: "none", provider: logto }),
  "https://logto.example/sign-in",
);
assert.equal(
  dashboardAuthRedirectTarget({ source: "none", provider: clerk }),
  "/sign-in",
);
assert.equal(
  dashboardAuthRedirectTarget({
    source: "none",
    provider: { provider: "none", signInUrl: null },
  }),
  null,
);
assert.equal(
  dashboardAuthRedirectTarget({
    source: "none",
    provider: { provider: "betterauth", signInUrl: null },
  }),
  null,
);
assert.equal(
  dashboardAuthRedirectTarget({
    source: "app_auth_bearer_token",
    provider: betterAuth,
  }),
  null,
);
assert.equal(
  dashboardAuthRedirectTarget({
    source: "app_api_token",
    provider: betterAuth,
  }),
  null,
);
assert.equal(
  dashboardAuthRedirectTarget({
    source: "app_auth_bearer_token",
    provider: betterAuth,
    meStatus: 401,
  }),
  null,
);
assert.equal(
  dashboardAuthRedirectTarget({
    source: "app_api_token",
    provider: betterAuth,
    meStatus: 401,
  }),
  null,
);
assert.equal(
  dashboardAuthRedirectTarget({
    source: "request_cookie",
    provider: betterAuth,
    meStatus: 401,
  }),
  "https://auth.example/sign-in",
);
assert.equal(
  dashboardAuthRedirectTarget({
    source: "request_cookie",
    provider: logto,
    meStatus: 401,
  }),
  "https://logto.example/sign-in",
);
assert.equal(
  dashboardAuthRedirectTarget({
    source: "request_cookie",
    provider: clerk,
    meStatus: 401,
  }),
  "/sign-in",
);
assert.equal(
  dashboardAuthRedirectTarget({
    source: "request_cookie",
    provider: betterAuth,
    meStatus: 200,
  }),
  null,
);
assert.equal(
  dashboardAuthRedirectTarget({
    source: "request_cookie",
    provider: betterAuth,
    meStatus: 500,
  }),
  null,
);
assert.equal(
  dashboardAuthRedirectTarget({
    source: "request_cookie",
    provider: betterAuth,
  }),
  null,
);

assert.equal(
  safeSignOutRedirectTarget(
    "https://auth.example/sign-in",
    "https://auth.example/sign-in",
  ),
  "https://auth.example/sign-in",
);
assert.equal(
  safeSignOutRedirectTarget(
    "https://evil.example",
    "https://auth.example/sign-in",
  ),
  "/",
);
assert.equal(
  safeSignOutRedirectTarget(null, "https://auth.example/sign-in"),
  "/",
);
assert.equal(
  safeSignOutRedirectTarget("not a url", "https://auth.example/sign-in"),
  "/",
);
assert.equal(
  safeSignOutRedirectTarget(
    "https://auth.example/sign-out",
    "https://auth.example/sign-in",
  ),
  "/",
);
assert.equal(safeSignOutRedirectTarget("/sign-in", "/sign-in"), "/sign-in");
assert.equal(safeSignOutRedirectTarget("/other", "/sign-in"), "/");
