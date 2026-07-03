"use server";

import { redirect } from "next/navigation";

import { apiHeaders, requireAuth } from "./api-auth";
import type { Agent, JoinLink, Tenant, TenantToken } from "./dashboard-types";

const apiUrl = process.env.APP_API_URL ?? "http://localhost:8080";

export type SecretActionState =
  | {
      ok: true;
      kind: "tenant_token";
      token: string;
      tenantToken: TenantToken;
      message: string;
    }
  | {
      ok: true;
      kind: "agent_pairing";
      agentEnv: string;
      agent: Agent;
      message: string;
    }
  | {
      ok: true;
      kind: "join_link";
      joinLink: JoinLink;
      token: string;
      message: string;
    }
  | {
      ok: false;
      error: string;
    }
  | null;

export async function discoverPrinters(formData: FormData) {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const agentId = stringField(formData, "agent_id");
  const timeoutValue = stringField(formData, "timeout_seconds");
  const response = await fetch(
    `${apiUrl}/api/v1/tenants/${tenantId}/agents/${agentId}/discover-printers`,
    {
      method: "POST",
      headers: await apiHeaders("application/json"),
      body: JSON.stringify({
        timeout_seconds: Number(timeoutValue || "5"),
      }),
    },
  );

  if (!response.ok) {
    throw new Error(`Discover printers returned ${response.status}`);
  }

  const command = (await response.json()) as { id: string };
  redirect(commandUrl(tenantId, command.id));
}

export async function refreshPrinters(formData: FormData) {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const agentId = stringField(formData, "agent_id");
  const response = await postJson(
    `/api/v1/tenants/${tenantId}/agents/${agentId}/refresh-printers`,
    {},
  );
  redirect(
    statusUrlForForm(
      formData,
      tenantId,
      response.ok ? "refresh_queued" : await errorCode(response),
    ),
  );
}

export async function refreshPrinterMaterials(formData: FormData) {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const printerId = stringField(formData, "printer_id");
  const response = await postJson(
    `/api/v1/tenants/${tenantId}/printers/${printerId}/materials:refresh`,
    {},
  );
  redirect(
    statusUrl(
      tenantId,
      response.ok ? "materials_refresh_queued" : await errorCode(response),
    ),
  );
}

export async function refreshAllAgents(formData: FormData) {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const agentIds = formData
    .getAll("agent_id")
    .filter((value): value is string => typeof value === "string");
  const responses = await Promise.all(
    agentIds.map((agentId) =>
      postJson(
        `/api/v1/tenants/${tenantId}/agents/${agentId}/refresh-printers`,
        {},
      ),
    ),
  );
  const allOk = responses.every((response) => response.ok);
  redirect(
    statusUrlForForm(
      formData,
      tenantId,
      allOk ? "refresh_queued" : "refresh_partial",
    ),
  );
}

export async function deleteAgent(formData: FormData) {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const agentId = stringField(formData, "agent_id");
  const response = await fetch(
    `${apiUrl}/api/v1/tenants/${tenantId}/agents/${agentId}`,
    {
      method: "DELETE",
      headers: await apiHeaders("application/json"),
    },
  );
  redirect(
    agentsStatusUrl(
      tenantId,
      response.ok ? "agent_deleted" : await errorCode(response),
    ),
  );
}

export async function diagnosePrinter(formData: FormData) {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const agentId = stringField(formData, "agent_id");
  const response = await fetch(
    `${apiUrl}/api/v1/tenants/${tenantId}/agents/${agentId}/diagnose-printer`,
    {
      method: "POST",
      headers: await apiHeaders("application/json"),
      body: JSON.stringify({
        serial_number: stringField(formData, "serial_number"),
      }),
    },
  );

  if (!response.ok) {
    throw new Error(`Diagnose printer returned ${response.status}`);
  }

  const command = (await response.json()) as { id: string };
  redirect(commandUrl(tenantId, command.id));
}

