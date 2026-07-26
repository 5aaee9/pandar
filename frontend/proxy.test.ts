import { NextRequest } from "next/server";
import { describe, expect, it } from "vitest";

import { proxy } from "./proxy";

describe("security proxy", () => {
  it("uses a per-request script nonce without unsafe-inline", () => {
    const response = proxy(new NextRequest("https://pandar.example/devices"));
    const policy = response.headers.get("content-security-policy");

    expect(policy).toMatch(
      /script-src 'self' 'nonce-[a-f0-9]{32}' 'strict-dynamic'/,
    );
    expect(policy).not.toContain("script-src 'self' 'unsafe-inline'");
    expect(policy).toContain("frame-ancestors 'none'");
    expect(response.headers.get("x-frame-options")).toBe("DENY");
  });
});
