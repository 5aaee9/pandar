import contract from "../../contracts/hub-client.openapi.json";
import type {
  HubSchemaMap,
  HubSchemaName,
} from "./generated/hub-api-schema-map";

type Schema = {
  $ref?: string;
  type?: string | string[];
  nullable?: boolean;
  enum?: unknown[];
  oneOf?: Schema[];
  required?: string[];
  properties?: Record<string, Schema>;
  items?: Schema;
  additionalProperties?: boolean | Schema;
};

const schemas = contract.components.schemas as Record<string, Schema>;

export class HubContractError extends Error {
  constructor(path: string, expected: string) {
    super(`Invalid Hub payload at ${path}: expected ${expected}`);
    this.name = "HubContractError";
  }
}

export function decodeHubPayload<Name extends HubSchemaName>(
  schemaName: Name,
  value: unknown,
): HubSchemaMap[Name] {
  validate(schemas[schemaName], value, "$", new Set());
  return value as HubSchemaMap[Name];
}

export function decodeHubResponse<Name extends HubSchemaName>(
  schemaName: Name,
  value: unknown,
): HubSchemaMap[Name] {
  return decodeHubPayload(schemaName, value);
}

function validate(
  schema: Schema,
  value: unknown,
  path: string,
  resolving: Set<string>,
): void {
  if (value === null && schema.nullable) return;
  if (Array.isArray(schema.type)) {
    const valid = schema.type.some((type) => {
      try {
        validate({ ...schema, type }, value, path, resolving);
        return true;
      } catch {
        return false;
      }
    });
    assert(valid, path, "one of the declared wire types");
    return;
  }
  if (schema.oneOf) {
    const valid = schema.oneOf.some((candidate) => {
      try {
        validate(candidate, value, path, resolving);
        return true;
      } catch {
        return false;
      }
    });
    assert(valid, path, "one of the declared wire types");
    return;
  }
  if (schema.$ref) {
    const name = schema.$ref.slice(schema.$ref.lastIndexOf("/") + 1);
    if (resolving.has(name)) return;
    const next = new Set(resolving).add(name);
    validate(schemas[name], value, path, next);
    return;
  }
  if (schema.enum && !schema.enum.includes(value)) {
    throw new HubContractError(path, schema.enum.map(String).join(" | "));
  }
  switch (schema.type) {
    case undefined:
      return;
    case "null":
      assert(value === null, path, "null");
      return;
    case "string":
      assert(typeof value === "string", path, "string");
      return;
    case "boolean":
      assert(typeof value === "boolean", path, "boolean");
      return;
    case "number":
      assert(
        typeof value === "number" && Number.isFinite(value),
        path,
        "number",
      );
      return;
    case "integer":
      assert(Number.isInteger(value), path, "integer");
      return;
    case "array":
      assert(Array.isArray(value), path, "array");
      value.forEach((item, index) =>
        validate(schema.items ?? {}, item, `${path}[${index}]`, resolving),
      );
      return;
    case "object": {
      assert(isRecord(value), path, "object");
      for (const required of schema.required ?? []) {
        assert(
          Object.hasOwn(value, required),
          `${path}.${required}`,
          "present field",
        );
      }
      const properties = schema.properties ?? {};
      for (const [field, fieldSchema] of Object.entries(properties)) {
        if (Object.hasOwn(value, field)) {
          validate(fieldSchema, value[field], `${path}.${field}`, resolving);
        }
      }
      if (schema.additionalProperties === false) {
        for (const field of Object.keys(value)) {
          assert(
            Object.hasOwn(properties, field),
            `${path}.${field}`,
            "declared field",
          );
        }
      }
      return;
    }
    default:
      throw new HubContractError(path, schema.type);
  }
}

function assert(
  condition: boolean,
  path: string,
  expected: string,
): asserts condition {
  if (!condition) throw new HubContractError(path, expected);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
