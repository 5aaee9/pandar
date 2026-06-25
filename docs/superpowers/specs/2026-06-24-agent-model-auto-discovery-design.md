# Agent Printer Model Auto Discovery Design

## Problem

`pandar-agent` currently builds refresh snapshots from the configured printer endpoint and copies `PANDAR_PRINTERS[].model` into the reported snapshot. Real LAN testing showed the printer can answer an MQTT `info.get_version` request with the actual product model. Refresh should therefore discover the model from the printer instead of relying on configuration.

If model discovery fails, refresh must fail. A stale or missing configured model must not be used as a fallback because that hides LAN/protocol failures from the user.

## Scope

- Update agent-side MQTT refresh only.
- Before publishing `pushing.pushall`, publish:

```json
{ "info": { "command": "get_version", "sequence_id": "90002" } }
```

- Parse the model from the `get_version` response.
- If publishing, receiving, or parsing the version response fails, return an error from refresh.
- Log the discovery failure with printer serial and the full error chain.
- Update `docs/development.md`, `docs/architecture.md`, and `docs/roadmap.md`.
- Keep hub persistence behavior unchanged: printers are already keyed by UUID with a separate `serial_number` attribute and tenant-scoped serial upsert.

## Response Shape

The reference-backed `get_version` response shape is:

```json
{
  "info": {
    "command": "get_version",
    "module": [
      {
        "name": "ota",
        "sw_ver": "01.08.01.00",
        "product_name": "P2S",
        "sn": "..."
      }
    ]
  }
}
```

The refresh path should select a version response by `info.command == "get_version"`. Non-matching reports on the same MQTT report topic are ignored while waiting, but the whole discovery wait uses one total `report_timeout` deadline. A usable model is the trimmed non-empty `product_name` from the `info.module[]` entry whose `name` is `ota`.

The command sequence id is not the identity source for the report; it is only the request sequence value used by the LAN command payload.

## Out Of Scope

- Removing `PANDAR_PRINTERS[].model` from configuration parsing.
- Persisting discovered model back into the local agent config.
- Changing FTPS compatibility decisions to use discovered model.
- Adding database migrations.

## Acceptance Criteria

- `refresh_printer` subscribes to the report topic, sends `info.get_version`, receives and parses a model, then sends `pushing.pushall`.
- A successful refresh snapshot contains the discovered model even when the endpoint config has no model.
- If the version response does not include a usable model string, `refresh_printer` fails and does not publish `pushall`.
- A failed model discovery writes a warning log containing the serial and full error chain.
- Refresh ignores unrelated report-topic messages before the `get_version` response.
- Refresh model discovery fails once the total `report_timeout` deadline expires, even if unrelated reports keep arriving.
- Architecture docs no longer describe configured model values as the refresh snapshot source; they describe refresh-time `info.get_version` discovery and fail-closed behavior.
- Existing pushall state mapping remains unchanged.
- Development docs and roadmap describe refresh-time model discovery and failure behavior.

## Test Plan

- Unit test the `get_version` MQTT payload.
- Unit test model extraction from a representative `info.get_version` response.
- Unit test unrelated reports before the version response are ignored.
- Unit test unrelated reports without a version response do not bypass the discovery timeout.
- Unit test refresh command ordering and snapshot model assignment using `FakeMqttTransport`.
- Unit test missing model response fails refresh before `pushall`.
- Unit test `get_version` publish failure and receive failure fail refresh before `pushall`.
- Unit test discovery failure logging preserves the full error chain.
- Run `cargo fmt`.
- Run focused agent MQTT tests.
- Run workspace clippy and nextest before completion.
