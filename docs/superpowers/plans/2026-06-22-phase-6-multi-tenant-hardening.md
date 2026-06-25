# Phase 6 Multi-Tenant Hardening Implementation Plan

> **For indexyz:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan.

**Goal:** Add tenant-scoped API token auth, role authorization, audit records, frontend token forwarding, and compose examples.

**Architecture:** Add hub-local auth and audit repositories backed by identical SQLite/PostgreSQL behavior. Keep route authorization explicit at handler boundaries. Keep frontend auth server-side through `APP_API_TOKEN`.

**Tech Stack:** Rust, axum, sqlx, SQLite, PostgreSQL, Next.js.

### Task 1: Add Phase 6 persistence

Create SQLite and PostgreSQL migrations for `api_tokens` and `audit_events`.

Success criteria:

- Migrations apply for both backends.
- Tables reference existing `tenants` and `users` rows.

### Task 2: Add auth and audit repositories

Implement backend-neutral repositories for:

- creating users for tests/bootstrap paths
- creating API tokens from plaintext input by hashing before storage
- authenticating bearer tokens
- recording and listing audit events

Success criteria:

- Repository tests pass against SQLite.
- PostgreSQL repository tests are present under the existing optional test harness.

### Task 3: Enforce tenant route authorization

Add request authorization helpers and require:

- viewer for read routes and WebSocket subscription
- operator for refresh and job creation
- tenant_admin for agent creation

Success criteria:

- Missing/invalid tokens return `401`.
- Cross-tenant tokens return `403`.
- Viewer cannot create jobs or agent commands.

### Task 4: Record audit events

Record audit rows after successful user-triggered mutations:

- agent creation
- printer refresh command creation
- print job creation

Success criteria:

- Tests assert audit rows exist after successful mutations.

### Task 5: Frontend and deployment examples

Forward `APP_API_TOKEN` from Next.js server fetches/actions. Add SQLite and PostgreSQL compose examples and document credential policy.

Success criteria:

- Frontend build succeeds.
- Compose files clearly expose required API token env values.

### Task 6: Verify and finish

Run formatting, linting, workspace tests, frontend build, generated protobuf ignore check, update roadmap, review diff, commit, and push.

Success criteria:

- `cargo fmt --check`
- `cargo clippy --workspace`
- `cargo nextest run --manifest-path "Cargo.toml" --workspace`
- `npm run build` in `frontend`
- no committed protobuf generated Rust files
