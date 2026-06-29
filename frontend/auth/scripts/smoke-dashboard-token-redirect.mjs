import assert from "node:assert/strict";
import { pathToFileURL } from "node:url";

const tokenModuleUrl = pathToFileURL(
  new URL("../lib/token.ts", import.meta.url).pathname,
);
const { redirectWithAuthToken } = await import(tokenModuleUrl.href);

const originalFetch = globalThis.fetch;
const originalWindow = globalThis.window;

async function captureRedirect({
  callbackUrl,
  token = "aaa.bbb.ccc",
  responseOk = true,
  body = { token },
}) {
  let redirectedTo = null;
  globalThis.fetch = async (input, init) => {
    assert.equal(input, "/api/auth/token");
    assert.deepEqual(init, { credentials: "include" });
    return {
      ok: responseOk,
      json: async () => body,
    };
  };
  globalThis.window = {
    location: {
      set href(value) {
        redirectedTo = value;
      },
    },
  };

  await redirectWithAuthToken(callbackUrl, {
    dashboardTokenEmpty: "empty token",
    dashboardTokenFailed: "token failed",
  });

  return redirectedTo;
}

try {
  assert.equal(
    await captureRedirect({
      callbackUrl: "https://pandar.example/auth/betterauth/callback",
    }),
    "https://pandar.example/auth/betterauth/callback?token=aaa.bbb.ccc",
  );

  assert.equal(
    await captureRedirect({
      callbackUrl:
        "https://pandar.example/auth/betterauth/callback?source=issuer",
      token: "header.payload.signature",
    }),
    "https://pandar.example/auth/betterauth/callback?source=issuer&token=header.payload.signature",
  );

  await assert.rejects(
    () =>
      captureRedirect({
        callbackUrl: "https://pandar.example/auth/betterauth/callback",
        responseOk: false,
      }),
    /token failed/,
  );

  await assert.rejects(
    () =>
      captureRedirect({
        callbackUrl: "https://pandar.example/auth/betterauth/callback",
        body: { token: "" },
      }),
    /empty token/,
  );
} finally {
  globalThis.fetch = originalFetch;
  globalThis.window = originalWindow;
}
