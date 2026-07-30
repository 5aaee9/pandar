# Pandar domain glossary

## Hub

The `pandar-hub` Rust API server (axum). The frontend reaches it at `APP_API_URL`. Browser code never calls it directly with cookie authentication — browser requests cross the Hub proxy.

## Hub proxy

The Next.js route surface under `frontend/app/api/tenants/[tenantId]/` and the module behind it, `frontend/app/hub-proxy.ts` (`hubProxy()`). The module owns cross-origin rejection on mutations, path-id validation and encoding, auth header attachment, request body streaming, and response header hygiene; route files are per-endpoint config only.

## Route data

The dashboard's per-view React Query data, owned by `frontend/app/route-data.ts`. The module exports `routeDataKeys` (per-view, tenant-scoped key prefixes), per-view query factories (`devicesRouteQuery`, `jobsRouteQuery`, `agentsRouteQuery`, `usersRouteQuery`, `settingsRouteQuery`, `agentSettingsRouteQuery`) carrying the fetch composition and cache policy, and the route-data types. Readers use the factories with `useQuery`; mutations invalidate through `routeDataKeys` — never hand-written key literals.

## Machine report

One Bambu MQTT report message decoded once into typed sections, owned by `crates/pandar-agent/src/machine/mqtt/report/` (`MachineReport`, with `print` / `snapshot` / `materials` sections plus firmware views). The `MachineReports<T>` adapter wraps a `BambuMqttTransport` so every consumer — refresh flows, report forwarding, the firmware session pump — crosses the seam typed; raw `serde_json::Value` stays inside the transport pumps and the report module, which privately retains the source payload for diagnostics pass-through only.
