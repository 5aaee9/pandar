# Pandar domain glossary

## Hub

The `pandar-hub` Rust API server (axum). The frontend reaches it at `APP_API_URL`. Browser code never calls it directly with cookie authentication — browser requests cross the Hub proxy.

## Hub proxy

The Next.js route surface under `frontend/app/api/tenants/[tenantId]/` and the module behind it, `frontend/app/hub-proxy.ts` (`hubProxy()`). The module owns cross-origin rejection on mutations, path-id validation and encoding, auth header attachment, request body streaming, and response header hygiene; route files are per-endpoint config only.
