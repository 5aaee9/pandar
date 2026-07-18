"use server";

import { redirect } from "next/navigation";

import {
  errorCode,
  nullableField,
  optionalBoolean,
  postJson,
  statusUrlForForm,
  stringField,
} from "./action-helpers";
import { requireAuth } from "./api-auth";
import { apiIdSegment } from "./api-path";

export async function retryDispatchJob(formData: FormData) {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const jobId = stringField(formData, "job_id");
  const response = await postJson(
    `/api/v1/tenants/${apiIdSegment(tenantId, "tenant_id")}/jobs/${apiIdSegment(jobId, "job_id")}/retry-dispatch`,
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
      postJson(
        `/api/v1/tenants/${apiIdSegment(tenantId, "tenant_id")}/jobs/${apiIdSegment(jobId, "job_id")}/retry-dispatch`,
        {
          reason: null,
        },
      ),
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
    `/api/v1/tenants/${apiIdSegment(tenantId, "tenant_id")}/jobs/${apiIdSegment(jobId, "job_id")}/reprint`,
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
    `/api/v1/tenants/${apiIdSegment(tenantId, "tenant_id")}/jobs/${apiIdSegment(jobId, "job_id")}/duplicate`,
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
