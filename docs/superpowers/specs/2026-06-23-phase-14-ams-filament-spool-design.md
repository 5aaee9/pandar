# Phase 14 Spec: AMS, Filament, And Spool State

## Scope

Phase 14 promotes Bambu AMS and external-spool state from raw MQTT report details into stable Pandar data that operators can inspect through the hub and frontend.

The implementation must:

1. Normalize AMS units, trays, external spool trays, active tray evidence, and filament identity/color/type fields from agent MQTT reports.
2. Persist the latest tenant-scoped material state for each printer in SQLite and PostgreSQL.
3. Preserve print-time `ams_mapping` and `ams_mapping2` semantics used by Bambu `project_file` commands.
4. Capture job-level filament usage records with explicit uncertainty boundaries derived from observed print reports and persisted print mappings.
5. Expose printer material state and job filament usage through tenant-scoped HTTP responses and the operational frontend.

This phase does not integrate Spoolman or any other external inventory system. It establishes Pandar's internal material state model first.

## Reference Facts

Use these facts from `reference/`:

- `reference/bambuddy/backend/app/services/bambu_mqtt.py` treats `ams.tray_exist_bits` as firmware's canonical slot-presence bitmask and clears stale tray filament fields when a slot bit is absent.
- `reference/bambuddy/backend/app/services/virtual_printer/mqtt_bridge.py` documents that firmware sends partial `ams` blobs: full unit/tray snapshots, status-only updates, and tray-targeted updates. A coherent state requires merge-by-AMS-id and merge-by-tray-id, not wholesale replacement.
- The same bridge notes that `vt_tray` can arrive as partial external-spool updates and must be shallow-merged with the previous external-spool state.
- `reference/bambuddy/backend/app/services/bambu_mqtt.py` handles `vir_slot` as H2-series external spool data and normalizes it into the same conceptual external tray state as `vt_tray`.
- `reference/BambuStudio/src/slic3r/Utils/bambu_networking.hpp` includes `ams_mapping`, `ams_mapping2`, and `task_use_ams` in print parameters.
- `reference/BambuStudio/src/slic3r/Utils/CalibUtils.cpp` builds filament tray config from fields such as `filament_id` / `setting_id`, `tag_uid`, `ams_id`, `slot_id`, `filament_type`, tray name, color, multi-color values, and existence.

## Non-Goals

- Do not add Spoolman-style external inventory, spool purchasing, catalog syncing, or weight-management APIs.
- Do not parse 3MF internals for precise per-filament grams in this phase.
- Do not add filament load/unload, AMS RFID refresh, drying, calibration, or tray-selection control commands.
- Do not persist raw MQTT report JSON as the public data model.
- Do not infer exact remaining filament grams from Bambu `remain` fields; preserve uncertain values as raw estimates with explicit field names. Do not change Phase 15 live WebSocket consumption scope.

## Agent Normalization

Add a focused material-state normalizer under `pandar-agent`, for example `crates/pandar-agent/src/machine/materials.rs`.

The normalizer consumes raw MQTT report JSON and returns a normalized printer material patch. It is a patch, not a full snapshot. Empty or absent raw material fields produce no patch.

```json
{
  "type": "printer_material_patch",
  "observed_at": "...",
  "ams_units": [
    {
      "unit_id": "0",
      "replace_trays": false,
      "unit_kind": "ams|ams_ht|unknown",
      "humidity": 30,
      "temperature_celsius": 28,
      "trays": [
        {
          "tray_id": "0",
          "global_tray_id": 0,
          "exists": true,
          "state": "10",
          "filament_id": "GFL05",
          "setting_id": "GFSL05",
          "type": "PLA",
          "color": "FF0000",
          "multi_color": ["FF0000", "00FF00"],
          "tag_uid": "...",
          "tray_uuid": "...",
          "name": "A1",
          "remaining_estimate": "42"
        }
      ]
    }
  ],
  "replace_external_spools": false,
  "external_spools": [
    {
      "external_id": "254",
      "tray_id": "0",
      "exists": true,
      "filament_id": "GFL05",
      "setting_id": "GFSL05",
      "type": "PLA",
      "color": "FF0000",
      "name": "Ext"
    }
  ],
  "active_tray": {
    "kind": "ams|ams_ht|external|unknown",
    "global_tray_id": 0,
    "ams_id": "0",
    "tray_id": "0"
  }
}
```

