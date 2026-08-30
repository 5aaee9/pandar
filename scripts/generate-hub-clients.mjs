import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import { format as formatWithPrettier } from "prettier";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const contractPath = path.join(root, "contracts/hub-client.openapi.json");
const contract = JSON.parse(fs.readFileSync(contractPath, "utf8"));
const schemas = contract.components.schemas;
const config = contract["x-pandar-codegen"];
const check = process.argv.includes("--check");
const outputs = new Map();

const refName = (ref) => ref.slice(ref.lastIndexOf("/") + 1);
const groupOf = (name) => schemas[name]["x-typescript-group"] ?? "core";
const tsFile = (group) => `hub-api-${group}.ts`;

validateContract();

function validateContract() {
  if (contract.openapi !== "3.1.0") throw new Error("Hub client contract must use OpenAPI 3.1.0");
  const knownRefs = new Set([
    ...Object.keys(contract.components.schemas).map((name) => `#/components/schemas/${name}`),
    ...Object.keys(contract.components.parameters ?? {}).map((name) => `#/components/parameters/${name}`),
  ]);
  const inspect = (value) => {
    if (!value || typeof value !== "object") return;
    if (Object.hasOwn(value, "nullable")) {
      throw new Error("OpenAPI 3.1 schemas must use JSON Schema null unions");
    }
    if (value.$ref && !knownRefs.has(value.$ref)) throw new Error(`Unknown OpenAPI ref: ${value.$ref}`);
    Object.values(value).forEach(inspect);
  };
  inspect(contract);
  for (const [route, item] of Object.entries(contract.paths)) {
    const declared = new Set(
      (item.parameters ?? []).map((parameter) => {
        const definition = contract.components.parameters[refName(parameter.$ref)];
        return definition.name;
      }),
    );
    for (const parameter of route.matchAll(/\{([^}]+)\}/g)) {
      if (!declared.has(parameter[1])) throw new Error(`Undeclared path parameter ${parameter[1]} in ${route}`);
    }
    for (const method of ["get", "post", "put", "patch", "delete"]) {
      const operation = item[method];
      if (!operation) continue;
      const errorRef = operation.responses?.default?.content?.["application/json"]?.schema?.$ref;
      if (errorRef !== "#/components/schemas/ErrorResponse") {
        throw new Error(`Missing default ErrorResponse for ${method.toUpperCase()} ${route}`);
      }
    }
  }
}

function walkRefs(schema, found = new Set()) {
  if (!schema || typeof schema !== "object") return found;
  if (schema.$ref) found.add(refName(schema.$ref));
  for (const value of Object.values(schema)) {
    if (value && typeof value === "object") walkRefs(value, found);
  }
  return found;
}

function nullableType(type, schema) {
  return schema.nullable ? `${type} | null` : type;
}

function tsType(schema) {
  if (schema.oneOf) {
    return nullableType(schema.oneOf.map(tsType).join(" | "), schema);
  }
  if (Array.isArray(schema.type)) {
    return schema.type.map((type) => tsType({ ...schema, type })).join(" | ");
  }
  if (schema.$ref) return nullableType(refName(schema.$ref), schema);
  if (schema.enum) {
    return nullableType(schema.enum.map((value) => JSON.stringify(value)).join(" | "), schema);
  }
  let type;
  switch (schema.type) {
    case "string": type = "string"; break;
    case "integer":
    case "number": type = "number"; break;
    case "boolean": type = "boolean"; break;
    case "array": type = `Array<${tsType(schema.items ?? {})}>`; break;
    case "object":
      type = schema.additionalProperties ? "Record<string, unknown>" : "Record<string, unknown>";
      break;
    case "null": type = "null"; break;
    default: type = "unknown";
  }
  return nullableType(type, schema);
}

function tsDefinition(name, schema) {
  if (schema.enum) {
    const values = schema.enum.map((value) => JSON.stringify(value));
    const inline = `export type ${name} = ${values.join(" | ")};`;
    return inline.length <= 80
      ? `${inline}\n`
      : `export type ${name} =\n${values.map((value) => `  | ${value}`).join("\n")};\n`;
  }
  if (schema.type !== "object" || !schema.properties) {
    return `export type ${name} = ${tsType(schema)};\n`;
  }
  const required = new Set(schema.required ?? []);
  const fields = Object.entries(schema.properties).map(([field, value]) =>
    `  ${JSON.stringify(field)}${required.has(field) ? "" : "?"}: ${tsType(value)};`,
  );
  return `export type ${name} = {\n${fields.join("\n")}\n};\n`;
}

