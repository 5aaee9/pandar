export const TENANT_COOKIE = "pandar.tenant";

export function setTenantCookie(tenantId: string) {
  document.cookie = `${TENANT_COOKIE}=${encodeURIComponent(tenantId)}; path=/; max-age=31536000; samesite=lax`;
}