Rust types may use normal Rust naming, but serialized JSON and persisted columns must use stable snake_case names. In patch JSON, an absent field means preserve previous state, `null` means clear the previous field, and a concrete value means overwrite. This presence contract is required for safe partial MQTT updates.

Patch JSON must be parsed as `serde_json::Value` or equivalent tri-state fields so absent and explicit `null` stay distinct. `observed_at` is RFC3339 UTC; ordering compares parsed timestamps, and equal timestamps are accepted.

Normalization rules:

- `print.ams.ams` is the AMS unit array when `print.ams` is an object.
- Unit ids and tray ids are strings in the public model because firmware may send string or numeric ids.
- `unit_kind` is `ams_ht` when the parsed unit id is in `128..=135`, `ams` when it is in `0..=63`, otherwise `unknown`. Phase 14 follows Bambuddy's active-tray range over BambuStudio's broader calibration range.
- `global_tray_id` is `ams_id * 4 + tray_id` for normal AMS units when both ids parse as non-negative integers and `ams_id < 64`. For AMS-HT or unknown unit ids, preserve `global_tray_id = null`; this avoids overclaiming flat global identity even though Bambuddy internally uses AMS-HT id as global id.
- `tray_exist_bits` and `power_on_flag` are read from `print.ams.tray_exist_bits` and `print.ams.power_on_flag` as agent-only patch-construction metadata. They are not public JSON or persisted; the hub must never parse them.
- For normal AMS units only, the agent parses `tray_exist_bits` from either an integer or firmware hex string. AMS-HT and unknown units skip bitmask cleanup.
- When a normal AMS bit is absent, the agent emits that tray in the patch with `exists = false`, `state = "9"`, and explicit `null` for `filament_id`, `setting_id`, `type`, `color`, `multi_color`, `tag_uid`, `tray_uuid`, `name`, and `remaining_estimate`.
- `power_on_flag = false` with parsed `tray_exist_bits = 0` must not emit slot-clearing patches during printer-shutdown transitional reports. Non-zero `tray_exist_bits` with `power_on_flag = false` still applies normal empty-slot cleanup.
- Partial MQTT reports must not erase previous material state. The agent emits only observed fields plus explicit `null` clears when firmware evidence says a field should be removed.
- `vt_tray` may be an object or array and is normalized as `external_spools`.
- `vir_slot` takes precedence over `vt_tray` when both appear in a report, matching the H2-series behavior noted by Bambuddy. If a report contains exactly one `vir_slot` entry with id `255`, normalize that external id to `254` so it matches the active `tray_now = 254` convention from the reference.
- External spool canonical identity is `(external_id, tray_id)`. Phase 14 canonicalizes all external-spool identity to `external_id = "254"` so `tray_now = 254`, flat `ams_mapping = 254`, and BambuStudio-style `ams_mapping2 ams_id = 255` match the same spool. `tray_id` is `"0"` for one external spool and the slot index for multi-entry `vir_slot`/`vt_tray`; raw `id = 254` or `255` does not become canonical `external_id`.
- Color values are normalized by trimming a leading `#`, uppercasing hex strings, and preserving only 6- or 8-character hex values. Invalid color strings are omitted.
- Filament fields should be collected from known Bambu keys when present: `tray_info_idx`, `setting_id`, `tray_type`, `tray_color`, `tray_sub_brands`, `tag_uid`, `tray_uuid`, `remain`, `state`, and `cols`. `remaining_estimate` is the raw firmware `remain` value serialized as a string; Phase 14 does not assign units to it.
- `filament_id` and `setting_id` should be normalized using Bambuddy's documented conversion:
  - filament id such as `GFL05` maps to setting id `GFSL05`.
  - setting id such as `GFSL05` maps to filament id `GFL05`.
  - version suffixes such as `_07` are stripped when deriving the opposite id.
  - user preset ids starting with `P` and non-`GF...` ids are preserved unchanged.
- The active tray comes from `print.ams.tray_now` when present: `255` emits `null`; `254` emits external `(254,"0")`; `0..=15` emits normal AMS `{kind:"ams", global_tray_id, ams_id=value/4, tray_id=value%4}`; `128..=135` emits `{kind:"ams_ht", ams_id:value, tray_id:"0", global_tray_id:null}`. H2D `snow`/`pending_tray_target` disambiguation is a known Phase 14 limitation; other values are ignored.
- `tray_tar`, `ams_tray_now`, `ams_tray_tar`, `vt_tray`, and model-specific H2 disambiguation fields are not active-tray sources in Phase 14 unless they also resolve to the `tray_now` shapes above. Unknown or conflicting fields must be ignored rather than guessed.

