"use server";

import { refresh } from "next/cache";
import { cookies } from "next/headers";
import { redirect } from "next/navigation";

import {
  apiUrl,
  errorCode,
  nullableField,
  numberOrNull,
  postJson,
  stringField,
} from "./action-helpers";
import type { MutationActionState, SecretActionState } from "./action-state";
import { apiHeaders, requireAuth } from "./api-auth";
import { apiIdSegment } from "./api-path";
import { TENANT_COOKIE } from "./tenant-cookie";
import { decodeHubResponse } from "./hub-contract";

async function selectTenant(tenantId: string) {
  (await cookies()).set(TENANT_COOKIE, tenantId, {
    path: "/",
    maxAge: 60 * 60 * 24 * 365,
    sameSite: "lax",
  });
}

export async function createTenantToken(
  _previousState: SecretActionState,
  formData: FormData,
): Promise<SecretActionState> {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const scopes = stringField(formData, "scopes")
    .split(",")
    .flatMap((scope) => {
      const trimmed = scope.trim();
      return trimmed ? [trimmed] : [];
    });
  const response = await postJson(
    `/api/v1/tenants/${apiIdSegment(tenantId, "tenant_id")}/tenant-tokens`,
    {
      name: stringField(formData, "name"),
      scopes,
      expires_at: nullableField(formData, "expires_at"),
    },
  );
  if (!response.ok) {
    return { ok: false, error: await errorCode(response) };
  }
  const body = decodeHubResponse(
    "TenantTokenSecretResponse",
    await response.json(),
  );
  refresh();
  return {
    ok: true,
    kind: "tenant_token",
    operation: "created",
    tenantToken: body.tenant_token,
    token: body.token,
  };
}

export async function revokeTenantToken(
  _previousState: MutationActionState,
  formData: FormData,
): Promise<MutationActionState> {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const tokenId = stringField(formData, "token_id");
  const response = await fetch(
    `${apiUrl}/api/v1/tenants/${apiIdSegment(tenantId, "tenant_id")}/tenant-tokens/${apiIdSegment(tokenId, "token_id")}`,
    {
      method: "DELETE",
      headers: await apiHeaders("application/json"),
    },
  );
  if (!response.ok) {
    return { ok: false, error: await errorCode(response) };
  }
  return { ok: true };
}

export async function rotateTenantToken(
  _previousState: SecretActionState,
  formData: FormData,
): Promise<SecretActionState> {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const tokenId = stringField(formData, "token_id");
  const response = await postJson(
    `/api/v1/tenants/${apiIdSegment(tenantId, "tenant_id")}/tenant-tokens/${apiIdSegment(tokenId, "token_id")}/rotate`,
    {
      expires_at: nullableField(formData, "expires_at"),
    },
  );
  if (!response.ok) {
    return { ok: false, error: await errorCode(response) };
  }
  const body = decodeHubResponse(
    "RotatedTenantTokenSecretResponse",
    await response.json(),
  );
  refresh();
  return {
    ok: true,
    kind: "tenant_token",
    operation: "rotated",
    tenantToken: body.tenant_token,
    token: body.token,
  };
}

export async function createTenantFromExternal(formData: FormData) {
  await requireAuth();
  const response = await postJson("/api/v1/onboarding/tenants", {
    slug: stringField(formData, "slug"),
    display_name: stringField(formData, "display_name"),
  });
  if (!response.ok) {
    redirect(`/?status=${encodeURIComponent(await errorCode(response))}`);
  }
  const body = decodeHubResponse(
    "CreatedTenantResponse",
    await response.json(),
  );
  await selectTenant(body.tenant.id);
  const { revalidatePath } = await import("next/cache");
  revalidatePath("/(dashboard)", "layout");
  redirect("/?status=tenant_created");
}

