# Delete Offline Agents Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a tenant-admin API and `/agents` UI flow that deletes agents only when their persisted status is not `online`.

**Architecture:** The Hub owns the safety rule in `AgentRepository::delete_offline_with_audit`; the route parses and authorizes requests, then delegates to that repository method. The frontend adds a confirmed row action to the existing `/agents` linked-agent table and treats `agent_deleted` as a normal positive action status.

**Tech Stack:** Rust, axum, SeaORM, SQLx-backed SQLite/PostgreSQL tests, Next.js server actions, React, next-intl, Vitest.

## Global Constraints

- Database behavior must support SQLite and PostgreSQL through backend-neutral repository/query boundaries.
- Do not add migrations for this feature; existing `ON DELETE CASCADE` foreign keys define dependent-row removal.
- Deletion is allowed for persisted `AgentStatus` values other than `AgentStatus::Online`; `offline` and `connecting` are deletable.
- Deleting an `online` agent must return `409 { "error": "agent_online" }` and preserve the row.
- Missing or cross-tenant agents must return `404 { "error": "agent_not_found" }`.
- `agent:register` alone is not enough to delete agents; use tenant-admin authorization for the delete route.
- Successful deletion must record `agent.delete` with agent name and previous status in audit metadata.
- Update `docs/roadmap.md` after implementation.
- Keep changes simple and scoped; do not add soft-delete, archival, bulk delete, or preservation of dependent rows.

---

## File Structure

- `crates/pandar-hub/src/repositories/mod.rs`: add `RepositoryError::AgentOnline`.
- `crates/pandar-hub/src/repositories/agents.rs`: add backend-neutral transactional delete method.
- `crates/pandar-hub/src/repositories/tests/phase1.rs`: add SQLite repository red/green tests.
- `crates/pandar-hub/src/repositories/tests/postgres.rs`: extend configured PostgreSQL core behavior with delete.
- `crates/pandar-hub/src/routes.rs`: register the DELETE route and map `AgentOnline` to `409 agent_online`.
- `crates/pandar-hub/src/routes/agents.rs`: add route handler and local agent ID parser.
- `crates/pandar-hub/src/routes/tests/agents.rs`: add API red/green tests.
- `frontend/app/actions.ts`: add `deleteAgent` server action and an agents-page status URL helper.
- `frontend/app/diagnostics-panel.tsx`: add confirmed delete row action to `LinkedAgentsSection`.
- `frontend/app/action-status.ts`: add `agent_deleted` as a positive action status.
- `frontend/messages/en.json` and `frontend/messages/zh.json`: add delete/status copy.
- `frontend/app/agent-pairing-guidance.test.tsx`: update action mock and test linked-agent delete controls.
- `frontend/app/action-status-toast.test.tsx`: add success classification/translation expectation.
- `docs/roadmap.md`: record the completed delete-offline-agent capability.

---

### Task 1: Hub Delete API And Repository

**Files:**

- Modify: `crates/pandar-hub/src/repositories/mod.rs`
- Modify: `crates/pandar-hub/src/repositories/agents.rs`
- Modify: `crates/pandar-hub/src/repositories/tests/phase1.rs`
- Modify: `crates/pandar-hub/src/repositories/tests/postgres.rs`
- Modify: `crates/pandar-hub/src/routes.rs`
- Modify: `crates/pandar-hub/src/routes/agents.rs`
- Modify: `crates/pandar-hub/src/routes/tests/agents.rs`

**Interfaces:**

- Produces: `AgentRepository::delete_offline_with_audit(&self, tenant_id: TenantId, agent_id: AgentId, actor: AuditActor) -> RepositoryResult<Agent>`.
- Produces: `RepositoryError::AgentOnline` mapped to `409 agent_online`.
- Produces: `DELETE /api/v1/tenants/{tenant_id}/agents/{agent_id}` returning `AgentResponse`.

- [ ] **Step 1: Add failing repository tests**

Add these tests to `crates/pandar-hub/src/repositories/tests/phase1.rs` near `agent_get_update_connection_and_mark_offline_work`:

```rust
#[tokio::test]
async fn agent_delete_offline_removes_agent_cascades_and_audits() {
    let (database, tenants, agents, printers, commands, _) = repositories().await;
    let tenant = tenants.create("delete-acme", "Delete Acme").await.unwrap();
    let agent = agents.create(tenant.id, "stale-agent").await.unwrap();
    let printer_id = insert_printer_fixture(&database, tenant.id, agent.id)
        .await
        .unwrap();
    insert_command_fixture(&database, tenant.id, agent.id, Some(&printer_id))
        .await
        .unwrap();

    let deleted = agents
        .delete_offline_with_audit(tenant.id, agent.id, crate::repositories::AuditActor::user("test-user"))
        .await
        .unwrap();

    assert_eq!(deleted, agent);
    assert_eq!(agents.get(agent.id).await.unwrap(), None);
    assert_eq!(printers.count().await.unwrap(), 0);
    assert_eq!(commands.count().await.unwrap(), 0);

    let audit = AuditEventRepository::new(database);
    let events = audit.list_for_tenant(tenant.id).await.unwrap();
    let event = events
        .iter()
        .find(|event| event.action == "agent.delete")
        .expect("agent delete audit event");
    assert_eq!(event.target_type, "agent");
    assert_eq!(event.target_id.as_deref(), Some(&agent.id.to_string()));
    assert_eq!(event.metadata["agent_name"], "stale-agent");
    assert_eq!(event.metadata["previous_status"], "offline");
}

#[tokio::test]
async fn agent_delete_rejects_online_agent() {
    let (_, tenants, agents, _, _, _) = repositories().await;
    let tenant = tenants.create("online-acme", "Online Acme").await.unwrap();
    let agent = agents.create(tenant.id, "online-agent").await.unwrap();
    agents
        .update_connection(
            agent.id,
            AgentStatus::Online,
            Some("0.2.0"),
            "2026-06-20T01:00:00Z",
        )
        .await
        .unwrap();

    let err = agents
        .delete_offline_with_audit(tenant.id, agent.id, crate::repositories::AuditActor::user("test-user"))
        .await
        .unwrap_err();

    assert!(matches!(err, RepositoryError::AgentOnline));
    assert_eq!(
        agents.get(agent.id).await.unwrap().unwrap().status,
        AgentStatus::Online
    );
}
```

Also extend `postgres_core_repository_behavior_when_configured` in `crates/pandar-hub/src/repositories/tests/postgres.rs` after command/printer count assertions:

```rust
    let stale = agents.create(tenant.id, "stale-agent").await.unwrap();
    let deleted = agents
        .delete_offline_with_audit(tenant.id, stale.id, AuditActor::user("postgres-test-user"))
        .await
        .unwrap();
    assert_eq!(deleted, stale);
    assert_eq!(agents.get(stale.id).await.unwrap(), None);
```

- [ ] **Step 2: Run repository tests and verify RED**

Run:

```bash
cargo test -p pandar-hub repositories::tests::phase1::agent_delete
```

Expected: fails to compile because `delete_offline_with_audit` and `RepositoryError::AgentOnline` do not exist.

- [ ] **Step 3: Implement repository error and delete method**

In `crates/pandar-hub/src/repositories/mod.rs`, add this enum variant after `MissingAgent`:

```rust
    #[error("agent is online")]
    AgentOnline,
```

In `crates/pandar-hub/src/repositories/agents.rs`, add imports if missing:

```rust
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, TransactionTrait};
```

Add this method inside `impl AgentRepository` after `get`:

```rust
    pub async fn delete_offline_with_audit(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
        actor: AuditActor,
    ) -> RepositoryResult<Agent> {
        let connection = self.database.sea_orm_connection();
        let tx = connection
            .begin()
            .await
            .context("failed to begin agent delete audit transaction")?;
        let Some(model) = agents::Entity::find_by_id(agent_id.to_string())
            .one(&tx)
            .await
            .context("failed to get agent before delete")?
        else {
            return Err(RepositoryError::MissingAgent);
        };
        if model.tenant_id != tenant_id.to_string() {
            return Err(RepositoryError::MissingAgent);
        }

        let agent = agent_from_model(model)?;
        if agent.status == AgentStatus::Online {
            return Err(RepositoryError::AgentOnline);
        }

        insert_audit_event_tx(
            &tx,
            &record_audit_event(
                tenant_id,
                actor,
                "agent.delete",
                "agent",
                Some(agent_id.to_string()),
                serde_json::json!({
                    "agent_name": agent.name.clone(),
                    "previous_status": agent.status.as_str(),
                }),
            ),
        )
        .await?;
        agents::Entity::delete_by_id(agent_id.to_string())
            .exec(&tx)
            .await
            .context("failed to delete agent")?;
        tx.commit()
            .await
            .context("failed to commit agent delete audit transaction")?;

        Ok(agent)
    }
```

