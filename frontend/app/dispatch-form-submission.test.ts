import { describe, expect, it, vi } from "vitest";

import { prepareDispatchSubmission } from "./dispatch-form-submission";

function formData() {
  const data = new FormData();
  data.set("printer_id", "printer-1");
  data.set("use_ams", "true");
  data.set("material_mapping_valid", "true");
  data.set("material_mapping_uses_ams", "false");
  data.set("external_material_mismatch", "false");
  return data;
}

describe("prepareDispatchSubmission", () => {
  it("normalizes derived AMS use and removes internal fields", () => {
    const confirm = vi.fn(() => true);
    const prepared = prepareDispatchSubmission(formData(), confirm);

    expect(prepared?.printerId).toBe("printer-1");
    expect(prepared?.formData.get("use_ams")).toBe("false");
    expect(prepared?.formData.get("material_mapping_valid")).toBeNull();
    expect(prepared?.formData.get("material_mapping_uses_ams")).toBeNull();
    expect(prepared?.formData.get("external_material_mismatch")).toBeNull();
    expect(confirm).not.toHaveBeenCalled();
  });

  it("rejects an invalid material mapping", () => {
    const data = formData();
    data.set("material_mapping_valid", "false");
    const confirm = vi.fn(() => true);

    expect(prepareDispatchSubmission(data, confirm)).toBeNull();
    expect(confirm).not.toHaveBeenCalled();
  });

  it("requires confirmation for an external material mismatch", () => {
    const cancelled = formData();
    cancelled.set("external_material_mismatch", "true");
    const reject = vi.fn(() => false);
    expect(prepareDispatchSubmission(cancelled, reject)).toBeNull();
    expect(reject).toHaveBeenCalledOnce();

    const accepted = formData();
    accepted.set("external_material_mismatch", "true");
    const confirm = vi.fn(() => true);
    const prepared = prepareDispatchSubmission(accepted, confirm);
    expect(prepared?.printerId).toBe("printer-1");
    expect(prepared?.formData.get("external_material_mismatch")).toBeNull();
    expect(confirm).toHaveBeenCalledOnce();
  });
});
