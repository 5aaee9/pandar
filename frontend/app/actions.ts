"use server";

import { redirect } from "next/navigation";

import {
  agentsStatusUrl,
  apiUrl,
  commandUrl,
  errorCode,
  nullableField,
  postJson,
  statusUrl,
  statusUrlForForm,
  stringField,
} from "./action-helpers";
import { apiHeaders, requireAuth } from "./api-auth";
import { apiIdSegment } from "./api-path";
import type {
  LinkPrinterActionState,
  MutationActionState,
} from "./action-state";

export async function discoverPrinters(formData: FormData) {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const agentId = stringField(formData, "agent_id");
  const timeoutValue = stringField(formData, "timeout_seconds");
  const response = await fetch(
    `${apiUrl}/api/v1/tenants/${apiIdSegment(tenantId, "tenant_id")}/agents/${apiIdSegment(agentId, "agent_id")}/discover-printers`,
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
  redirect(commandUrl(command.id));
}

export async function refreshPrinters(formData: FormData) {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const agentId = stringField(formData, "agent_id");
  const response = await postJson(
    `/api/v1/tenants/${apiIdSegment(tenantId, "tenant_id")}/agents/${apiIdSegment(agentId, "agent_id")}/refresh-printers`,
    {},
  );
  redirect(
    statusUrlForForm(
      formData,
      response.ok ? "refresh_queued" : await errorCode(response),
    ),
  );
}

export async function refreshPrinterMaterials(
  _previousState: MutationActionState,
  formData: FormData,
): Promise<MutationActionState> {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const printerId = stringField(formData, "printer_id");
  const response = await postJson(
    `/api/v1/tenants/${apiIdSegment(tenantId, "tenant_id")}/printers/${apiIdSegment(printerId, "printer_id")}/materials:refresh`,
    {},
  );
  if (!response.ok) {
    return { ok: false, error: await errorCode(response) };
  }
  return { ok: true };
}

export async function deletePrinter(
  _previousState: MutationActionState,
  formData: FormData,
): Promise<MutationActionState> {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const printerId = stringField(formData, "printer_id");
  const response = await fetch(
    `${apiUrl}/api/v1/tenants/${apiIdSegment(tenantId, "tenant_id")}/printers/${apiIdSegment(printerId, "printer_id")}`,
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

export async function updatePrinter(
  _previousState: MutationActionState,
  formData: FormData,
): Promise<MutationActionState> {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const printerId = stringField(formData, "printer_id");
  const response = await fetch(
    `${apiUrl}/api/v1/tenants/${apiIdSegment(tenantId, "tenant_id")}/printers/${apiIdSegment(printerId, "printer_id")}`,
    {
      method: "PATCH",
      headers: await apiHeaders("application/json"),
      body: JSON.stringify({
        host: stringField(formData, "host"),
        access_code: stringField(formData, "access_code"),
        name: stringField(formData, "name"),
      }),
    },
  );
  if (!response.ok) {
    return { ok: false, error: await errorCode(response) };
  }
  return { ok: true };
}

export type AgentDeleteResult = {
  ok: boolean;
  redirectUrl: string;
};

export async function deleteAgent(
  formData: FormData,
): Promise<AgentDeleteResult> {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const agentId = stringField(formData, "agent_id");
  const response = await fetch(
    `${apiUrl}/api/v1/tenants/${apiIdSegment(tenantId, "tenant_id")}/agents/${apiIdSegment(agentId, "agent_id")}`,
    {
      method: "DELETE",
      headers: await apiHeaders("application/json"),
    },
  );
  return {
    ok: response.ok,
    redirectUrl: agentsStatusUrl(
      response.ok ? "agent_deleted" : await errorCode(response),
    ),
  };
}

export async function diagnosePrinter(formData: FormData) {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const agentId = stringField(formData, "agent_id");
  const response = await fetch(
    `${apiUrl}/api/v1/tenants/${apiIdSegment(tenantId, "tenant_id")}/agents/${apiIdSegment(agentId, "agent_id")}/diagnose-printer`,
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
  redirect(commandUrl(command.id));
}

export async function linkPrinter(
  _previousState: LinkPrinterActionState,
  formData: FormData,
): Promise<LinkPrinterActionState> {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const agentId = stringField(formData, "agent_id");
  const response = await postJson(
    `/api/v1/tenants/${apiIdSegment(tenantId, "tenant_id")}/agents/${apiIdSegment(agentId, "agent_id")}/link-printer`,
    {
      type: stringField(formData, "type"),
      host: stringField(formData, "host"),
      access_code: stringField(formData, "access_code"),
      name: nullableField(formData, "name"),
    },
  );
  if (!response.ok) {
    return { ok: false, error: await errorCode(response) };
  }
  const command = (await response.json()) as { id: string };
  return { ok: true, commandId: command.id };
}

export async function controlPrinter(
  _previousState: MutationActionState,
  formData: FormData,
): Promise<MutationActionState> {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const printerId = stringField(formData, "printer_id");
  const action = stringField(formData, "action");
  const speedMode = nullableField(formData, "speed_mode");
  const fanIndex = nullableField(formData, "fan_index");
  const speedPercent = nullableField(formData, "speed_percent");
  const airduct = nullableField(formData, "airduct");
  const amsId = nullableField(formData, "ams_id");
  const slotId = nullableField(formData, "slot_id");
  const globalTrayId = nullableField(formData, "global_tray_id");
  const externalId = nullableField(formData, "external_id");
  const durationHours = nullableField(formData, "duration_hours");
  const filament = nullableField(formData, "filament");
  const rotateTray = nullableField(formData, "rotate_tray");
  const holderAction = nullableField(formData, "holder_action");
  const nozzleId = nullableField(formData, "nozzle_id");
  const extruderId = nullableField(formData, "extruder_id");
  const temperatureCelsius = nullableField(formData, "temperature_celsius");
  const wait = nullableField(formData, "wait");
  const lightOn = nullableField(formData, "light_on");
  const axis = nullableField(formData, "axis");
  const deltaMm = nullableField(formData, "delta_mm");
  const feedrateMmPerMin = nullableField(formData, "feedrate_mm_per_min");
  const response = await postJson(
    `/api/v1/tenants/${apiIdSegment(tenantId, "tenant_id")}/printers/${apiIdSegment(printerId, "printer_id")}/controls`,
    {
      action,
      axes: action === "home" ? [] : undefined,
      movements:
        action === "move_axes"
          ? [{ axis: axis ?? "", delta_mm: Number(deltaMm) }]
          : undefined,
      feedrate_mm_per_min:
        action === "move_axes" && feedrateMmPerMin
          ? Number(feedrateMmPerMin)
          : undefined,
      speed_mode: speedMode ? Number(speedMode) : undefined,
      fan_index: fanIndex ? Number(fanIndex) : undefined,
      speed_percent: speedPercent ? Number(speedPercent) : undefined,
      airduct: airduct ? airduct === "true" || airduct === "on" : undefined,
      temperature_celsius: temperatureCelsius
        ? Number(temperatureCelsius)
        : undefined,
      wait: wait ? wait === "true" || wait === "on" : undefined,
      ams_id: amsId ? Number(amsId) : undefined,
      slot_id: slotId ? Number(slotId) : undefined,
      global_tray_id: globalTrayId ? Number(globalTrayId) : undefined,
      external_id: externalId || undefined,
      duration_hours: durationHours ? Number(durationHours) : undefined,
      filament: filament || undefined,
      rotate_tray: rotateTray
        ? rotateTray === "true" || rotateTray === "on"
        : undefined,
      holder_action: holderAction ? Number(holderAction) : undefined,
      nozzle_id: nozzleId ? Number(nozzleId) : undefined,
      extruder_id: extruderId ? Number(extruderId) : undefined,
      light_on: lightOn ? lightOn === "true" || lightOn === "on" : undefined,
    },
  );
  if (!response.ok) {
    return { ok: false, error: await errorCode(response) };
  }
  return { ok: true };
}

export async function createPluginTicket(formData: FormData) {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const redirectUrl = stringField(formData, "redirect_url");
  const response = await postJson(
    `/api/v1/tenants/${apiIdSegment(tenantId, "tenant_id")}/plugin/login-tickets`,
    {
      redirect_url: redirectUrl,
    },
  );
  if (!response.ok) {
    redirect(statusUrl(await errorCode(response)));
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

export async function createMobileTicket(formData: FormData) {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const redirectUrl = stringField(formData, "redirect_url");
  const codeChallenge = stringField(formData, "code_challenge");
  const state = stringField(formData, "state");
  const response = await postJson(
    `/api/v1/tenants/${apiIdSegment(tenantId, "tenant_id")}/mobile/login-tickets`,
    {
      redirect_url: redirectUrl,
      code_challenge: codeChallenge,
    },
  );
  if (!response.ok) {
    redirect(statusUrl(await errorCode(response)));
  }
  const body = (await response.json()) as {
    ticket: string;
    redirect_url: string;
  };
  const url = new URL(body.redirect_url);
  url.searchParams.set("ticket", body.ticket);
  url.searchParams.set("redirect_url", body.redirect_url);
  url.searchParams.set("state", state);
  redirect(url.toString());
}
