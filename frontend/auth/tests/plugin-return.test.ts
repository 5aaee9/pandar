import { describe, expect, it } from "vitest";

import {
  decodePluginSignInReturnTarget,
  encodePluginSignInReturnTarget,
} from "../../app/auth/betterauth/callback-redirect";
import {
  normalizePluginReturnTo,
  withPluginReturnTo,
} from "../lib/plugin-return";

describe("plugin return target", () => {
  const target =
    "/plugin-sign-in?tenant=tenant-1&redirect_url=http%3A%2F%2Flocalhost%3A13618%2Fcallback";
  const token = withPluginReturnTo("/sign-in", target).split("return_to=")[1];

  it("accepts only the plugin sign-in path", () => {
    expect(normalizePluginReturnTo(token)).toBe(target);
    expect(normalizePluginReturnTo([token, "v1.bm90LXRoZS10YXJnZXQ"])).toBe(
      target,
    );

    for (const invalid of [
      target,
      "",
      "v1.",
      "v1.%%%",
      "v1.aHR0cDovL1s",
      `v1.${"a".repeat(4094)}`,
      "v1.aHR0cHM6Ly9ldmlsLmV4YW1wbGUvcGx1Z2luLXNpZ24taW4",
      "v1.Ly9ldmlsLmV4YW1wbGUvcGx1Z2luLXNpZ24taW4",
      "v1.L290aGVy",
      "v1.L3BsdWdpbi1zaWduLWluI2ZyYWdtZW50",
      "v1.L1xldmlsLmV4YW1wbGUvcGx1Z2luLXNpZ24taW4",
    ]) {
      expect(normalizePluginReturnTo(invalid)).toBeNull();
    }
  });

  it("carries the normalized target through relative and absolute callbacks", () => {
    for (const url of [
      withPluginReturnTo("/auth/complete", target),
      withPluginReturnTo(
        "https://pandar.example/auth/betterauth/callback?source=issuer",
        target,
      ),
    ]) {
      const carried = new URL(url, "https://auth.example").searchParams.get(
        "return_to",
      );
      expect(carried).toMatch(/^v1\.[A-Za-z0-9_-]+$/);
      expect(carried).not.toMatch(/[%&]/);
      expect(normalizePluginReturnTo(carried ?? undefined)).toBe(target);
    }
  });

  it("keeps the web and auth codecs interoperable", () => {
    const webToken = encodePluginSignInReturnTarget(target);
    expect(normalizePluginReturnTo(webToken)).toBe(target);

    const authToken = new URL(
      withPluginReturnTo("/auth/complete", target),
      "https://auth.example",
    ).searchParams.get("return_to");
    expect(decodePluginSignInReturnTarget(authToken)).toBe(target);
  });

  it("removes stale return targets when no validated target exists", () => {
    expect(
      withPluginReturnTo(
        "https://pandar.example/auth/betterauth/callback?return_to=%2Fevil&source=issuer",
        null,
      ),
    ).toBe(
      "https://pandar.example/auth/betterauth/callback?source=issuer",
    );
  });
});
