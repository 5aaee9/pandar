# Add Printer Type And Bambu Metadata Autodiscovery Design

## Scope

Update the runtime printer-linking flow so operators no longer enter a printer serial number or model when adding a printer from the Agents page. The flow should add an explicit printer type selector with the single supported value `BambuLab`, selected by default.

This change covers:

- Frontend add-printer form fields, English and Chinese copy, and React tests.
- Frontend server action payload shape.
- Hub `link-printer` HTTP request validation, command payloads, redacted persistence, audit metadata, route tests, and gRPC conversion tests.
- Agent `LinkPrinter` handling so Bambu serial/model are resolved from the host during link validation instead of supplied by the operator.
- Agent completion reporting so the serial/model discovered during runtime linking supplement the Hub/UI printer metadata after onboarding finishes.
- Focused docs/roadmap updates and verification.

This change does not cover:

- Adding non-Bambu printer support beyond the enum/selector value.
- Persisting access codes in Hub storage.
- Durable agent-side config persistence across agent restarts.
- A database migration.
- A broad redesign of the Agents page.

## Existing Context

The current link-printer flow asks the operator for host/IP, serial number, access code, optional name, and optional model. Hub validates `host`, `serial_number`, and `access_code` as required strings, persists a redacted `link_printer` command payload, and sends the full secret-bearing `LinkPrinter` proto command over the live agent reverse stream. The agent builds a `BambuPrinterEndpoint` directly from the command payload and uses the supplied serial to subscribe/publish on `device/{serial}/...` MQTT topics.

Bambu LAN discovery already exists in `crates/pandar-agent/src/machine/discovery.rs`. SSDP responses can include host, serial number, printer name, and model. MQTT refresh also discovers model through `info.get_version` once the serial is known. That means serial/model can move from operator input to agent-side metadata resolution, while access code and host remain operator input.

## Product Behavior

On the Agents page, the "Link printer to agent" form should show these editable fields:

- Agent selector, defaulting to the first online agent, then first agent.
- Type selector, defaulting to `BambuLab`; currently `BambuLab` is the only option.
- Printer IPv4 address, required.
- Access code, required password field.
- Name, optional display name.

The form must not render editable serial number or model inputs. The submit button remains "Link printer" / "连接打印机".

The product copy should describe submitting connection details, not submitting serial/model metadata. Empty states stay unchanged except where wording says "printer credentials" and should remain accurate for host/access-code submission.

## API And Payload Design

Hub accepts this request shape for runtime linking:

```json
{
  "type": "BambuLab",
  "host": "192.0.2.10",
  "access_code": "12345678",
  "name": "Office X1C"
}
```

Validation:

- `type`, `host`, and `access_code` are required non-empty strings after trimming.
- `type` must be exactly `BambuLab` for now.
- `host` must parse as an IPv4 address. Hostname input is out of scope for this flow because Bambu SSDP discovery reports the printer source IP address, not the submitted hostname.
- `name` is optional; blank values normalize to `null`.
- `serial_number` and `model` are no longer accepted request fields.
- Unknown fields remain rejected through `serde(deny_unknown_fields)`.
- Invalid JSON, blank required fields, unsupported type values, invalid/non-IPv4 host values, or legacy serial/model fields return `400 { "error": "bad_request" }`.

The internal Hub command payload should carry `printer_type`, `host`, `access_code`, and optional `name`. It should no longer carry operator-supplied serial/model for link-printer commands. Persisted command JSON and audit metadata must still redact the access code and must not contain the submitted access code.

The `LinkPrinter` proto command must carry `host`, `access_code`, `name`, and `printer_type` only. The schema is:

```proto
message LinkPrinter {
  string host = 1;
  reserved 2;
  reserved "serial_number";
  string access_code = 3;
  string name = 4;
  reserved 5;
  reserved "model";
  string printer_type = 6;
}
```

`printer_type` is a string for this change and must be exactly `BambuLab`. The old `serial_number = 2` and `model = 5` tags and field names are reserved, not reused. Hub must not provide serial/model in the live add-printer command; the agent resolves them.

Persisted `link_printer` rows remain redacted live-command records and must never be converted into replayable `HubCommand::LinkPrinter` values. The durable gRPC command converter must continue returning `FailedPrecondition` for persisted `link_printer` records, because persisted payloads intentionally do not contain the submitted access code. Only the route/session live dispatch path may construct the secret-bearing proto command.

Successful link completion result JSON must include the agent-discovered `serial_number` and `model` values alongside `type = "printer_link"`, `host`, optional name, and printer status. The Hub and frontend use this as supplemental onboarding metadata; they must not infer or require those values from the original request.

## Agent Design

For `printer_type = BambuLab`, the agent should resolve printer metadata before constructing the endpoint used for MQTT validation:

1. Run Bambu SSDP discovery with a short bounded timeout.
2. Find a discovered printer whose `host` exactly matches the submitted IPv4 address after trimming.
3. Require the discovery result to include `serial_number`; if missing, fail the link command with a redacted error explaining that the printer serial could not be discovered for the host.
4. Use the discovered serial to build `BambuPrinterEndpoint` with the submitted host/access code, optional submitted name, and discovered model if present.
5. Validate through the existing runtime link path. The MQTT refresh continues to discover the authoritative model through `info.get_version`, and the emitted snapshot/result use the validated snapshot values.
6. After the link/onboard validation succeeds, send a `PrinterSnapshot` and a successful `CommandResult.result_json` that both carry the discovered serial and model. This is the required metadata-completion path for fields the operator no longer enters.

This keeps Bambu serial/model as discovered printer metadata. If SSDP discovery cannot see the printer host, link fails cleanly and the operator can use existing discovery/diagnostic flows to investigate. Adding a manual advanced serial override is intentionally out of scope because the request says serial should not be required during add-printer.

Unsupported `printer_type` values must be rejected by Hub before dispatch. If a future proto command reaches an agent with an unsupported type, the agent should reject/fail it without attempting Bambu network calls.

## Testing

Frontend tests should verify:

- The add-printer form renders Agent, Type, printer IP address, Access code, optional Name, and submit.
- Type defaults to `BambuLab`.
- Serial number and Model fields are absent.
- The frontend server action posts `type`, `host`, `access_code`, and optional `name`, not serial/model.

Hub route/repository/gRPC tests should verify:

- `link-printer` accepts the new payload and direct-sends a `LinkPrinter` command with type/host/access code/name.
- Persisted command payloads and audit metadata include type/host/name with redacted access code and no operator serial/model.
- Blank `type`, blank `host`, blank `access_code`, invalid/non-IPv4 host values, unsupported type, and legacy `serial_number`/`model` request fields are rejected without command rows.
- Durable gRPC command conversion still rejects persisted `link_printer` records with `FailedPrecondition`; redacted persisted rows are never replayed into live proto commands.

Agent tests should verify:

- Successful Bambu link resolves serial/model by host before calling runtime validation, emits ack, snapshot, and success result without the access code.
- The successful snapshot and `printer_link` result JSON include the agent-discovered serial/model so Hub/UI metadata is supplemented after onboarding completion.
- Discovery miss or missing serial produces a failed command result with redacted access code.
- Unsupported printer type is rejected without attempting discovery/MQTT validation.

## Rollback And Safety

The change does not require a database migration. Rollback is code-only: restore the previous request/proto/payload shape and frontend fields if needed. Access-code redaction remains a hard invariant in persisted command payloads, audit metadata, command result errors, and logs.

Runtime compatibility risk is limited to the direct live link-printer path. Existing configured printers, refresh, discovery, diagnostics, print dispatch, and printer controls continue to use stored/reported printer serial numbers.
