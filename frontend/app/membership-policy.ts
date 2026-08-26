export type MembershipResult = {
  role: "tenant_admin" | "operator" | "viewer" | null;
  error: string | null;
};

/**
 * Whether the current principal may enqueue printer control / job actions.
 * Unknown membership (load failure or no membership) fails closed.
 */
export function canManageJobs(
  provider: string,
  membership: MembershipResult,
): boolean {
  return (
    provider === "none" ||
    (membership.error === null &&
      membership.role !== null &&
      membership.role !== "viewer")
  );
}

/** Whether admin surfaces are off-limits; unknown membership fails closed. */
export function adminAccessUnavailable(
  provider: string,
  membership: MembershipResult,
): boolean {
  return provider !== "none" && membership.role !== "tenant_admin";
}

/** Whether the membership itself failed to load, as opposed to being restricted. */
export function adminAccessLoadError(
  provider: string,
  membership: MembershipResult,
): boolean {
  return provider !== "none" && membership.error !== null;
}