export async function createJoinLink(
  _previousState: SecretActionState,
  formData: FormData,
): Promise<SecretActionState> {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const response = await postJson(
    `/api/v1/tenants/${apiIdSegment(tenantId, "tenant_id")}/join-links`,
    {
      role: stringField(formData, "role"),
      email_constraint: nullableField(formData, "email_constraint"),
      expires_in_seconds: numberOrNull(formData, "expires_in_seconds"),
      max_uses: numberOrNull(formData, "max_uses"),
    },
  );
  if (!response.ok) {
    return { ok: false, error: await errorCode(response) };
  }
  const body = decodeHubResponse(
    "JoinLinkSecretResponse",
    await response.json(),
  );
  return {
    ok: true,
    kind: "join_link",
    joinLink: body.join_link,
    token: body.token,
    message: "Join link created",
  };
}

export async function revokeJoinLink(
  _previousState: MutationActionState,
  formData: FormData,
): Promise<MutationActionState> {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const joinLinkId = stringField(formData, "join_link_id");
  const response = await fetch(
    `${apiUrl}/api/v1/tenants/${apiIdSegment(tenantId, "tenant_id")}/join-links/${apiIdSegment(joinLinkId, "join_link_id")}`,
    {
      method: "DELETE",
      headers: await apiHeaders("application/json"),
    },
  );
  if (!response.ok) {
    return { ok: false, error: await errorCode(response) };
  }
  return { ok: true };
}

export async function acceptJoinLink(formData: FormData) {
  await requireAuth();
  const response = await postJson("/api/v1/join-links/accept", {
    token: stringField(formData, "token"),
  });
  if (!response.ok) {
    redirect(`/?status=${encodeURIComponent(await errorCode(response))}`);
  }
  const body = decodeHubResponse(
    "AcceptedJoinLinkResponse",
    await response.json(),
  );
  await selectTenant(body.tenant.id);
  const { revalidatePath } = await import("next/cache");
  revalidatePath("/(dashboard)", "layout");
  redirect("/?status=join_link_accepted");
}

export async function updateTenantDisplayName(
  _previousState: MutationActionState,
  formData: FormData,
): Promise<MutationActionState> {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const displayName = stringField(formData, "display_name").trim();
  if (!displayName) {
    return { ok: false, error: "bad_request" };
  }
  const response = await fetch(
    `${apiUrl}/api/v1/tenants/${apiIdSegment(tenantId, "tenant_id")}`,
    {
      method: "PATCH",
      headers: await apiHeaders("application/json"),
      body: JSON.stringify({ display_name: displayName }),
    },
  );
  if (!response.ok) {
    return { ok: false, error: await errorCode(response) };
  }
  refresh();
  return { ok: true };
}

export async function updateTenantUserRole(
  _previousState: MutationActionState,
  formData: FormData,
): Promise<MutationActionState> {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const userId = stringField(formData, "user_id");
  const response = await fetch(
    `${apiUrl}/api/v1/tenants/${apiIdSegment(tenantId, "tenant_id")}/users/${apiIdSegment(userId, "user_id")}/role`,
    {
      method: "PATCH",
      headers: await apiHeaders("application/json"),
      body: JSON.stringify({ role: stringField(formData, "role") }),
    },
  );
  if (!response.ok) {
    return { ok: false, error: await errorCode(response) };
  }
  return { ok: true };
}

export async function removeTenantUser(
  _previousState: MutationActionState,
  formData: FormData,
): Promise<MutationActionState> {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const userId = stringField(formData, "user_id");
  const response = await fetch(
    `${apiUrl}/api/v1/tenants/${apiIdSegment(tenantId, "tenant_id")}/users/${apiIdSegment(userId, "user_id")}`,
    {
      method: "DELETE",
      headers: await apiHeaders("application/json"),
    },
  );
  if (!response.ok) {
    return { ok: false, error: await errorCode(response) };
  }
  return { ok: true };
}

export async function createAgentPairing(
  _previousState: SecretActionState,
  formData: FormData,
): Promise<SecretActionState> {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const response = await postJson(
    `/api/v1/tenants/${apiIdSegment(tenantId, "tenant_id")}/agent-pairings`,
    {
      name: stringField(formData, "name"),
    },
  );
  if (!response.ok) {
    return { ok: false, error: await errorCode(response) };
  }
  const body = decodeHubResponse("AgentPairingResponse", await response.json());
  return {
    ok: true,
    kind: "agent_pairing",
    agent: body.agent,
    agentEnv: body.agent_env,
    message: "Agent pairing created",
  };
}
