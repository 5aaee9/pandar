export function prepareDispatchSubmission(
  formData: FormData,
  confirmExternalMismatch: () => boolean,
) {
  const printerId = String(formData.get("printer_id") ?? "");
  const externalMaterialMismatch =
    formData.get("external_material_mismatch") === "true";
  const mappingValid = formData.get("material_mapping_valid") !== "false";
  formData.delete("material_mapping_valid");
  if (!mappingValid) return null;

  const mappingUsesAms = formData.get("material_mapping_uses_ams");
  formData.delete("material_mapping_uses_ams");
  if (mappingUsesAms !== null) {
    formData.delete("use_ams");
    formData.set("use_ams", String(mappingUsesAms));
  }

  formData.delete("external_material_mismatch");
  if (externalMaterialMismatch && !confirmExternalMismatch()) return null;
  return { formData, printerId };
}

export async function dispatchErrorCode(response: Response) {
  try {
    const body = (await response.json()) as { error?: string };
    return body.error ?? `http_${response.status}`;
  } catch {
    return `http_${response.status}`;
  }
}

export function reprintRequestBody(formData: FormData) {
  return {
    reason: null,
    printer_id: String(formData.get("printer_id")),
    plate_id: Number(formData.get("plate_id")),
    use_ams: booleanField(formData, "use_ams"),
    bed_leveling: booleanField(formData, "bed_leveling"),
    auto_bed_leveling: Number(formData.get("auto_bed_leveling")),
    flow_cali: booleanField(formData, "flow_cali"),
    auto_flow_cali: Number(formData.get("auto_flow_cali")),
    auto_offset_cali: Number(formData.get("auto_offset_cali")),
    timelapse: booleanField(formData, "timelapse"),
    ams_mapping: jsonArrayField(formData, "ams_mapping"),
    ams_mapping2: jsonArrayField(formData, "ams_mapping2"),
    ams_mapping_info: jsonArrayField(formData, "ams_mapping_info"),
  };
}

function booleanField(formData: FormData, name: string) {
  return formData.get(name) === "true";
}

function jsonArrayField(formData: FormData, name: string): unknown[] {
  const value = formData.get(name);
  return value === null ? [] : (JSON.parse(String(value)) as unknown[]);
}
