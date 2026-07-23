import { betterAuth } from "better-auth";
import { magicLink } from "better-auth/plugins";
import { describe, expect, it } from "vitest";

import {
  normalizePluginReturnTo,
  withPluginReturnTo,
} from "../lib/plugin-return";

describe("Better Auth magic-link plugin return", () => {
  it("preserves the complete Studio callback through the real verify handler", async () => {
    let magicLinkUrl = "";
    const auth = betterAuth({
      baseURL: "https://auth.example",
      basePath: "/api/auth",
      secret: "pandar-magic-link-return-test-secret",
      trustedOrigins: ["https://auth.example"],
      plugins: [
        magicLink({
          sendMagicLink: async ({ url }) => {
            magicLinkUrl = url;
          },
        }),
      ],
    });
    const target =
      "/plugin-sign-in?tenant=tenant-1&redirect_url=http%3A%2F%2Flocalhost%3A13618%2Fcallback%3Fsource%3Dstudio%26attempt%3D2";
    const completionUrl = withPluginReturnTo("/auth/complete", target);

    const signIn = await auth.handler(
      new Request("https://auth.example/api/auth/sign-in/magic-link", {
        method: "POST",
        headers: {
          "content-type": "application/json",
          origin: "https://auth.example",
        },
        body: JSON.stringify({
          email: "studio@example.com",
          callbackURL: completionUrl,
          newUserCallbackURL: completionUrl,
          errorCallbackURL: withPluginReturnTo("/sign-in", target),
        }),
      }),
    );
    expect(signIn.status).toBe(200);
    expect(magicLinkUrl).not.toBe("");

    const verify = await auth.handler(new Request(magicLinkUrl));
    expect(verify.status).toBe(302);
    const location = verify.headers.get("location");
    expect(location).not.toBeNull();
    const redirect = new URL(location!);
    expect(redirect.pathname).toBe("/auth/complete");
    expect(redirect.searchParams.get("error")).toBeNull();
    expect(verify.headers.get("set-cookie")).toMatch(
      /better-auth\.session_token=/,
    );
    const returned = redirect.searchParams.get("return_to");
    expect(returned).toMatch(/^v1\.[A-Za-z0-9_-]+$/);
    expect(normalizePluginReturnTo(returned ?? undefined)).toBe(target);
  });
});
