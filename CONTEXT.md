# Pandar domain glossary

## Hub

The `pandar-hub` Rust API server (axum). The frontend reaches it at `APP_API_URL`. Browser code never calls it directly with cookie authentication — browser requests cross the Hub proxy.

## Hub proxy

The Next.js route surface under `frontend/app/api/tenants/[tenantId]/` and the module behind it, `frontend/app/hub-proxy.ts` (`hubProxy()`). The module owns cross-origin rejection on mutations, path-id validation and encoding, auth header attachment, request body streaming, and response header hygiene; route files are per-endpoint config only.

## Route data

The dashboard's per-view React Query data, owned by `frontend/app/route-data.ts`. The module exports `routeDataKeys` (per-view, tenant-scoped key prefixes), per-view query factories (`devicesRouteQuery`, `jobsRouteQuery`, `agentsRouteQuery`, `usersRouteQuery`, `settingsRouteQuery`, `agentSettingsRouteQuery`) carrying the fetch composition and cache policy, and the route-data types. Readers use the factories with `useQuery`; mutations invalidate through `routeDataKeys` — never hand-written key literals.
