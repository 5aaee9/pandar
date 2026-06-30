# Delete Offline Agents Design

## Scope

Add a tenant-scoped agent deletion flow that lets authorized users remove an agent only when the agent is not currently `online`.

This change covers:

- Rust Hub API and repository behavior.
- Frontend `/agents` row action and server action.
- Audit, route, repository, and frontend tests.
- Roadmap update after implementation.

This change does not cover:

- Bulk agent deletion.
- Soft-delete or agent archival.
- Deleting an agent while preserving its printers, commands, jobs, materials, or machine events.
- Direct manipulation of live agent sessions from the browser.

## Existing Context

Agents are tenant-owned rows in `agents`. Agent-owned rows already reference `agents(id)` with `ON DELETE CASCADE` in both SQLite and PostgreSQL migrations, including printers, commands, jobs, print reports, material snapshots, and machine events. Agent creation and listing are exposed at `GET/POST /api/v1/tenants/{tenant_id}/agents`. Agent credentials and pairing use tenant-admin or `agent:register` authorization, while ordinary agent creation uses tenant-admin authorization. The frontend `/agents` page renders `LinkedAgentsSection`, which already lists agent rows and exposes row-level discovery actions.

Agent status is persisted as the `pandar_core::AgentStatus` enum: `offline`, `connecting`, and `online`. The requested rule is interpreted as: deletion is allowed when the persisted status is any value other than `online`; deletion is rejected when it is `online`.

## Options Considered

### Recommended: Hard Delete With Online Guard

Add `DELETE /api/v1/tenants/{tenant_id}/agents/{agent_id}`. The repository loads the tenant-scoped agent, rejects `online`, records an `agent.delete` audit event, deletes the agent row in the same transaction, and relies on existing foreign-key cascades for agent-owned rows.

Tradeoffs:

- Keeps deletion behavior simple and matches the existing schema.
- Avoids new schema or legacy fallback behavior.
- Removes dependent operational history for that agent, which is acceptable for this task because the request is deletion, not archival.

### Alternative: Soft Delete

Add a deleted timestamp and hide deleted agents from lists.

Tradeoffs:

- Preserves history, but requires schema changes, list filtering, uniqueness decisions, auth decisions, and broader query updates.
- More complexity than the requested deletion support.

### Alternative: Frontend-Only Hide

Remove the agent from the UI list without deleting the server record.

Tradeoffs:

- Does not satisfy API deletion and leaves stale agents in backend lists, metrics, and references.
- Not acceptable for this request.

## API Design

Endpoint:

```text
DELETE /api/v1/tenants/{tenant_id}/agents/{agent_id}
```

Authorization:

- Require tenant-admin authorization, matching manual agent creation.
- Tenant tokens require `*` through the existing tenant-admin-principal path.
- `agent:register` alone is not enough to delete agents.

Request:

- No body.
- Invalid tenant IDs return `400 { "error": "invalid_tenant_id" }`.
- Invalid agent IDs return `400 { "error": "invalid_agent_id" }`.

Success response:

- Return `200` with the deleted `AgentResponse`, using the pre-delete agent data.

Error responses:

- Missing or cross-tenant agent returns `404 { "error": "agent_not_found" }`.
- Online agent returns `409 { "error": "agent_online" }`.
- Viewer/operator callers return the existing `403 { "error": "role_forbidden" }`.

## Repository Design

Add an `AgentRepository::delete_offline_with_audit(tenant_id, agent_id, actor)` method.

Behavior:

1. Start a transaction.
2. Load the agent by ID and require `agent.tenant_id == tenant_id`.
3. Convert the model to the domain `Agent` so invalid persisted statuses still use existing status validation.
4. If the status is `AgentStatus::Online`, return a new repository error that maps to `409 agent_online`.
5. Insert an audit event with action `agent.delete`, target type `agent`, target ID set to the agent ID, and metadata containing the deleted agent name and previous status.
6. Delete the agent row in the same transaction.
7. Commit and return the deleted domain agent.

Deletion uses SeaORM through the existing backend-neutral repository layer. It should not add raw backend-specific SQL or new migrations.

## Frontend Design

On `/agents`, update `LinkedAgentsSection` so each row has a destructive delete action for agents whose status is not `online`.

Behavior:

- The action appears in a new row-action column alongside discovery controls.
- The action uses the existing confirmation dialog pattern before submitting.
- The button is disabled for `online` agents and labels the state clearly.
- `offline` and `connecting` agents can be submitted.
- The server action calls the new DELETE endpoint and redirects back to `/agents?tenant=...&status=agent_deleted` on success, or `/agents?tenant=...&status=<api_error>` on failure.
- Add `agent_deleted` as a positive action status with English and Chinese translations.

The frontend should not perform its own final authorization or safety decision. The API remains authoritative; the UI only reflects the expected allowed state.

## Tests

Hub route tests:

- Tenant admin can delete an offline agent and the list no longer includes it.
- Deleting an online agent returns `409 agent_online` and preserves the row.
- Viewer cannot delete an agent.
- Cross-tenant or missing agent returns `404 agent_not_found`.
- Invalid agent ID returns `400 invalid_agent_id`.
- Successful deletion records `agent.delete` audit metadata.

Repository tests:

- SQLite repository deletion removes the agent and cascades agent-owned fixtures.
- SQLite repository deletion rejects `online` agents.
- PostgreSQL core repository behavior covers deletion when `PANDAR_TEST_POSTGRES_URL` is configured.

Frontend tests:

- `/agents` renders delete controls for non-online agents and disables them for online agents.
- The action status helper treats `agent_deleted` as a success and translates it.

## Acceptance Criteria

- API clients can delete `offline` and `connecting` agents with tenant-admin permission.
- API clients cannot delete `online` agents.
- Frontend users can delete a non-online agent from `/agents` with confirmation.
- Frontend does not offer an enabled delete action for online agents.
- Successful deletion is audited.
- Rust formatting, Clippy, Next/Vitest frontend checks, and workspace tests pass or any environment blocker is reported with exact output.