for (const group of config.typescriptGroups) {
  const names = Object.keys(schemas).filter((name) => groupOf(name) === group);
  const external = new Map();
  for (const name of names) {
    for (const dependency of walkRefs(schemas[name])) {
      const dependencyGroup = groupOf(dependency);
      if (dependencyGroup === group) continue;
      const list = external.get(dependencyGroup) ?? new Set();
      list.add(dependency);
      external.set(dependencyGroup, list);
    }
  }
  const imports = [...external.entries()]
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([dependencyGroup, dependencies]) =>
      `import type { ${[...dependencies].sort().join(", ")} } from "./${tsFile(dependencyGroup).replace(/\.ts$/, "")}";`,
    );
  const body = names.map((name) => tsDefinition(name, schemas[name])).join("\n");
  outputs.set(
    path.join(root, "frontend/app/generated", tsFile(group)),
    `// Generated from contracts/hub-client.openapi.json. Do not edit.\n${imports.join("\n")}${imports.length ? "\n\n" : "\n"}${body}`,
  );
}

outputs.set(
  path.join(root, "frontend/app/generated/hub-api.ts"),
  `// Generated from contracts/hub-client.openapi.json. Do not edit.\n${config.typescriptGroups.map((group) => `export * from "./${tsFile(group).replace(/\.ts$/, "")}";`).join("\n")}\n`,
);

const allNames = Object.keys(schemas);
const schemaImports = config.typescriptGroups.map((group) =>
  `import type * as ${group[0].toUpperCase() + group.slice(1)} from "./${tsFile(group).replace(/\.ts$/, "")}";`,
);
const schemaMap = allNames.map((name) =>
  `  ${JSON.stringify(name)}: ${groupOf(name)[0].toUpperCase() + groupOf(name).slice(1)}.${name};`,
);
outputs.set(
  path.join(root, "frontend/app/generated/hub-api-schema-map.ts"),
  `// Generated from contracts/hub-client.openapi.json. Do not edit.\n${schemaImports.join("\n")}\n\nexport type HubSchemaMap = {\n${schemaMap.join("\n")}\n};\n\nexport type HubSchemaName = keyof HubSchemaMap;\n`,
);

function kotlinName(name) {
  return schemas[name]["x-kotlin-name"] ?? `${name}Dto`;
}

function camelCase(value) {
  return value.replace(/_([a-z])/g, (_, letter) => letter.toUpperCase());
}

function kotlinType(schema) {
  if (schema.oneOf) {
    const nonNull = schema.oneOf.filter((candidate) => candidate.type !== "null");
    const nullable = nonNull.length !== schema.oneOf.length;
    if (nonNull.length === 1) {
      const type = kotlinType(nonNull[0]);
      return nullable && !type.endsWith("?") ? `${type}?` : type;
    }
    return `JsonElement${nullable ? "?" : ""}`;
  }
  if (Array.isArray(schema.type)) {
    const nonNull = schema.type.filter((type) => type !== "null");
    const type = kotlinType({ ...schema, type: nonNull[0] });
    return nonNull.length !== schema.type.length && !type.endsWith("?") ? `${type}?` : type;
  }
  if (schema.$ref) return `${kotlinName(refName(schema.$ref))}${schema.nullable ? "?" : ""}`;
  let type;
  switch (schema.type) {
    case "string": type = "String"; break;
    case "integer": type = schema.format === "int64" ? "Long" : "Int"; break;
    case "number": type = "Double"; break;
    case "boolean": type = "Boolean"; break;
    case "array": type = `List<${kotlinType(schema.items ?? {})}>`; break;
    case "object": type = "Map<String, JsonElement>"; break;
    case "null": type = "Nothing?"; break;
    default: type = "JsonElement";
  }
  return `${type}${schema.nullable ? "?" : ""}`;
}

function enumConstant(value) {
  const normalized = String(value).replace(/[^A-Za-z0-9]+/g, "_").replace(/^_|_$/g, "").toUpperCase();
  return /^\d/.test(normalized) ? `VALUE_${normalized}` : normalized;
}

