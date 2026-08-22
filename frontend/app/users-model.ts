import type { JoinLink, User } from "./dashboard-types";

export type RoleFilter = User["role"] | "all";

export type InviteStatus = "active" | "expired" | "revoked" | "exhausted";

export const INVITE_TTL_OPTIONS = [
  { id: "day", seconds: 24 * 60 * 60 },
  { id: "week", seconds: 7 * 24 * 60 * 60 },
  { id: "month", seconds: 30 * 24 * 60 * 60 },
] as const;

export type InviteTtlId = (typeof INVITE_TTL_OPTIONS)[number]["id"];

export const DEFAULT_INVITE_TTL: InviteTtlId = "week";

const ROLE_ORDER: Record<User["role"], number> = {
  tenant_admin: 0,
  operator: 1,
  viewer: 2,
};

export function inviteStatus(link: JoinLink, nowMs: number): InviteStatus {
  if (link.revoked_at) {
    return "revoked";
  }
  const expiresMs = Date.parse(link.expires_at);
  if (Number.isFinite(expiresMs) && expiresMs <= nowMs) {
    return "expired";
  }
  if (link.used_count >= link.max_uses) {
    return "exhausted";
  }
  return "active";
}

export function sortJoinLinks(
  links: readonly JoinLink[],
  nowMs: number,
): JoinLink[] {
  return [...links].sort((a, b) => {
    const activeDiff =
      Number(inviteStatus(b, nowMs) === "active") -
      Number(inviteStatus(a, nowMs) === "active");
    if (activeDiff !== 0) {
      return activeDiff;
    }
    return Date.parse(b.created_at) - Date.parse(a.created_at);
  });
}

export function sortUsers(users: readonly User[]): User[] {
  return [...users].sort((a, b) => {
    const roleDiff = ROLE_ORDER[a.role] - ROLE_ORDER[b.role];
    if (roleDiff !== 0) {
      return roleDiff;
    }
    return (
      a.display_name.localeCompare(b.display_name) ||
      a.email.localeCompare(b.email)
    );
  });
}

export function filterUsers(
  users: readonly User[],
  query: string,
  role: RoleFilter,
): User[] {
  const normalized = query.trim().toLowerCase();
  return users.filter((user) => {
    if (role !== "all" && user.role !== role) {
      return false;
    }
    if (!normalized) {
      return true;
    }
    return (
      user.display_name.toLowerCase().includes(normalized) ||
      user.email.toLowerCase().includes(normalized)
    );
  });
}

export function countByRole(
  users: readonly User[],
): Record<User["role"], number> {
  const counts: Record<User["role"], number> = {
    tenant_admin: 0,
    operator: 0,
    viewer: 0,
  };
  for (const user of users) {
    counts[user.role] += 1;
  }
  return counts;
}

export function isLastTenantAdmin(user: User, users: readonly User[]): boolean {
  return (
    user.role === "tenant_admin" &&
    users.filter((candidate) => candidate.role === "tenant_admin").length <= 1
  );
}

export function isSelf(user: User, meEmail: string | null): boolean {
  return meEmail !== null && user.email.toLowerCase() === meEmail.toLowerCase();
}

export function userInitials(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) {
    return "?";
  }
  const initials =
    parts.length === 1
      ? parts[0].slice(0, 2)
      : `${parts[0][0]}${parts[parts.length - 1][0]}`;
  return initials.toUpperCase();
}
