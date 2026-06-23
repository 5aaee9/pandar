use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};

use super::{AgentRepository, AuthRepository, RepositoryError, TenantRepository, sqlite_database};
use crate::entities::{agents, plugin_login_tickets, tenant_tokens};

#[test]
fn phase_16_migrations_are_backend_equivalent() {
    let sqlite = include_str!(
        "../../../migrations/sqlite/20260623020000_phase_16_tenant_tokens_agent_credentials.sql"
    );
    let postgres = include_str!(
        "../../../migrations/postgres/20260623020000_phase_16_tenant_tokens_agent_credentials.sql"
    );

    assert_eq!(sqlite, postgres);
    assert!(!sqlite.contains("UNIQUE(tenant_id, name)"));
    assert!(!sqlite.contains("UNIQUE (tenant_id, name)"));
}

#[tokio::test]
async fn sqlite_phase_16_entities_match_migration_shape() {
    let database = sqlite_database().await;
    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let tenant = tenants.create("phase-16", "Phase 16").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let connection = database.sea_orm_connection();

    tenant_tokens::ActiveModel {
        id: Set("tenant-token-1".to_owned()),
        tenant_id: Set(tenant.id.to_string()),
        name: Set("CI".to_owned()),
        token_hash: Set("tenant-token-hash-1".to_owned()),
        scopes_json: Set(r#"["jobs:read"]"#.to_owned()),
        created_by_user_id: Set(None),
        created_at: Set("2026-06-23T02:00:00Z".to_owned()),
        last_used_at: Set(None),
        expires_at: Set(None),
        revoked_at: Set(None),
    }
    .insert(&connection)
    .await
    .unwrap();

    plugin_login_tickets::ActiveModel {
        id: Set("plugin-ticket-1".to_owned()),
        tenant_id: Set(tenant.id.to_string()),
        user_id: Set(None),
        ticket_hash: Set("plugin-ticket-hash-1".to_owned()),
        redirect_url: Set("https://example.test/callback".to_owned()),
        created_at: Set("2026-06-23T02:00:00Z".to_owned()),
        expires_at: Set("2026-06-23T02:05:00Z".to_owned()),
        used_at: Set(None),
        revoked_at: Set(None),
    }
    .insert(&connection)
    .await
    .unwrap();

    let stored_agent = agents::Entity::find_by_id(agent.id.to_string())
        .one(&connection)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(stored_agent.credential_hash, None);
    assert_eq!(stored_agent.credential_rotated_at, None);
    assert_eq!(stored_agent.credential_revoked_at, None);
}

#[tokio::test]
async fn tenant_tokens_create_authenticate_update_last_used_rotate_and_revoke() {
    let database = sqlite_database().await;
    let tenants = TenantRepository::new(database.clone());
    let auth = AuthRepository::new(database);
    let tenant = tenants.create("tokens", "Tokens").await.unwrap();
    let admin = auth
        .create_user(
            tenant.id,
            "admin@example.test",
            "Admin",
            crate::repositories::UserRole::TenantAdmin,
        )
        .await
        .unwrap();

    let created = auth
        .create_tenant_token_with_audit(
            tenant.id,
            "Studio",
            vec![
                crate::repositories::TenantTokenScope::All,
                crate::repositories::TenantTokenScope::AgentRegister,
            ],
            None,
            crate::repositories::AuditActor::user(admin.id.clone()),
        )
        .await
        .unwrap();
    assert_eq!(created.token.created_by_user_id, Some(admin.id.clone()));
    assert_eq!(
        created.token.scopes,
        vec![
            crate::repositories::TenantTokenScope::All,
            crate::repositories::TenantTokenScope::AgentRegister
        ]
    );
    assert!(created.plaintext_token.starts_with("pandar_tenant_"));
    assert_ne!(created.plaintext_token, "tenant-token-secret");

    let stored = auth.list_tenant_tokens(tenant.id).await.unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].last_used_at, None);

    let authenticated = auth
        .authenticate_tenant_token(&created.plaintext_token)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(authenticated.token.id, created.token.id);
    assert_eq!(authenticated.token.tenant_id, tenant.id);
    assert!(authenticated.token.last_used_at.is_none());

    let stored = auth.list_tenant_tokens(tenant.id).await.unwrap();
    let last_used_at = stored[0].last_used_at.clone().unwrap();

    let rotated = auth
        .rotate_tenant_token_with_audit(
            tenant.id,
            &created.token.id,
            None,
            crate::repositories::AuditActor::user(admin.id),
        )
        .await
        .unwrap();
    assert!(rotated.plaintext_token.starts_with("pandar_tenant_"));
    assert_ne!(rotated.token.id, created.token.id);
    assert_eq!(rotated.token.name, created.token.name);
    assert_eq!(rotated.token.scopes, created.token.scopes);
    assert_eq!(rotated.token.last_used_at, None);
    assert!(
        auth.authenticate_tenant_token(&created.plaintext_token)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        auth.authenticate_tenant_token(&rotated.plaintext_token)
            .await
            .unwrap()
            .is_some()
    );
    let stored = auth.list_tenant_tokens(tenant.id).await.unwrap();
    let old = stored
        .iter()
        .find(|token| token.id == created.token.id)
        .unwrap();
    assert!(old.revoked_at.is_some());
    assert_eq!(old.last_used_at.as_deref(), Some(last_used_at.as_str()));

    let revoked = auth
        .revoke_tenant_token_with_audit(
            tenant.id,
            &rotated.token.id,
            crate::repositories::AuditActor::tenant_token(
                None,
                rotated.token.id.clone(),
                vec!["*"],
            ),
        )
        .await
        .unwrap();
    assert!(revoked.revoked_at.is_some());
    assert!(
        auth.authenticate_tenant_token(&rotated.plaintext_token)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn tenant_tokens_reject_unknown_scopes_and_expired_tokens() {
    let database = sqlite_database().await;
    let tenants = TenantRepository::new(database.clone());
    let auth = AuthRepository::new(database.clone());
    let tenant = tenants.create("invalid", "Invalid").await.unwrap();
    let connection = database.sea_orm_connection();

    tenant_tokens::ActiveModel {
        id: Set("invalid-scope".to_owned()),
        tenant_id: Set(tenant.id.to_string()),
        name: Set("Invalid".to_owned()),
        token_hash: Set(crate::repositories::auth::hash_token(
            "invalid-scope-secret",
        )),
        scopes_json: Set(r#"["unknown:scope"]"#.to_owned()),
        created_by_user_id: Set(None),
        created_at: Set("2026-06-23T02:00:00Z".to_owned()),
        last_used_at: Set(None),
        expires_at: Set(None),
        revoked_at: Set(None),
    }
    .insert(&connection)
    .await
    .unwrap();
    assert!(matches!(
        auth.list_tenant_tokens(tenant.id).await.unwrap_err(),
        RepositoryError::InvalidTokenScope(_)
    ));

    tenant_tokens::Entity::delete_by_id("invalid-scope")
        .exec(&connection)
        .await
        .unwrap();
    tenant_tokens::ActiveModel {
        id: Set("expired".to_owned()),
        tenant_id: Set(tenant.id.to_string()),
        name: Set("Expired".to_owned()),
        token_hash: Set(crate::repositories::auth::hash_token("expired-secret")),
        scopes_json: Set(r#"["*"]"#.to_owned()),
        created_by_user_id: Set(None),
        created_at: Set("2026-06-23T02:00:00Z".to_owned()),
        last_used_at: Set(None),
        expires_at: Set(Some("2000-01-01T00:00:00Z".to_owned())),
        revoked_at: Set(None),
    }
    .insert(&connection)
    .await
    .unwrap();

    assert!(
        auth.authenticate_tenant_token("expired-secret")
            .await
            .unwrap()
            .is_none()
    );
    let stored = tenant_tokens::Entity::find_by_id("expired")
        .one(&connection)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.last_used_at, None);
}

#[tokio::test]
async fn plugin_login_ticket_exchange_is_one_use_and_creates_plugin_token() {
    let database = sqlite_database().await;
    let tenants = TenantRepository::new(database.clone());
    let auth = AuthRepository::new(database.clone());
    let tenant = tenants
        .create("plugin-login", "Plugin Login")
        .await
        .unwrap();
    let admin = auth
        .create_user(
            tenant.id,
            "admin@example.test",
            "Admin",
            crate::repositories::UserRole::TenantAdmin,
        )
        .await
        .unwrap();
    let expires_at = future_rfc3339();
    let ticket = auth
        .create_plugin_login_ticket_with_audit(
            tenant.id,
            Some(admin.id.clone()),
            "http://localhost:4100/callback?state=abc",
            expires_at,
            crate::repositories::AuditActor::user(admin.id.clone()),
        )
        .await
        .unwrap();

    assert!(ticket.plaintext_ticket.starts_with("pandar_plugin_ticket_"));
    assert_eq!(
        ticket.ticket.redirect_url,
        "http://localhost:4100/callback?state=abc"
    );

    let exchanged = auth
        .exchange_plugin_login_ticket(&ticket.plaintext_ticket)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(exchanged.redirect_url, ticket.ticket.redirect_url);
    assert!(
        exchanged
            .tenant_token
            .plaintext_token
            .starts_with("pandar_plugin_")
    );
    assert_eq!(exchanged.tenant_token.token.tenant_id, tenant.id);
    assert_eq!(
        exchanged.tenant_token.token.created_by_user_id,
        Some(admin.id)
    );
    assert!(exchanged.tenant_token.token.expires_at.is_some());
    assert_eq!(
        exchanged.tenant_token.token.scopes,
        vec![crate::repositories::TenantTokenScope::PluginStudio]
    );

    assert!(
        auth.exchange_plugin_login_ticket(&ticket.plaintext_ticket)
            .await
            .unwrap()
            .is_none()
    );

    let stored_ticket = plugin_login_tickets::Entity::find_by_id(ticket.ticket.id)
        .one(&database.sea_orm_connection())
        .await
        .unwrap()
        .unwrap();
    assert!(stored_ticket.used_at.is_some());

    let stored_token = auth
        .authenticate_tenant_token(&exchanged.tenant_token.plaintext_token)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored_token.token.id, exchanged.tenant_token.token.id);
}

#[tokio::test]
async fn plugin_login_ticket_exchange_rejects_expired_tickets() {
    let database = sqlite_database().await;
    let tenants = TenantRepository::new(database.clone());
    let auth = AuthRepository::new(database.clone());
    let tenant = tenants
        .create("plugin-expired", "Plugin Expired")
        .await
        .unwrap();

    let ticket = auth
        .create_plugin_login_ticket_with_audit(
            tenant.id,
            None,
            "http://127.0.0.1:4100/callback",
            future_rfc3339(),
            crate::repositories::AuditActor::tenant_token(None, "setup", vec!["*"]),
        )
        .await
        .unwrap();
    let connection = database.sea_orm_connection();
    let mut active: plugin_login_tickets::ActiveModel =
        plugin_login_tickets::Entity::find_by_id(ticket.ticket.id.clone())
            .one(&connection)
            .await
            .unwrap()
            .unwrap()
            .into();
    active.expires_at = Set(past_rfc3339());
    active.update(&connection).await.unwrap();

    assert!(
        auth.exchange_plugin_login_ticket(&ticket.plaintext_ticket)
            .await
            .unwrap()
            .is_none()
    );

    let stored_ticket = plugin_login_tickets::Entity::find_by_id(ticket.ticket.id)
        .one(&database.sea_orm_connection())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored_ticket.used_at, None);
}

