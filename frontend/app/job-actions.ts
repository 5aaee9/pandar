"use server";

import {
  errorCode,
  nullableField,
  postJson,
  statusUrlForForm,
  stringField,
} from "./action-helpers";
import { requireAuth } from "./api-auth";
import { apiIdSegment } from "./api-path";

export type JobMutationResult = {
  ok: boolean;
  redirectUrl: string;
};

export async function retryDispatchJob(
  formData: FormData,
): Promise<JobMutationResult> {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const jobId = stringField(formData, "job_id");
  const response = await postJson(
    `/api/v1/tenants/${apiIdSegment(tenantId, "tenant_id")}/jobs/${apiIdSegment(jobId, "job_id")}/retry-dispatch`,
    {
      reason: nullableField(formData, "reason"),
    },
  );
  return {
    ok: response.ok,
    redirectUrl: statusUrlForForm(
      formData,
      response.ok ? "retry_queued" : await errorCode(response),
    ),
  };
}
