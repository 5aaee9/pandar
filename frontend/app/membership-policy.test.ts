import { describe, expect, it } from "vitest";

import {
  adminAccessLoadError,
  adminAccessUnavailable,
  canManageJobs,
  type MembershipResult,
} from "./membership-policy";

function membership(
  role: MembershipResult["role"],
  error: MembershipResult["error"] = null,
): MembershipResult {
  return { role, error };
}

describe("canManageJobs", () => {
  it("allows local no-auth deployments regardless of membership", () => {
    expect(canManageJobs("none", membership(null))).toBe(true);
    expect(canManageJobs("none", membership("viewer"))).toBe(true);
  });

  it("allows operators and tenant administrators for external providers", () => {
    expect(canManageJobs("pandar", membership("operator"))).toBe(true);
    expect(canManageJobs("pandar", membership("tenant_admin"))).toBe(true);
  });

  it("denies viewers", () => {
    expect(canManageJobs("pandar", membership("viewer"))).toBe(false);
  });

  it("fails closed when the membership failed to load or is missing", () => {
    expect(
      canManageJobs("pandar", membership(null, "/me request failed")),
    ).toBe(false);
    expect(canManageJobs("pandar", membership(null))).toBe(false);
  });
});

describe("adminAccessUnavailable", () => {
  it("keeps local no-auth deployments administrable", () => {
    expect(adminAccessUnavailable("none", membership(null))).toBe(false);
  });

  it("restricts non-admin roles and unknown membership", () => {
    expect(adminAccessUnavailable("pandar", membership("viewer"))).toBe(true);
    expect(adminAccessUnavailable("pandar", membership("operator"))).toBe(true);
    expect(
      adminAccessUnavailable("pandar", membership(null, "/me request failed")),
    ).toBe(true);
  });

  it("admits tenant administrators with a loaded membership", () => {
    expect(adminAccessUnavailable("pandar", membership("tenant_admin"))).toBe(
      false,
    );
  });
});

describe("adminAccessLoadError", () => {
  it("reports only membership load failures as load errors", () => {
    expect(
      adminAccessLoadError("pandar", membership(null, "/me request failed")),
    ).toBe(true);
    expect(adminAccessLoadError("pandar", membership("viewer"))).toBe(false);
    expect(adminAccessLoadError("none", membership(null))).toBe(false);
  });
});