## JSON Boundary Rules

The hub applies these parsing rules:

- Empty `printer_materials_json` means no material patch. Invalid JSON/root/type/`observed_at` ignores only the material patch, logs the full cause chain at warn level, and still reconciles print progress.
- `ams_mapping_json` and `ams_mapping2_json` in queued command payloads are trusted only if they parse as the validated API shapes below. Invalid stored command mapping JSON is treated as a command serialization/protocol error and must preserve the lower error context.
- API `ams_mapping` and `ams_mapping2` are optional. Omitted fields persist `NULL`. Present empty arrays persist `"[]"`. Invalid shape, non-integer entries, or more than 32 entries returns HTTP `400`.
- Job responses render persisted `NULL` mappings as `null` and persisted `"[]"` as `[]`; corrupt persisted mapping JSON is a repository error with context.

## Material Merge Contract

The hub merges material patches into `printer_material_snapshots` with these exact rules:

- If `printer_materials_json` is empty, the report has no material patch and the snapshot is unchanged.
- If patch `observed_at` is older than the persisted snapshot `observed_at`, ignore only the material patch. Equal timestamps are accepted. Print progress reconciliation still runs from the report.
- If no snapshot exists, create one from the patch with absent collections treated as empty and absent scalar fields omitted.
- `ams_units` absent preserves all persisted AMS units. Present units merge by `unit_id`.
- For a matching AMS unit, absent scalar fields preserve previous values, `null` clears previous values, and concrete values overwrite previous values.
- Unit `trays` absent preserves all persisted trays for that unit. Present trays merge by `tray_id`.
- `replace_trays = true` removes persisted trays for that unit that are not present in the patch. The agent sets it only when the raw report contains a full unit snapshot with a `tray` array for that unit. `replace_trays = false` or absent preserves unmentioned trays.
- Tray fields use the same absent/null/concrete merge rule as unit fields.
- Slot-empty behavior is represented only by agent-emitted tray field patches, for example `exists = false` plus explicit null identity fields. The hub applies the generic field merge and does not know why a field was cleared.
- `external_spools` absent preserves all persisted external spools. Present external spools merge by canonical `(external_id, tray_id)`.
- `replace_external_spools = true` removes persisted external spools not present in the patch. The agent sets it only for `vir_slot` arrays or `vt_tray` arrays with more than one entry; single `vt_tray` object/array leaves it false.
- `active_tray` absent preserves the current active tray. `active_tray = null` clears it. A concrete active-tray object overwrites it.
- Persisted `updated_at` is the hub write time; persisted `observed_at` is the latest accepted patch observation time.

## Protocol

Extend `proto/pandar/agent/v1/agent.proto`:

- Add `string printer_materials_json = 18` to `PrintJobReport`.
- Add `PrintProjectFile.ams_mapping_json = 12` and `PrintProjectFile.ams_mapping2_json = 13`. These carry serialized JSON arrays from hub to agent and are empty when the request has no explicit mapping.

`printer_materials_json` carries normalized material patch JSON, not raw MQTT, so field presence and `null` survive proto encoding. `ams_mapping2_json` uses API/domain spelling; the agent publishes it to Bambu MQTT as `ams_mapping_2`.

Generated protobuf artifacts must stay under Cargo `target` and remain untracked.

## Hub Persistence

Add SQLite and PostgreSQL migrations with equivalent behavior.

New latest-state tables:

- `printer_material_snapshots`
  - `id TEXT PRIMARY KEY`
  - `tenant_id TEXT NOT NULL`
  - `printer_id TEXT NOT NULL`
  - `agent_id TEXT NOT NULL`
  - `serial_number TEXT NOT NULL`
  - `ams_json TEXT NOT NULL`
  - `external_spools_json TEXT NOT NULL`
  - `active_tray_json` nullable JSON text
  - `observed_at TEXT NOT NULL`
  - `updated_at TEXT NOT NULL`
  - unique `(tenant_id, printer_id)`
  - foreign keys to `tenants(id)`, `printers(id)`, and `agents(id)`
  - indexes on `(tenant_id, printer_id)` and `(tenant_id, serial_number)`

All id, timestamp, and JSON columns use `TEXT` in both SQLite and PostgreSQL. `ams_json` and `external_spools_json` store JSON arrays. `active_tray_json` stores a JSON object or `NULL`.

