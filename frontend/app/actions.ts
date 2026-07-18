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
  redirect(commandUrl(tenantId, command.id));
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
    `/api/v1/tenants/${apiIdSegment(tenantId, "tenant_id")}/printers/${apiIdSegment(printerId, "printer_id")}/materials:refresh`,
    {},
  );
  redirect(
    statusUrl(
      tenantId,
      response.ok ? "materials_refresh_queued" : await errorCode(response),
    ),
  );
}

export async function deletePrinter(formData: FormData) {
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
  redirect(
    statusUrl(
      tenantId,
      response.ok ? "printer_deleted" : await errorCode(response),
    ),
  );
}

export async function updatePrinter(formData: FormData) {
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
    redirect(statusUrl(tenantId, await errorCode(response)));
  }
  redirect(statusUrl(tenantId, "printer_updated"));
}

export async function deleteAgent(formData: FormData) {
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
  redirect(commandUrl(tenantId, command.id));
}

export async function linkPrinter(formData: FormData) {
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
    redirect(agentsStatusUrl(tenantId, await errorCode(response)));
  }
  const command = (await response.json()) as { id: string };
  redirect(commandUrl(tenantId, command.id));
}

export async function controlPrinter(formData: FormData) {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const printerId = stringField(formData, "printer_id");
  const action = stringField(formData, "action");
  const speedMode = nullableField(formData, "speed_mode");
  const amsId = nullableField(formData, "ams_id");
  const slotId = nullableField(formData, "slot_id");
  const globalTrayId = nullableField(formData, "global_tray_id");
  const externalId = nullableField(formData, "external_id");
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
      temperature_celsius: temperatureCelsius
        ? Number(temperatureCelsius)
        : undefined,
      wait: wait ? wait === "true" || wait === "on" : undefined,
      ams_id: amsId ? Number(amsId) : undefined,
      slot_id: slotId ? Number(slotId) : undefined,
      global_tray_id: globalTrayId ? Number(globalTrayId) : undefined,
      external_id: externalId || undefined,
      extruder_id: extruderId ? Number(extruderId) : undefined,
      light_on: lightOn ? lightOn === "true" || lightOn === "on" : undefined,
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
  const tenantId = stringField(formData, "tenant_id");
  const redirectUrl = stringField(formData, "redirect_url");
  const response = await postJson(
    `/api/v1/tenants/${apiIdSegment(tenantId, "tenant_id")}/plugin/login-tickets`,
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

export async function createMobileTicket(formData: FormData) {
  const tenantId = stringField(formData, "tenant_id");
  const redirectUrl = stringField(formData, "redirect_url");
  const response = await postJson(
    `/api/v1/tenants/${apiIdSegment(tenantId, "tenant_id")}/mobile/login-tickets`,
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
