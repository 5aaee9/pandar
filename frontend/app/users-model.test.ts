import { describe, expect, it } from "vitest";

import type { JoinLink, User } from "./dashboard-types";
import {
  countByRole,
  filterUsers,
  inviteStatus,
  isLastTenantAdmin,
  isSelf,
  sortJoinLinks,
  sortUsers,
  userInitials,
} from "./users-model";

const NOW = Date.parse("2026-07-01T00:00:00Z");

function user(overrides: Partial<User>): User {
  return {
    id: "user-1",
    tenant_id: "tenant-1",
    email: "user@example.test",
    display_name: "User",
    role: "viewer",
    created_at: "2026-06-01T00:00:00Z",
    ...overrides,
  };
}

function joinLink(overrides: Partial<JoinLink>): JoinLink {
  return {
    id: "link-1",
    tenant_id: "tenant-1",
    role: "viewer",
    email_constraint: null,
    expires_at: "2026-07-08T00:00:00Z",
    max_uses: 1,
    used_count: 0,
    created_by_user_id: null,
    revoked_at: null,
    created_at: "2026-06-30T00:00:00Z",
    ...overrides,
  };
}

describe("inviteStatus", () => {
  it("marks a usable link as active", () => {
    expect(inviteStatus(joinLink({}), NOW)).toBe("active");
  });

  it("prefers revoked over every other state", () => {
    expect(
      inviteStatus(
        joinLink({
          revoked_at: "2026-06-30T12:00:00Z",
          expires_at: "2026-06-01T00:00:00Z",
          used_count: 1,
        }),
        NOW,
      ),
    ).toBe("revoked");
  });

  it("marks links past their expiry as expired", () => {
    expect(
      inviteStatus(joinLink({ expires_at: "2026-06-30T23:59:59Z" }), NOW),
    ).toBe("expired");
  });

  it("marks links at their use limit as exhausted", () => {
    expect(
      inviteStatus(joinLink({ used_count: 2, max_uses: 2 }), NOW),
    ).toBe("exhausted");
  });
});

describe("sortJoinLinks", () => {
  it("sorts active links first, newest first within each group", () => {
    const oldActive = joinLink({ id: "old-active", created_at: "2026-06-01T00:00:00Z" });
    const newActive = joinLink({ id: "new-active", created_at: "2026-06-30T00:00:00Z" });
    const expired = joinLink({
      id: "expired",
      expires_at: "2026-06-01T00:00:00Z",
      created_at: "2026-06-29T00:00:00Z",
    });

    expect(
      sortJoinLinks([oldActive, expired, newActive], NOW).map((link) => link.id),
    ).toEqual(["new-active", "old-active", "expired"]);
  });
});

describe("sortUsers", () => {
  it("orders by role first, then display name", () => {
    const users = [
      user({ id: "v", display_name: "Zoe", role: "viewer" }),
      user({ id: "a", display_name: "Bob", role: "tenant_admin" }),
      user({ id: "o", display_name: "Carol", role: "operator" }),
      user({ id: "b", display_name: "Alice", role: "tenant_admin" }),
    ];

    expect(sortUsers(users).map((entry) => entry.id)).toEqual([
      "b",
      "a",
      "o",
      "v",
    ]);
  });
});

describe("filterUsers", () => {
  const users = [
    user({ id: "1", display_name: "Ada Lovelace", email: "ada@example.test", role: "tenant_admin" }),
    user({ id: "2", display_name: "Grace Hopper", email: "grace@example.test", role: "operator" }),
  ];

  it("matches name and email case-insensitively", () => {
    expect(filterUsers(users, "ADA", "all").map((entry) => entry.id)).toEqual(["1"]);
    expect(
      filterUsers(users, "grace@EXAMPLE", "all").map((entry) => entry.id),
    ).toEqual(["2"]);
  });

  it("applies the role filter", () => {
    expect(filterUsers(users, "", "operator").map((entry) => entry.id)).toEqual(["2"]);
    expect(filterUsers(users, "ada", "viewer")).toEqual([]);
  });

  it("returns everyone for an empty query", () => {
    expect(filterUsers(users, "   ", "all")).toHaveLength(2);
  });
});

describe("countByRole", () => {
  it("counts every role, including zeroes", () => {
    expect(
      countByRole([
        user({ role: "tenant_admin" }),
        user({ id: "2", role: "tenant_admin" }),
        user({ id: "3", role: "viewer" }),
      ]),
    ).toEqual({ tenant_admin: 2, operator: 0, viewer: 1 });
  });
});

describe("isLastTenantAdmin", () => {
  it("is true only for the sole remaining admin", () => {
    const admin = user({ id: "a", role: "tenant_admin" });
    const viewer = user({ id: "v", role: "viewer" });
    expect(isLastTenantAdmin(admin, [admin, viewer])).toBe(true);
    expect(isLastTenantAdmin(admin, [admin, user({ id: "b", role: "tenant_admin" })])).toBe(false);
    expect(isLastTenantAdmin(viewer, [viewer])).toBe(false);
  });
});

describe("isSelf", () => {
  it("matches email case-insensitively and handles missing identity", () => {
    const member = user({ email: "ada@example.test" });
    expect(isSelf(member, "ADA@example.test")).toBe(true);
    expect(isSelf(member, "other@example.test")).toBe(false);
    expect(isSelf(member, null)).toBe(false);
  });
});

describe("userInitials", () => {
  it.each([
    ["Ada Lovelace", "AL"],
    ["grace hopper", "GH"],
    ["Ada", "AD"],
    ["  ", "?"],
  ])("derives %s as %s", (name, initials) => {
    expect(userInitials(name)).toBe(initials);
  });
});