New job mapping/usage tables or columns:

- Persist print request mapping on `jobs`:
  - `ams_mapping_json` nullable JSON text
  - `ams_mapping2_json` nullable JSON text
- Add `job_filament_usages`:
  - `id TEXT PRIMARY KEY`
  - `tenant_id TEXT NOT NULL`
  - `job_id TEXT NOT NULL`
  - `slot_index INTEGER NOT NULL`
  - `source TEXT NOT NULL`
  - `ams_id TEXT NULL`
  - `tray_id TEXT NULL`
  - `global_tray_id INTEGER NULL`
  - `external_id TEXT NULL`
  - `filament_id TEXT NULL`
  - `setting_id TEXT NULL`
  - `filament_type TEXT NULL`
  - `color TEXT NULL`
  - `used_mm TEXT NULL`
  - `used_grams TEXT NULL`
  - `confidence TEXT NOT NULL`
  - `created_at TEXT NOT NULL`
  - unique `(tenant_id, job_id, slot_index, source)`
  - foreign keys to `tenants(id)` and `jobs(id)`
  - indexes on `(tenant_id, job_id)` and `(tenant_id, job_id, slot_index)`

Allowed `source` values are `ams_mapping2` and `ams_mapping`. Allowed `confidence` values are `mapped_no_quantity` and `report_estimate`. Phase 14 only emits `mapped_no_quantity`; `report_estimate` is reserved for a future parser that has a reference-backed quantity field. `used_mm` and `used_grams` are nullable decimal strings and remain `NULL` in Phase 14.

Allowed values may be enforced by SQL `CHECK` constraints or repository validation, but SQLite/PostgreSQL behavior must match. Do not use backend-specific JSON types or generated columns.

The repository API should expose:

- latest material snapshot lookup for a tenant printer;
- list material snapshots for a tenant;
- upsert latest material snapshot from an agent print report;
- persist mapping JSON when creating a print job;
- upsert derived `job_filament_usages` when a job reaches a terminal physical print status and mapping/material data exists.

Usage derivation rules:

- BambuStudio and Bambuddy prove that `ams_mapping` is a flat integer array and `ams_mapping_2` / `ams_mapping2` is an array of `{ams_id, slot_id}` objects in print commands. The exact persistence-time derivation below is Pandar's Phase 14 convention for stable reporting; unverified firmware cases must stay visible as `mapped_no_quantity` rather than exact usage.
- Mapping behavior across boundaries:
  - neither supplied: persist `NULL` for both, proto strings are empty, MQTT omits both keys;
  - only `ams_mapping`: persist/set only `ams_mapping_json`, MQTT includes only `ams_mapping`;
  - only `ams_mapping2`: persist/set only `ams_mapping2_json`, MQTT includes only `ams_mapping_2`;
  - both supplied: persist/dispatch both; `ams_mapping2` only takes precedence for usage derivation.
- `slot_index` is zero-based and comes from the array index in `ams_mapping` or `ams_mapping2`.
- `ams_mapping2` takes precedence for a slot when it has an object at that slot index. Otherwise derive from `ams_mapping`.
- An `ams_mapping2` object must contain integer `ams_id` and `slot_id`.
- `ams_mapping2: {"ams_id": 255, "slot_id": 255}` means unmapped and produces no usage row. `{"ams_id": 254, "slot_id": 255}` is also treated as unmapped because slot `255` is the no-slot sentinel.
- `ams_mapping2` with `ams_id` in `0..=63` derives `ams_id`, `tray_id`, and `global_tray_id = ams_id * 4 + slot_id`.
- `ams_mapping2` with `ams_id` in `128..=135` derives `ams_id` and `tray_id = slot_id`, but `global_tray_id = NULL`. Values outside all listed ranges produce no usage row in Phase 14.
- `ams_mapping2` with `ams_id` `254` or `255` and `slot_id != 255` derives external canonical identity `(external_id = "254", tray_id = slot_id)`.
- `ams_mapping` values use the same slot index as the array position.
- `ams_mapping = -1` means unmapped and produces no usage row.
- `ams_mapping` values `0..=15` derive normal AMS `ams_id = value / 4`, `tray_id = value % 4`, and `global_tray_id = value`.
- `ams_mapping` values `128..=135` derive `ams_id = value`, `tray_id = "0"`, and `global_tray_id = NULL`.
- `ams_mapping` value `254` derives external canonical identity `(external_id = "254", tray_id = "0")`.
- `ams_mapping` value `255` is treated as unmapped and produces no usage row.
- Duplicate mappings are allowed and produce one row per `slot_index`.
- Filament identity fields are copied from the latest material snapshot by the derived AMS tray identity or external canonical identity. Missing material snapshot or missing tray leaves identity fields `NULL`.
- Phase 14 creates rows with `used_mm = NULL`, `used_grams = NULL`, and `confidence = mapped_no_quantity`.
- Completed, failed, and cancelled physical terminal statuses can produce usage rows. Pending/running jobs must not produce final usage rows.
- Usage derivation must be idempotent under replayed terminal reports. Once a usage row exists for `(tenant_id, job_id, slot_index, source)`, Phase 14 does not rewrite its filament identity from newer material snapshots; identity is fixed at first terminal derivation.

