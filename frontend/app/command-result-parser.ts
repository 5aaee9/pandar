import type {
  Command,
  CommandResultData,
  DiagnosticResultData,
  DiscoveryResultData,
} from "./dashboard-types";

type JsonRecord = Record<string, unknown>;

export function parseCommandResult(
  command: Command | null,
): CommandResultData | null {
  if (!command?.result_json) {
    return null;
  }

  try {
    const parsed = JSON.parse(command.result_json) as unknown;
    if (!isRecord(parsed)) {
      return null;
    }
    if (parsed.type === "printer_discovery" && Array.isArray(parsed.printers)) {
      return {
        type: "printer_discovery",
        printers: parsed.printers
          .map(parseDiscoveryPrinter)
          .filter(
            (printer): printer is DiscoveryResultData["printers"][number] =>
              Boolean(printer),
          ),
      };
    }
    if (
      parsed.type === "printer_diagnostic" &&
      typeof parsed.serial_number === "string" &&
      typeof parsed.overall === "string" &&
      Array.isArray(parsed.checks)
    ) {
      return {
        type: "printer_diagnostic",
        serial_number: parsed.serial_number,
        overall: parsed.overall,
        host: optionalString(parsed.host),
        model: optionalString(parsed.model),
        checks: parsed.checks
          .map(parseDiagnosticCheck)
          .filter((check): check is DiagnosticResultData["checks"][number] =>
            Boolean(check),
          ),
        compatibility: parseCompatibility(parsed.compatibility),
      };
    }
  } catch {
    return null;
  }

  return null;
}

function parseDiscoveryPrinter(
  value: unknown,
): DiscoveryResultData["printers"][number] | null {
  if (!isRecord(value) || typeof value.host !== "string") {
    return null;
  }

  return {
    host: value.host,
    serial_number: optionalString(value.serial_number),
    name: optionalString(value.name),
    model: optionalString(value.model),
    source: optionalString(value.source),
  };
}

function parseDiagnosticCheck(
  value: unknown,
): DiagnosticResultData["checks"][number] | null {
  if (
    !isRecord(value) ||
    typeof value.id !== "string" ||
    typeof value.status !== "string" ||
    typeof value.message !== "string"
  ) {
    return null;
  }

  return {
    id: value.id,
    status: value.status,
    message: value.message,
    details: optionalString(value.details),
  };
}

function parseCompatibility(
  value: unknown,
): DiagnosticResultData["compatibility"] {
  if (!isRecord(value)) {
    return undefined;
  }

  return {
    normalized_model:
      typeof value.normalized_model === "string" ||
      value.normalized_model === null
        ? value.normalized_model
        : undefined,
    external_storage: optionalString(value.external_storage),
    ftps_tls_1_2_cap: optionalBoolean(value.ftps_tls_1_2_cap),
    ftps_clear_data_fallback: optionalBoolean(value.ftps_clear_data_fallback),
    features: parseFeatureMap(value.features),
  };
}

function parseFeatureMap(value: unknown): Record<string, string> | undefined {
  if (!isRecord(value)) {
    return undefined;
  }

  return Object.fromEntries(
    Object.entries(value).filter((entry): entry is [string, string] => {
      const [, featureValue] = entry;
      return typeof featureValue === "string";
    }),
  );
}

function optionalString(value: unknown) {
  return typeof value === "string" ? value : undefined;
}

function optionalBoolean(value: unknown) {
  return typeof value === "boolean" ? value : undefined;
}

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