export async function linkPrinter(formData: FormData) {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const agentId = stringField(formData, "agent_id");
  const response = await postJson(
    `/api/v1/tenants/${tenantId}/agents/${agentId}/link-printer`,
    {
      type: stringField(formData, "type"),
      host: stringField(formData, "host"),
      access_code: stringField(formData, "access_code"),
      name: nullableField(formData, "name"),
    },
  );
  if (!response.ok) {
    redirect(agentsStatusUrl(tenantId, await errorCode(response)));
  }
  const command = (await response.json()) as { id: string };
  redirect(commandUrl(tenantId, command.id));
}

export async function createTenantToken(
  _previousState: SecretActionState,
  formData: FormData,
): Promise<SecretActionState> {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const scopes = stringField(formData, "scopes").split(",").flatMap((scope) => {
    const trimmed = scope.trim();
    return trimmed ? [trimmed] : [];
  });
  const response = await postJson(`/api/v1/tenants/${tenantId}/tenant-tokens`, {
    name: stringField(formData, "name"),
    scopes,
    expires_at: nullableField(formData, "expires_at"),
  });
  if (!response.ok) {
    return { ok: false, error: await errorCode(response) };
  }
  const body = (await response.json()) as {
    tenant_token: TenantToken;
    token: string;
  };
  return {
    ok: true,
    kind: "tenant_token",
    tenantToken: body.tenant_token,
    token: body.token,
    message: "Tenant token created",
  };
}

export async function revokeTenantToken(formData: FormData) {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const tokenId = stringField(formData, "token_id");
  const response = await fetch(
    `${apiUrl}/api/v1/tenants/${tenantId}/tenant-tokens/${tokenId}`,
    {
      method: "DELETE",
      headers: await apiHeaders("application/json"),
    },
  );
  redirect(
    statusUrl(
      tenantId,
      response.ok ? "tenant_token_revoked" : await errorCode(response),
    ),
  );
}

