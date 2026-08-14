# Pandar domain glossary

## Hub

The `pandar-hub` Rust API server (axum). The frontend reaches it at `APP_API_URL`. Browser code never calls it directly with cookie authentication — browser requests cross the Hub proxy.

## Personal preset

A user-created Bambu Studio Process, Filament, or Printer preset owned by one tenant-local Pandar user and synchronized through the Studio plugin. Personal presets are not tenant-shared configuration.

## Hub database dialect

The SQLite/PostgreSQL differences owned by `crates/pandar-hub/src/db.rs`: write and snapshot transaction modes, row and table locking, and typed unique/foreign-key violation classification. Repositories cross this seam through `Database`, `ConnectionDialectExt`, and `TransactionDialectExt`; they do not branch on the backend for these shared behaviors. Runtime SQL that is genuinely backend-specific remains local to its owning module. Migration authors edit `migrations/shared/` plus paired full-file overrides under `migrations/overrides/{sqlite,postgres}/`, then regenerate the sqlx input directories with `scripts/sync-hub-migrations.sh`.

## Hub proxy

The Next.js route surface under `frontend/app/api/tenants/[tenantId]/` and the module behind it, `frontend/app/hub-proxy.ts` (`hubProxy()`). Every browser request to the Hub crosses it — reads and mutations alike; browser code never fetches the Hub directly. The module owns cross-origin rejection on mutations, path-id validation and encoding, auth header attachment, request body streaming, response header hygiene, and declared query strings; route files are per-endpoint config only.

## Route data

The dashboard's per-view React Query data, owned by `frontend/app/route-data.ts`. The module exports `routeDataKeys` (per-view, tenant-scoped key prefixes), per-view query factories (`devicesRouteQuery`, `jobsRouteQuery`, `agentsRouteQuery`, `usersRouteQuery`, `settingsRouteQuery`, `agentSettingsRouteQuery`) carrying the fetch composition and cache policy, and the route-data types. Readers use the factories with `useQuery`; queryFns fetch same-origin through the Hub proxy. Mutations invalidate through `routeDataKeys` — never hand-written key literals.

## Printer control

The dashboard's printer-action seam, owned by `frontend/app/printer-controls.tsx`. The module exports `usePrinterControl()` (the mutation hook bound to the `controlPrinter` server action), the `PrinterControlIntent` tagged union (semantic actions such as `move_axes`, `set_hotend_temperature`, `ams_load_filament`, `nozzle_holder_ctrl`), `PrinterControlFields` (renders the hidden form fields for an intent, including per-action field-selection policy such as AMS target inclusion), and `printerControlFieldNames` for the user-editable inputs. View components declare intents; they never hand-write `tenant_id` / `printer_id` / `action` hidden fields.

## Studio status projection

The deterministic translation of one validated Hub printer-list response into Bambu Studio-facing printer status and firmware observations. It does not own connection freshness, authentication, request transport, or callback lifecycle.

## Machine report

One Bambu MQTT report message decoded once into typed sections, owned by `crates/pandar-agent/src/machine/mqtt/report/` (`MachineReport`, with `print` / `snapshot` / `materials` sections plus firmware views). The `MachineReports<T>` adapter wraps a `BambuMqttTransport` so every consumer — refresh flows, report forwarding, the firmware session pump — crosses the seam typed; raw `serde_json::Value` stays inside the transport pumps and the report module, which privately retains the source payload for diagnostics pass-through only.

## Machine snapshot

A normalized point-in-time printer observation produced by the Agent before wire projection. It may contain connection details, inventory identity, state, telemetry, and capability observations; explicit authority facts state which absent values may clear previously persisted data.
