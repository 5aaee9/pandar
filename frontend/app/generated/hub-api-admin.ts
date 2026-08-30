// Generated from contracts/hub-client.openapi.json. Do not edit.
import type { UserRole } from "./hub-api-core";

export type User = {
  id: string;
  tenant_id: string;
  email: string;
  display_name: string;
  role: UserRole;
  created_at: string;
};

export type UserIdentity = {
  id: string;
  tenant_id: string;
  user_id: string;
  provider: string;
  subject: string;
  created_at: string;
};

export type UserList = {
  users: Array<User>;
  identities: Array<UserIdentity>;
};

export type JoinLink = {
  id: string;
  tenant_id: string;
  role: UserRole;
  email_constraint: string | null;
  expires_at: string;
  max_uses: number;
  used_count: number;
  created_by_user_id: string | null;
  revoked_at: string | null;
  created_at: string;
};

export type JoinLinkList = {
  join_links: Array<JoinLink>;
};

export type TenantToken = {
  id: string;
  tenant_id: string;
  name: string;
  scopes: Array<string>;
  created_by_user_id: string | null;
  created_at: string;
  last_used_at: string | null;
  expires_at: string | null;
  revoked_at: string | null;
};

export type TenantTokenList = {
  tenant_tokens: Array<TenantToken>;
};

export type TenantTokenSecretResponse = {
  tenant_token: TenantToken;
  token: string;
};

export type JoinLinkSecretResponse = {
  join_link: JoinLink;
  token: string;
};

export type AuditEvent = {
  id: string;
  tenant_id: string;
  actor_type: string;
  user_id: string | null;
  action: string;
  target_type: string;
  target_id: string | null;
  metadata: Record<string, unknown>;
  created_at: string;
};

export type AuditEventList = {
  audit_events: Array<AuditEvent>;
};

export type RotatedTenantTokenSecretResponse = {
  tenant_token: TenantToken;
  token: string;
  rotated_from_token_id: string;
};
