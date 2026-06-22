# Phase 6 Multi-Tenant Hardening Design

## Goal

Phase 6 adds tenant-scoped authentication, authorization, audit records, frontend API token forwarding, and deployment examples without introducing a browser login system yet.

## Scope

- Add API token authentication for HTTP and WebSocket tenant APIs.
- Enforce tenant matching and role checks on tenant-scoped routes.
- Add tenant roles: `tenant_admin`, `operator`, and `viewer`.
- Add durable audit events for user-triggered agent and printer actions.
- Keep Bambu printer credentials agent-local; the hub must not store printer access codes.
- Add Docker Compose examples for SQLite and PostgreSQL deployments.

## Non-Goals

- No username/password login or session cookies.
- No global super-admin model.
- No real Bambu network calls from hub tests.
- No hub-side storage of printer access codes.

## Authorization Model

API clients send `Authorization: Bearer <token>`.

Tokens are stored hashed in `api_tokens`. A token authenticates one tenant user. Tenant-scoped routes reject requests where the token tenant does not match the path tenant.

Roles:

- `tenant_admin`: read/write tenant resources, agents, printers, and jobs.
- `operator`: read tenant resources and create operational commands/jobs.
- `viewer`: read tenant resources and subscribe to printer events.

## Audit Model

Audit events are stored in `audit_events` with tenant id, actor type, optional user id, action, target type, optional target id, JSON metadata, and timestamp.

Phase 6 records successful user-triggered mutations:

- create agent
- refresh printers
- create print job

## Frontend Integration

The Next.js frontend reads `APP_API_TOKEN` server-side and forwards it to the Rust API. This keeps browser storage out of Phase 6 and lets deployments put auth at the server boundary.

## Deployment

Compose examples cover:

- SQLite hub + frontend using persistent local volumes.
- PostgreSQL hub + frontend using a postgres service and persistent volumes.

Both examples set `APP_API_URL` and `APP_API_TOKEN` placeholders and expose the existing 8080/3000 ports.