All new persistent behavior must be behind backend-neutral repositories and tested against SQLite by default, with PostgreSQL parity through the existing optional `PANDAR_TEST_POSTGRES_URL` harness plus static migration parity tests when no PostgreSQL URL is configured.

## Hub HTTP API

Extend tenant-scoped responses:

- `GET /api/v1/tenants/{tenant_id}/printers`
- `GET /api/v1/tenants/{tenant_id}/printers/{printer_id}`

Printer responses include:

```json
{
  "materials": {
    "ams_units": [],
    "external_spools": [],
    "active_tray": null,
    "observed_at": "..."
  }
}
```

When no material state exists, `materials` is `null`.

Extend job create request:

```json
{
  "ams_mapping": [0, -1, 254],
  "ams_mapping2": [{ "ams_id": 0, "slot_id": 0 }]
}
```

Both fields are optional. Validation:

- `ams_mapping` must be an array of integers.
- `ams_mapping2` must be an array of objects with integer `ams_id` and `slot_id`.
- More than 32 entries returns HTTP `400`.
- Invalid shape returns HTTP `400`.

Extend job responses:

```json
{
  "material": {
    "ams_mapping": [],
    "ams_mapping2": [],
    "filament_usage": [
      {
        "slot_index": 0,
        "source": "ams_mapping2",
        "ams_id": "0",
        "tray_id": "0",
        "global_tray_id": 0,
        "external_id": null,
        "filament_id": "GFL05",
        "setting_id": "GFSL05",
        "filament_type": "PLA",
        "color": "FF0000",
        "used_mm": null,
        "used_grams": null,
        "confidence": "mapped_no_quantity"
      }
    ]
  }
}
```

Ordering is stable: `filament_usage` sorts by `slot_index`, then `source`.

The hub still does not accept Bambu access codes. Material fields must not contain access codes.

Material parsing must not copy Bambu credential fields such as `access_code`, `password`, `passwd`, `token`, or `auth` from raw reports into normalized material JSON. Mapping APIs are numeric-only and therefore reject credential-shaped strings by schema validation.

## Agent Print Dispatch

`BambuMqttCommand::ProjectFile` must include `ams_mapping` and `ams_mapping_2` in the Bambu `project_file` payload only when the hub supplied valid mapping JSON. Public Pandar/proto use `ams_mapping2`; Bambu MQTT uses `ams_mapping_2`. When publishing, flat `ams_mapping` external values `254`/`255` must be rewritten to `-1`; persisted Pandar mapping JSON may keep `254` for reporting, but on-wire external identity belongs in `ams_mapping_2`.

Preserve existing `use_ams` behavior:

- When no mapping is supplied, keep current payload behavior.
- When mapping is supplied, keep the hub-provided `use_ams` value and include explicit mapping fields. Phase 14 does not auto-toggle `use_ams` from mapping contents.

## Frontend

The operations dashboard must:

- Show each printer's current AMS/external-spool material summary without exposing raw MQTT JSON.
- Show AMS unit/tray identifiers, filament type, color swatch, filament/setting id, existence, and active-tray marker when available.
- Show external spool rows separately from AMS trays.
- Show each job's persisted `ams_mapping`, `ams_mapping2`, and derived filament usage rows when present.
- Keep the print dispatch form simple: optional mapping inputs may be omitted in Phase 14 UI if the backend API and job response can preserve mappings from API clients. Do not add a complex slicer-style mapping editor in this phase.

## WebSocket Events

Existing printer/job WebSocket event production may include the enriched HTTP response shapes where those events are already emitted. Frontend WebSocket consumption remains Phase 15.