- [ ] **Step 4: Run repository tests and verify GREEN**

Run:

```bash
cargo test -p pandar-hub repositories::tests::phase1::agent_delete
```

Expected: tests pass.

- [ ] **Step 5: Add failing route tests**

Append these tests to `crates/pandar-hub/src/routes/tests/agents.rs`:

```rust
#[tokio::test]
async fn tenant_admin_can_delete_offline_agent() {
    let state = state().await;
    let app = router(state.clone());
    let (_, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = agent["tenant_id"].as_str().unwrap();
    let agent_id = agent["id"].as_str().unwrap();

    let (status, body) = request_as(
        app.clone(),
        Method::DELETE,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, agent);

    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/agents"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "agents": [] }));

    let events = state
        .audit_events()
        .list_for_tenant(TenantId::parse(tenant_id).unwrap())
        .await
        .unwrap();
    let event = events
        .iter()
        .find(|event| event.action == "agent.delete")
        .expect("agent delete audit event");
    assert_eq!(event.target_id.as_deref(), Some(agent_id));
    assert_eq!(event.metadata["agent_name"], "shop-agent");
    assert_eq!(event.metadata["previous_status"], "offline");
}

#[tokio::test]
async fn agent_delete_rejects_online_agent() {
    let state = state().await;
    let app = router(state.clone());
    let (_, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(agent["tenant_id"].as_str().unwrap()).unwrap();
    let agent_id = pandar_core::AgentId::parse(agent["id"].as_str().unwrap()).unwrap();
    state
        .agents()
        .update_connection(
            agent_id,
            pandar_core::AgentStatus::Online,
            Some("0.2.0"),
            "2026-06-20T01:00:00Z",
        )
        .await
        .unwrap();

    let (status, body) = request_as(
        app,
        Method::DELETE,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body, json!({ "error": "agent_online" }));
    assert!(state.agents().get(agent_id).await.unwrap().is_some());
}

#[tokio::test]
async fn viewer_cannot_delete_agent() {
    let state = state().await;
    let app = router(state.clone());
    let (_, agent, _) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = agent["tenant_id"].as_str().unwrap();
    let token = auth_token_for_role(
        &state,
        tenant_id,
        crate::repositories::UserRole::Viewer,
        "viewer-delete-agent",
    )
    .await;

    let (status, body) = request_as(
        app,
        Method::DELETE,
        &format!("/api/v1/tenants/{}/agents/{}", tenant_id, agent["id"].as_str().unwrap()),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body, json!({ "error": "role_forbidden" }));
}

#[tokio::test]
async fn agent_delete_rejects_invalid_or_missing_agent() {
    let state = state().await;
    let app = router(state.clone());
    let (_, tenant) = create_tenant_for_test(app.clone()).await;
    let tenant_id = tenant["id"].as_str().unwrap();
    let token = auth_token_for_role(
        &state,
        tenant_id,
        crate::repositories::UserRole::TenantAdmin,
        "delete-missing-agent",
    )
    .await;

    let (status, body) = request_as(
        app.clone(),
        Method::DELETE,
        &format!("/api/v1/tenants/{tenant_id}/agents/not-a-uuid"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, json!({ "error": "invalid_agent_id" }));

    let (status, body) = request_as(
        app,
        Method::DELETE,
        &format!("/api/v1/tenants/{tenant_id}/agents/00000000-0000-0000-0000-000000000001"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, json!({ "error": "agent_not_found" }));
}
```

- [ ] **Step 6: Run route tests and verify RED**

Run:

```bash
cargo test -p pandar-hub routes::tests::agents::
```

Expected: route tests fail with `405 Method Not Allowed` or missing route handler.

- [ ] **Step 7: Implement route and error mapping**

In `crates/pandar-hub/src/routes.rs`, change the import to include `delete` if needed:

```rust
routing::{delete, get, post},
```

Update the agents route registration:

```rust
        .route(
            "/api/v1/tenants/{tenant_id}/agents",
            get(agents::list_agents).post(agents::create_agent),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/agents/{agent_id}",
            delete(agents::delete_agent),
        )
```

In `impl From<RepositoryError> for ApiError`, add:

```rust
            RepositoryError::AgentOnline => Self::new(StatusCode::CONFLICT, "agent_online"),
```

In `crates/pandar-hub/src/routes/agents.rs`, add `AgentId` to imports:

```rust
use pandar_core::AgentId;
```

Add this handler after `list_agents`:

```rust
pub(in crate::routes) async fn delete_agent(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((tenant_id, agent_id)): Path<(String, String)>,
) -> Result<Json<AgentResponse>, ApiError> {
    let tenant_id = parse_tenant_id(&tenant_id)?;
    let agent_id = parse_agent_id(&agent_id)?;
    let auth = auth::authorize_tenant_admin_principal(&state, &headers, tenant_id).await?;
    let deleted = state
        .agents()
        .delete_offline_with_audit(tenant_id, agent_id, auth::audit_actor(&auth))
        .await?;

    Ok(Json(AgentResponse::from(deleted)))
}

fn parse_agent_id(value: &str) -> Result<AgentId, ApiError> {
    AgentId::parse(value).map_err(|_| ApiError::bad_request("invalid_agent_id"))
}
```

- [ ] **Step 8: Run backend tests and verify GREEN**

Run:

```bash
cargo test -p pandar-hub repositories::tests::phase1::agent_delete
cargo test -p pandar-hub routes::tests::agents::
```

Expected: tests pass.

---

### Task 2: Frontend Delete Action

**Files:**

- Modify: `frontend/app/actions.ts`
- Modify: `frontend/app/diagnostics-panel.tsx`
- Modify: `frontend/app/action-status.ts`
- Modify: `frontend/messages/en.json`
- Modify: `frontend/messages/zh.json`
- Modify: `frontend/app/agent-pairing-guidance.test.tsx`
- Modify: `frontend/app/action-status-toast.test.tsx`

**Interfaces:**

- Consumes: `DELETE /api/v1/tenants/{tenant_id}/agents/{agent_id}` from Task 1.
- Produces: `deleteAgent(formData: FormData)` server action.
- Produces: `agent_deleted` positive action status.

- [ ] **Step 1: Add failing frontend tests**

In `frontend/app/agent-pairing-guidance.test.tsx`, add `deleteAgent: vi.fn(),` to the `vi.mock("./actions", ...)` object.

Add this test inside `describe("Agents view pairing guidance", ...)`:

```tsx
it("renders delete controls only for agents that are not online", () => {
  renderAgentsView({
    agents: [
      {
        id: "agent-offline",
        tenant_id: tenant.id,
        name: "Offline agent",
        status: "offline",
        created_at: "2026-06-30T00:00:00Z",
      },
      {
        id: "agent-online",
        tenant_id: tenant.id,
        name: "Online agent",
        status: "online",
        created_at: "2026-06-30T00:00:00Z",
      },
    ],
  });

  expect(
    screen.getByRole("button", { name: "Delete Offline agent" }),
  ).toBeEnabled();
  expect(
    screen.getByRole("button", { name: "Online agent is online" }),
  ).toBeDisabled();
});
```

In `frontend/app/action-status-toast.test.tsx`, update the first helper test:

```tsx
expect(formatActionStatus("agent_deleted", tStatus)).toBe("Agent deleted");
```

Update the tone test:

```tsx
expect(actionStatusTone("agent_deleted")).toBe("success");
```

- [ ] **Step 2: Run frontend tests and verify RED**

Run:

```bash
npm run test:web -- app/agent-pairing-guidance.test.tsx app/action-status-toast.test.tsx
```

Expected: tests fail because delete controls and `agent_deleted` are not implemented.

- [ ] **Step 3: Implement server action and status routing**

In `frontend/app/actions.ts`, add this exported action after `refreshAllAgents`:

```ts
export async function deleteAgent(formData: FormData) {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id");
  const agentId = stringField(formData, "agent_id");
  const response = await fetch(
    `${apiUrl}/api/v1/tenants/${tenantId}/agents/${agentId}`,
    {
      method: "DELETE",
      headers: await apiHeaders("application/json"),
    },
  );
  redirect(
    agentsStatusUrl(
      tenantId,
      response.ok ? "agent_deleted" : await errorCode(response),
    ),
  );
}
```

Add this helper near `statusUrl`:

```ts
function agentsStatusUrl(tenantId: string, status: string) {
  return `/agents?tenant=${encodeURIComponent(tenantId)}&status=${encodeURIComponent(status)}`;
}
```

- [ ] **Step 4: Implement LinkedAgentsSection delete controls**

In `frontend/app/diagnostics-panel.tsx`, update imports:

```ts
import { deleteAgent, diagnosePrinter, discoverPrinters } from "./actions";
import { ConfirmForm } from "./confirm-dialog";
```

Add the action header after the discovery header:

```tsx
<th className="px-4 py-2">{t("colActions")}</th>
```

