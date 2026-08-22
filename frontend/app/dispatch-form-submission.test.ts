import { describe, expect, it, vi } from "vitest";

import {
  prepareDispatchSubmission,
  reprintRequestBody,
} from "./dispatch-form-submission";

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

  it("serializes all editable print options for reprint", () => {
    const data = formData();
    data.set("plate_id", "2");
    data.set("bed_leveling", "false");
    data.set("auto_bed_leveling", "2");
    data.set("flow_cali", "true");
    data.set("auto_flow_cali", "1");
    data.set("auto_offset_cali", "0");
    data.set("timelapse", "false");
    data.set("ams_mapping", "[4,0]");
    data.set(
      "ams_mapping2",
      '[{"ams_id":1,"slot_id":0},{"ams_id":0,"slot_id":0}]',
    );
    data.set(
      "ams_mapping_info",
      '[{"ams":4,"filamentType":"PLA","nozzleId":1}]',
    );

    expect(reprintRequestBody(data)).toEqual({
      reason: null,
      printer_id: "printer-1",
      plate_id: 2,
      use_ams: true,
      bed_leveling: false,
      auto_bed_leveling: 2,
      flow_cali: true,
      auto_flow_cali: 1,
      auto_offset_cali: 0,
      timelapse: false,
      ams_mapping: [4, 0],
      ams_mapping2: [
        { ams_id: 1, slot_id: 0 },
        { ams_id: 0, slot_id: 0 },
      ],
      ams_mapping_info: [{ ams: 4, filamentType: "PLA", nozzleId: 1 }],
    });
  });

  it("explicitly clears inherited mappings when the selected plate has none", () => {
    const data = formData();
    data.set("plate_id", "1");
    data.set("bed_leveling", "true");
    data.set("auto_bed_leveling", "2");
    data.set("flow_cali", "false");
    data.set("auto_flow_cali", "2");
    data.set("auto_offset_cali", "0");
    data.set("timelapse", "false");

    expect(reprintRequestBody(data)).toMatchObject({
      ams_mapping: [],
      ams_mapping2: [],
      ams_mapping_info: [],
    });
  });
});
