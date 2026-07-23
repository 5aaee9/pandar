import { describe, expect, it } from "vitest";

import {
  decodePluginSignInReturnTarget,
  encodePluginSignInReturnTarget,
} from "../auth/betterauth/callback-redirect";
import {
  pluginAuthSignInUrl,
  pluginSignInReturnTarget,
} from "./auth-return";

describe("plugin authentication return", () => {
  it("preserves the Studio callback and selected tenant", () => {
    expect(
      pluginSignInReturnTarget(
        "tenant-1",
        "http://localhost:13618/callback",
      ),
    ).toBe(
      "/plugin-sign-in?tenant=tenant-1&redirect_url=http%3A%2F%2Flocalhost%3A13618%2Fcallback",
    );
  });

  it("adds the return target only to Better Auth sign-in", () => {
    const returnTarget = pluginSignInReturnTarget(
      undefined,
      "http://localhost:13618/callback",
    );
    const signIn = new URL(
      pluginAuthSignInUrl(
        {
          provider: "betterauth",
          signInUrl: "https://auth.example/sign-in?lang=en",
        },
        returnTarget,
      )!,
    );
    const token = signIn.searchParams.get("return_to");
    expect(token).toMatch(/^v1\.[A-Za-z0-9_-]+$/);
    expect(decodePluginSignInReturnTarget(token)).toBe(returnTarget);
    expect(signIn.searchParams.get("lang")).toBe("en");
    expect(
      pluginAuthSignInUrl(
        { provider: "logto", signInUrl: "https://logto.example/sign-in" },
        returnTarget,
      ),
    ).toBe("https://logto.example/sign-in");
  });

  it("rejects malformed and non-plugin opaque targets", () => {
    for (const invalid of [
      "",
      "v1.",
      "v1.%%%",
      "v1.aHR0cDovL1s",
      `v1.${"a".repeat(4094)}`,
      encodePluginSignInReturnTarget("https://evil.example/plugin-sign-in"),
      encodePluginSignInReturnTarget("//evil.example/plugin-sign-in"),
      encodePluginSignInReturnTarget("/other"),
      encodePluginSignInReturnTarget("/plugin-sign-in#fragment"),
      encodePluginSignInReturnTarget("/\\evil.example/plugin-sign-in"),
    ]) {
      expect(decodePluginSignInReturnTarget(invalid)).toBeNull();
    }
  });
});