## Tests

Required test coverage:

- Agent material parser:
  - full AMS report with units/trays;
  - partial AMS update omits unobserved fields and can emit explicit `null` clears;
  - integer and hex-string `tray_exist_bits` make the agent emit explicit empty-slot clears for normal AMS units only;
  - `power_on_flag = false` with zero bitmask does not emit slot-clearing patches;
  - `power_on_flag = false` with non-zero bitmask still applies empty-slot cleanup;
  - `vt_tray` object and array normalization;
  - `vir_slot` precedence over `vt_tray`;
  - single-entry `vir_slot` id `255` normalizes to external id `254`;
  - `replace_trays` and `replace_external_spools` are emitted only for full snapshot shapes, including one-entry vs multi-entry `vt_tray` behavior;
  - active tray derives with exact JSON shapes from `print.ams.tray_now` values `0..=15`, `128..=135`, `254`, and `255`;
  - filament id / setting id conversion;
  - color normalization;
  - proto `PrintJobReport` material payload population.
- Agent print command payload:
  - no mapping keeps current payload shape;
  - mapping adds Bambu MQTT `ams_mapping` and `ams_mapping_2`; single-field mappings produce only their corresponding key.
  - flat `ams_mapping` rewrites external values `254`/`255` to `-1` in the MQTT payload.
  - mapping does not auto-toggle the hub-provided `use_ams` value.
- Hub persistence:
  - SQLite material snapshot upsert and tenant scoping;
  - optional PostgreSQL material snapshot and usage behavior when configured;
  - migration parity for SQLite/PostgreSQL including column nullability, foreign keys, uniqueness, indexes, and allowed-value enforcement;
  - partial material patch replay preserves unmentioned units/trays and applies explicit `null` clears;
  - out-of-order material patches do not overwrite newer material snapshots, while equal `observed_at` patches are accepted;
  - invalid `printer_materials_json` is ignored for material state while print progress reconciliation still runs;
  - mapping derivation covers `-1`, normal AMS ids, AMS-HT ids, external id `254`, unmapped sentinel `255`, external canonical identity matching from both mapping formats, `ams_mapping2` precedence, and duplicate slot mappings;
  - external spool identity matches across normalized snapshot `(254, "0")`, `tray_now = 254`, `ams_mapping = 254`, and `ams_mapping2` single-external entries such as `{ "ams_id": 255, "slot_id": 0 }`;
  - newer material snapshots after terminal usage derivation do not rewrite existing usage identity rows;
  - replaying an older material patch before or after terminal usage derivation does not change the latest snapshot or duplicate usage rows;
  - terminal print report with material state upserts snapshot and derives idempotent usage rows.
- Hub routes:
  - printer list/detail includes `materials`;
  - job create validates mapping shape and persists valid mappings;
  - job list/detail includes mapping and filament usage, with omitted mappings rendered as `null` and present empty arrays rendered as `[]`;
  - tenant scoping and role behavior remain unchanged.
  - raw material reports containing credential keys such as `access_code`, `password`, `passwd`, `token`, or `auth` do not persist, render, or log those values; mapping APIs reject credential-shaped strings through numeric schema validation.
- Frontend:
  - TypeScript build passes.
  - Parser/rendering helpers tolerate `materials = null`, empty trays, and missing usage rows.

## Documentation

Update:

- `docs/architecture.md` with the material-state boundary, mapping persistence, and Spoolman non-goal.
- `docs/roadmap.md` to mark Phase 14 completed only after implementation review and full verification pass.

## Acceptance Criteria

- Printer HTTP responses expose current normalized AMS/external-spool state without requiring raw MQTT knowledge.
- Print job creation can persist Bambu `ams_mapping` and `ams_mapping2`; agent dispatch includes them in `project_file` when supplied.
- Job HTTP responses expose persisted mapping and idempotently derived filament usage rows with clear confidence labels.
- Physical print report processing updates progress and material state together from one observed report.
- Partial material patch replay is deterministic: absent fields preserve, explicit `null` clears, concrete values overwrite, and older observations cannot regress the latest material snapshot.
- Old agents that omit `printer_materials_json` continue to report print progress without material updates; new agents talking to Phase 14 hubs can send material patches without requiring a separate protocol event.
- SQLite and PostgreSQL schemas stay equivalent.
- No Bambu access code is accepted, persisted, logged, or rendered by Phase 14 material features.
- No generated protobuf output is committed.