#[tokio::test]
async fn plugin_redirect_validation_allows_localhost_http_with_path_and_query_only() {
    let auth = AuthRepository::new(sqlite_database().await);
    assert_eq!(
        auth.validate_plugin_redirect_url("http://localhost:3000/callback?code=1")
            .unwrap(),
        "http://localhost:3000/callback?code=1"
    );
    assert_eq!(
        auth.validate_plugin_redirect_url("http://127.0.0.1:1/")
            .unwrap(),
        "http://127.0.0.1:1/"
    );
    assert_eq!(
        auth.validate_plugin_redirect_url("http://[::1]:65535/plugin")
            .unwrap(),
        "http://[::1]:65535/plugin"
    );

    for invalid in [
        "https://localhost:3000/callback",
        "http://example.test:3000/callback",
        "http://localhost/callback",
        "http://localhost:0/callback",
        "http://localhost:65536/callback",
        "http://user@localhost:3000/callback",
        "http://localhost:3000/callback#fragment",
    ] {
        assert!(auth.validate_plugin_redirect_url(invalid).is_err());
    }
}

#[tokio::test]
async fn audit_events_can_be_listed_newest_first_with_filters() {
    let database = sqlite_database().await;
    let tenants = TenantRepository::new(database.clone());
    let audit = crate::repositories::AuditEventRepository::new(database);
    let tenant = tenants
        .create("audit-filter", "Audit Filter")
        .await
        .unwrap();

    audit
        .record(crate::repositories::RecordAuditEvent {
            tenant_id: tenant.id,
            actor_type: "user".to_owned(),
            user_id: None,
            action: "plugin_login_ticket.create".to_owned(),
            target_type: "plugin_login_ticket".to_owned(),
            target_id: Some("ticket-1".to_owned()),
            metadata_json: "{}".to_owned(),
        })
        .await
        .unwrap();
    audit
        .record(crate::repositories::RecordAuditEvent {
            tenant_id: tenant.id,
            actor_type: "user".to_owned(),
            user_id: None,
            action: "plugin_login_ticket.exchange".to_owned(),
            target_type: "plugin_login_ticket".to_owned(),
            target_id: Some("ticket-1".to_owned()),
            metadata_json: "{}".to_owned(),
        })
        .await
        .unwrap();

    let newest = audit
        .list_for_tenant_newest_first(tenant.id, 1, None, None)
        .await
        .unwrap();
    assert_eq!(newest.len(), 1);
    assert_eq!(newest[0].action, "plugin_login_ticket.exchange");

    let before = newest[0].created_at.clone();
    let previous = audit
        .list_for_tenant_newest_first(tenant.id, 100, Some(before), None)
        .await
        .unwrap();
    assert_eq!(previous.len(), 1);
    assert_eq!(previous[0].action, "plugin_login_ticket.create");

    let create = audit
        .list_for_tenant_newest_first(
            tenant.id,
            100,
            None,
            Some("plugin_login_ticket.create".to_owned()),
        )
        .await
        .unwrap();
    assert_eq!(create.len(), 1);
    assert_eq!(create[0].action, "plugin_login_ticket.create");
}

fn past_rfc3339() -> String {
    (OffsetDateTime::now_utc() - Duration::minutes(5))
        .format(&Rfc3339)
        .unwrap()
}

fn future_rfc3339() -> String {
    (OffsetDateTime::now_utc() + Duration::minutes(5))
        .format(&Rfc3339)
        .unwrap()
}