export async function rotateTenantToken(
  _previousState: SecretActionState,
  formData: FormData,
): Promise<SecretActionState> {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const tokenId = stringField(formData, "token_id");
  const response = await postJson(
    `/api/v1/tenants/${tenantId}/tenant-tokens/${tokenId}/rotate`,
    {
      expires_at: nullableField(formData, "expires_at"),
    },
  );
  if (!response.ok) {
    return { ok: false, error: await errorCode(response) };
  }
  const body = (await response.json()) as {
    tenant_token: TenantToken;
    token: string;
  };
  return {
    ok: true,
    kind: "tenant_token",
    tenantToken: body.tenant_token,
    token: body.token,
    message: "Tenant token rotated",
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
  const body = (await response.json()) as { tenant: Tenant };
  redirect(
    `/?tenant=${encodeURIComponent(body.tenant.id)}&status=tenant_created`,
  );
}

export async function createJoinLink(
  _previousState: SecretActionState,
  formData: FormData,
): Promise<SecretActionState> {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const response = await postJson(`/api/v1/tenants/${tenantId}/join-links`, {
    role: stringField(formData, "role"),
    email_constraint: nullableField(formData, "email_constraint"),
    expires_in_seconds: numberOrNull(formData, "expires_in_seconds"),
    max_uses: numberOrNull(formData, "max_uses"),
  });
  if (!response.ok) {
    return { ok: false, error: await errorCode(response) };
  }
  const body = (await response.json()) as {
    join_link: JoinLink;
    token: string;
  };
  return {
    ok: true,
    kind: "join_link",
    joinLink: body.join_link,
    token: body.token,
    message: "Join link created",
  };
}

export async function revokeJoinLink(formData: FormData) {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const joinLinkId = stringField(formData, "join_link_id");
  const response = await fetch(
    `${apiUrl}/api/v1/tenants/${tenantId}/join-links/${joinLinkId}`,
    {
      method: "DELETE",
      headers: await apiHeaders("application/json"),
    },
  );
  redirect(
    statusUrl(
      tenantId,
      response.ok ? "join_link_revoked" : await errorCode(response),
    ),
  );
}

export async function acceptJoinLink(formData: FormData) {
  await requireAuth();
  const response = await postJson("/api/v1/join-links/accept", {
    token: stringField(formData, "token"),
  });
  if (!response.ok) {
    redirect(`/?status=${encodeURIComponent(await errorCode(response))}`);
  }
  const body = (await response.json()) as { tenant: Tenant };
  redirect(
    `/?tenant=${encodeURIComponent(body.tenant.id)}&status=join_link_accepted`,
  );
}

export async function createTenantUser(formData: FormData) {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const response = await postJson(`/api/v1/tenants/${tenantId}/users`, {
    email: stringField(formData, "email"),
    display_name: stringField(formData, "display_name"),
    role: stringField(formData, "role"),
  });
  redirect(
    statusUrl(
      tenantId,
      response.ok ? "user_created" : await errorCode(response),
    ),
  );
}

export async function updateTenantUserRole(formData: FormData) {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const userId = stringField(formData, "user_id");
  const response = await fetch(
    `${apiUrl}/api/v1/tenants/${tenantId}/users/${userId}/role`,
    {
      method: "PATCH",
      headers: await apiHeaders("application/json"),
      body: JSON.stringify({ role: stringField(formData, "role") }),
    },
  );
  redirect(
    statusUrl(
      tenantId,
      response.ok ? "user_role_updated" : await errorCode(response),
    ),
  );
}

export async function linkUserIdentity(formData: FormData) {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const userId = stringField(formData, "user_id");
  const response = await postJson(
    `/api/v1/tenants/${tenantId}/users/${userId}/identities`,
    {
      provider: stringField(formData, "provider"),
      subject: stringField(formData, "subject"),
    },
  );
  redirect(
    statusUrl(
      tenantId,
      response.ok ? "identity_linked" : await errorCode(response),
    ),
  );
}

export async function createAgentPairing(
  _previousState: SecretActionState,
  formData: FormData,
): Promise<SecretActionState> {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const response = await postJson(
    `/api/v1/tenants/${tenantId}/agent-pairings`,
    {
      name: stringField(formData, "name"),
    },
  );
  if (!response.ok) {
    return { ok: false, error: await errorCode(response) };
  }
  const body = (await response.json()) as { agent: Agent; agent_env: string };
  return {
    ok: true,
    kind: "agent_pairing",
    agent: body.agent,
    agentEnv: body.agent_env,
    message: "Agent pairing created",
  };
}

export async function retryDispatchJob(formData: FormData) {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const jobId = stringField(formData, "job_id");
  const response = await postJson(
    `/api/v1/tenants/${tenantId}/jobs/${jobId}/retry-dispatch`,
    {
      reason: nullableField(formData, "reason"),
    },
  );
  redirect(
    statusUrlForForm(
      formData,
      tenantId,
      response.ok ? "retry_queued" : await errorCode(response),
    ),
  );
}

export async function retryDispatchJobs(formData: FormData) {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const jobIds = formData
    .getAll("job_id")
    .filter((value): value is string => typeof value === "string");
  const responses = await Promise.all(
    jobIds.map((jobId) =>
      postJson(`/api/v1/tenants/${tenantId}/jobs/${jobId}/retry-dispatch`, {
        reason: null,
      }),
    ),
  );
  const allOk = responses.every((response) => response.ok);
  redirect(
    statusUrlForForm(
      formData,
      tenantId,
      allOk ? "retry_queued" : "retry_partial",
    ),
  );
}

export async function reprintJob(formData: FormData) {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const jobId = stringField(formData, "job_id");
  const response = await postJson(
    `/api/v1/tenants/${tenantId}/jobs/${jobId}/reprint`,
    {
      reason: nullableField(formData, "reason"),
    },
  );
  redirect(
    statusUrlForForm(
      formData,
      tenantId,
      response.ok ? "reprint_queued" : await errorCode(response),
    ),
  );
}

export async function duplicateJob(formData: FormData) {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const jobId = stringField(formData, "job_id");
  const plateId = nullableField(formData, "plate_id");
  const response = await postJson(
    `/api/v1/tenants/${tenantId}/jobs/${jobId}/duplicate`,
    {
      printer_id: nullableField(formData, "printer_id"),
      plate_id: plateId ? Number(plateId) : null,
      use_ams: optionalBoolean(formData, "use_ams"),
      flow_cali: optionalBoolean(formData, "flow_cali"),
      timelapse: optionalBoolean(formData, "timelapse"),
      ams_mapping: null,
      ams_mapping2: null,
    },
  );
  redirect(
    statusUrlForForm(
      formData,
      tenantId,
      response.ok ? "duplicate_queued" : await errorCode(response),
    ),
  );
}

export async function controlPrinter(formData: FormData) {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const printerId = stringField(formData, "printer_id");
  const action = stringField(formData, "action");
  const speedMode = nullableField(formData, "speed_mode");
  const response = await postJson(
    `/api/v1/tenants/${tenantId}/printers/${printerId}/controls`,
    {
      action,
      speed_mode: speedMode ? Number(speedMode) : undefined,
    },
  );
  redirect(
    statusUrlForForm(
      formData,
      tenantId,
      response.ok ? "printer_control_queued" : await errorCode(response),
    ),
  );
}

export async function createPluginTicket(formData: FormData) {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const redirectUrl = stringField(formData, "redirect_url");
  const response = await postJson(
    `/api/v1/tenants/${tenantId}/plugin/login-tickets`,
    {
      redirect_url: redirectUrl,
    },
  );
  if (!response.ok) {
    redirect(statusUrl(tenantId, await errorCode(response)));
  }
  const body = (await response.json()) as {
    ticket: string;
    redirect_url: string;
  };
  const url = new URL(body.redirect_url);
  url.searchParams.set("ticket", body.ticket);
  url.searchParams.set("redirect_url", body.redirect_url);
  redirect(url.toString());
}

async function postJson(path: string, body: unknown) {
  return fetch(`${apiUrl}${path}`, {
    method: "POST",
    headers: await apiHeaders("application/json"),
    body: JSON.stringify(body),
  });
}

function stringField(formData: FormData, name: string) {
  const value = formData.get(name);
  return typeof value === "string" ? value : "";
}

function nullableField(formData: FormData, name: string) {
  const value = stringField(formData, name).trim();
  return value.length > 0 ? value : null;
}

function optionalBoolean(formData: FormData, name: string) {
  return formData.has(name) ? formData.get(name) === "on" : null;
}

function numberOrNull(formData: FormData, name: string) {
  const value = nullableField(formData, name);
  return value ? Number(value) : null;
}

async function errorCode(response: Response) {
  try {
    const body = (await response.json()) as { error?: string };
    return body.error ?? `http_${response.status}`;
  } catch {
    return `http_${response.status}`;
  }
}

function statusUrlForForm(formData: FormData, tenantId: string, status: string) {
  return statusUrl(tenantId, status, stringField(formData, "return_to"));
}

function statusUrl(tenantId: string, status: string, returnTo?: string) {
  const view = returnTo === "jobs" ? "jobs" : "devices";
  return `/${view}?tenant=${encodeURIComponent(tenantId)}&status=${encodeURIComponent(status)}`;
}

function agentsStatusUrl(tenantId: string, status: string) {
  return `/agents?tenant=${encodeURIComponent(tenantId)}&status=${encodeURIComponent(status)}`;
}

function commandUrl(tenantId: string, commandId: string) {
  return `/agents?tenant=${encodeURIComponent(tenantId)}&command=${encodeURIComponent(commandId)}`;
}