function kotlinDefinition(name, schema) {
  const generatedName = kotlinName(name);
  if (schema.enum) {
    const values = schema.enum.map((value) =>
      `    @SerialName(${JSON.stringify(value)})\n    ${enumConstant(value)}(${JSON.stringify(value)})`,
    );
    return `@Serializable\nenum class ${generatedName}(val wireValue: String) {\n${values.join(",\n")}\n}\n`;
  }
  if (schema.type !== "object" || !schema.properties) return "";
  const required = new Set(schema.required ?? []);
  const fields = Object.entries(schema.properties).map(([wireName, fieldSchema]) => {
    const propertyName = camelCase(wireName);
    let type = kotlinType(fieldSchema);
    const optional = !required.has(wireName) || fieldSchema["x-kotlin-optional"];
    const annotations = [];
    if (propertyName !== wireName) annotations.push(`@SerialName(${JSON.stringify(wireName)})`);
    if (!optional && type.endsWith("?")) annotations.push("@Required");
    let defaultValue = "";
    if (optional) {
      if (!type.endsWith("?")) type += "?";
      defaultValue = " = null";
    }
    const annotation = annotations.map((value) => `${value}\n    `).join("");
    return `${annotation}val ${propertyName}: ${type}${defaultValue}`;
  });
  return `@Serializable\ndata class ${generatedName}(\n${fields.map((field) => `    ${field}`).join(",\n")}\n)\n`;
}

const kotlinReachable = new Set(config.kotlinRoots);
for (const rootName of [...kotlinReachable]) {
  for (const dependency of walkRefs(schemas[rootName])) kotlinReachable.add(dependency);
}
let changed = true;
while (changed) {
  changed = false;
  for (const name of [...kotlinReachable]) {
    for (const dependency of walkRefs(schemas[name])) {
      if (!kotlinReachable.has(dependency)) {
        kotlinReachable.add(dependency);
        changed = true;
      }
    }
  }
}
for (const group of config.typescriptGroups) {
  const names = allNames.filter((name) => kotlinReachable.has(name) && groupOf(name) === group);
  if (names.length === 0) continue;
  const body = names.map((name) => kotlinDefinition(name, schemas[name])).filter(Boolean).join("\n");
  outputs.set(
    path.join(root, `mobile/android/app/src/main/kotlin/zip/iptables/pandar/android/data/remote/dto/GeneratedHub${group[0].toUpperCase() + group.slice(1)}.kt`),
    `// Generated from contracts/hub-client.openapi.json. Do not edit.\npackage zip.iptables.pandar.android.data.remote.dto\n\n${body.includes("@Required") ? "import kotlinx.serialization.Required\n" : ""}import kotlinx.serialization.SerialName\nimport kotlinx.serialization.Serializable\n${body.includes("JsonElement") ? "import kotlinx.serialization.json.JsonElement\n" : ""}\n${body}`,
  );
}

for (const [outputPath, content] of outputs) {
  if (outputPath.endsWith(".ts")) {
    const formatted = await formatWithPrettier(content, { parser: "typescript" });
    outputs.set(
      outputPath,
      formatted.replace(
        /export type (\w+) =\n  ((?:"[^"]+"(?: \| )?)+);/g,
        (_, name, values) =>
          `export type ${name} =\n${values.split(" | ").map((value) => `  | ${value}`).join("\n")};`,
      ),
    );
  }
}

const managedDirectories = [
  {
    directory: path.join(root, "frontend/app/generated"),
    matches: (name) => name.startsWith("hub-api") && name.endsWith(".ts"),
  },
  {
    directory: path.join(
      root,
      "mobile/android/app/src/main/kotlin/zip/iptables/pandar/android/data/remote/dto",
    ),
    matches: (name) => name.startsWith("GeneratedHub") && name.endsWith(".kt"),
  },
];
const orphaned = managedDirectories.flatMap(({ directory, matches }) =>
  fs.existsSync(directory)
    ? fs
        .readdirSync(directory)
        .filter(matches)
        .map((name) => path.join(directory, name))
        .filter((candidate) => !outputs.has(candidate))
    : [],
);
if (!check) orphaned.forEach((orphan) => fs.unlinkSync(orphan));

let stale = check && orphaned.length > 0;
if (check) {
  for (const orphan of orphaned) {
    console.error(`orphan generated client: ${path.relative(root, orphan)}`);
  }
}
for (const [outputPath, content] of outputs) {
  const normalized = content.endsWith("\n") ? content : `${content}\n`;
  if (check) {
    if (!fs.existsSync(outputPath) || fs.readFileSync(outputPath, "utf8") !== normalized) {
      console.error(`stale generated client: ${path.relative(root, outputPath)}`);
      stale = true;
    }
  } else {
    fs.mkdirSync(path.dirname(outputPath), { recursive: true });
    fs.writeFileSync(outputPath, normalized);
  }
}
if (stale) process.exit(1);