Add this table cell after the discovery form cell inside each agent row:

```tsx
<td className="px-4 py-3">
  <ConfirmForm
    action={deleteAgent}
    buttonClassName="h-9 rounded-md border border-red-300 px-3 text-sm font-medium text-red-700 disabled:border-slate-200 disabled:text-slate-400"
    buttonLabel={
      agent.status.toLowerCase() === "online"
        ? t("deleteOnline", { name: agent.name })
        : t("deleteAgent", { name: agent.name })
    }
    disabled={agent.status.toLowerCase() === "online"}
    title={t("deleteTitle")}
    message={t("deleteMessage", { name: agent.name })}
    confirmLabel={t("deleteConfirm")}
    tone="danger"
  >
    <input name="tenant_id" type="hidden" value={selectedTenant.id} />
    <input name="agent_id" type="hidden" value={agent.id} />
  </ConfirmForm>
</td>
```

- [ ] **Step 5: Add translations and positive status**

In `frontend/app/action-status.ts`, add `'agent_deleted',` to `knownPositiveActionStatuses`.

In `frontend/messages/en.json`, add:

```json
"agent_deleted": "Agent deleted"
```

inside `runtime.actionStatus`, and add these keys inside `diagnostics`:

```json
"colActions": "Actions",
"deleteAgent": "Delete {name}",
"deleteOnline": "{name} is online",
"deleteTitle": "Delete agent",
"deleteMessage": "Delete {name}? Its reported printers, commands, jobs, and machine events will be removed.",
"deleteConfirm": "Delete agent"
```

In `frontend/messages/zh.json`, add:

```json
"agent_deleted": "Agent 已删除"
```

inside `runtime.actionStatus`, and add these keys inside `diagnostics`:

```json
"colActions": "操作",
"deleteAgent": "删除 {name}",
"deleteOnline": "{name} 在线",
"deleteTitle": "删除 Agent",
"deleteMessage": "删除 {name}？其上报的打印机、命令、任务和机器事件也会被移除。",
"deleteConfirm": "删除 Agent"
```

- [ ] **Step 6: Run frontend tests and verify GREEN**

Run:

```bash
npm run test:web -- app/agent-pairing-guidance.test.tsx app/action-status-toast.test.tsx
```

Expected: tests pass.

---

### Task 3: Roadmap And Verification

**Files:**

- Modify: `docs/roadmap.md`

**Interfaces:**

- Consumes: implemented backend and frontend behavior from Tasks 1 and 2.
- Produces: roadmap entry documenting completed offline-agent deletion.

- [ ] **Step 1: Update roadmap**

Add this bullet under `## Phase 17: Tenant Admin Product UI` completed items in `docs/roadmap.md`:

```markdown
- Completed tenant-admin removal for stale linked agents: `/agents` now exposes a confirmed delete action for non-online agents, the Hub rejects online-agent deletion with `agent_online`, and successful removals are audited while existing agent-owned rows cascade through the database.
```

- [ ] **Step 2: Run formatting**

Run:

```bash
cargo fmt
```

Expected: exits 0.

- [ ] **Step 3: Run Rust lint**

Run:

```bash
cargo clippy --workspace --all-targets --all-features
```

Expected: exits 0.

- [ ] **Step 4: Run Rust workspace tests**

Run:

```bash
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Expected: exits 0. If `cargo-nextest` is unavailable, run `cargo test --workspace` and report the missing tool.

- [ ] **Step 5: Run frontend tests**

Run:

```bash
npm run test:web
```

Expected: exits 0.

- [ ] **Step 6: Review final diff**

Run:

```bash
git status --short
git diff --stat
git diff -- docs/superpowers/specs/2026-07-01-delete-offline-agents-design.md docs/superpowers/plans/2026-07-01-delete-offline-agents.md crates/pandar-hub/src/repositories/mod.rs crates/pandar-hub/src/repositories/agents.rs crates/pandar-hub/src/repositories/tests/phase1.rs crates/pandar-hub/src/repositories/tests/postgres.rs crates/pandar-hub/src/routes.rs crates/pandar-hub/src/routes/agents.rs crates/pandar-hub/src/routes/tests/agents.rs frontend/app/actions.ts frontend/app/diagnostics-panel.tsx frontend/app/action-status.ts frontend/messages/en.json frontend/messages/zh.json frontend/app/agent-pairing-guidance.test.tsx frontend/app/action-status-toast.test.tsx docs/roadmap.md
```

Expected: only planned files changed, with no unrelated refactors.
