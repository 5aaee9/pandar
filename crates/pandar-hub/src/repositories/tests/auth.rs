use super::*;
use crate::repositories::{RecordAuditEvent, UserRole};

#[tokio::test]
async fn auth_repository_authenticates_hashed_tokens() {
    let database = sqlite_database().await;
    let auth = AuthRepository::new(database.clone());
    let audit = AuditEventRepository::new(database.clone());
    let tenants = TenantRepository::new(database);
    let tenant = tenants.create("acme-auth", "Acme Auth").await.unwrap();
    let user = auth
        .create_user(
            tenant.id,
            "admin@example.test",
            "Admin",
            UserRole::TenantAdmin,
        )
        .await
        .unwrap();
    auth.create_api_token(tenant.id, &user.id, "admin", "secret-token")
        .await
        .unwrap();

    let authenticated = auth
        .authenticate_bearer("secret-token")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(authenticated.user.id, user.id);
    assert_eq!(authenticated.user.role, UserRole::TenantAdmin);
    assert!(
        auth.authenticate_bearer("other-token")
            .await
            .unwrap()
            .is_none()
    );

    audit
        .record(RecordAuditEvent {
            tenant_id: tenant.id,
            actor_type: "user".to_owned(),
            user_id: Some(user.id),
            action: "agent.create".to_owned(),
            target_type: "agent".to_owned(),
            target_id: Some("agent-id".to_owned()),
            metadata_json: "{}".to_owned(),
        })
        .await
        .unwrap();
    let events = audit.list_for_tenant(tenant.id).await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].action, "agent.create");
}

#[tokio::test]
async fn api_tokens_must_belong_to_user_tenant() {
    let database = sqlite_database().await;
    let tenants = TenantRepository::new(database.clone());
    let auth = AuthRepository::new(database);
    let acme = tenants.create("acme-auth", "Acme Auth").await.unwrap();
    let beta = tenants.create("beta-auth", "Beta Auth").await.unwrap();
    let user = auth
        .create_user(
            acme.id,
            "admin@example.test",
            "Admin",
            UserRole::TenantAdmin,
        )
        .await
        .unwrap();

    let err = auth
        .create_api_token(beta.id, &user.id, "cross-tenant", "cross-tenant-token")
        .await
        .unwrap_err();
    assert!(matches!(err, RepositoryError::MissingUser));
    assert!(
        auth.authenticate_bearer("cross-tenant-token")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn external_identity_resolves_tenant_user_role() {
    let database = sqlite_database().await;
    let tenants = TenantRepository::new(database.clone());
    let auth = AuthRepository::new(database);
    let tenant = tenants
        .create("acme-identity", "Acme Identity")
        .await
        .unwrap();
    let user = auth
        .create_user(tenant.id, "viewer@example.test", "Viewer", UserRole::Viewer)
        .await
        .unwrap();

    let identity = auth
        .link_external_identity(tenant.id, &user.id, "clerk", "user_123")
        .await
        .unwrap();
    let authenticated = auth
        .authenticate_external_identity(tenant.id, "clerk", "user_123")
        .await
        .unwrap()
        .unwrap();

    assert_eq!(identity.tenant_id, tenant.id);
    assert_eq!(identity.user_id, user.id);
    assert_eq!(authenticated.token_id, identity.id);
    assert_eq!(authenticated.user.id, user.id);
    assert_eq!(authenticated.user.role, UserRole::Viewer);
}

#[tokio::test]
async fn external_identity_rejects_missing_and_duplicate_links() {
    let database = sqlite_database().await;
    let tenants = TenantRepository::new(database.clone());
    let auth = AuthRepository::new(database);
    let tenant = tenants
        .create("acme-identity-duplicates", "Acme Identity")
        .await
        .unwrap();
    let user = auth
        .create_user(
            tenant.id,
            "admin@example.test",
            "Admin",
            UserRole::TenantAdmin,
        )
        .await
        .unwrap();

    let missing = auth
        .link_external_identity(tenant.id, "missing-user", "clerk", "user_missing")
        .await
        .unwrap_err();
    assert!(matches!(missing, RepositoryError::MissingUser));

    auth.link_external_identity(tenant.id, &user.id, "clerk", "user_123")
        .await
        .unwrap();

    let duplicate_identity = auth
        .link_external_identity(tenant.id, &user.id, "clerk", "user_123")
        .await
        .unwrap_err();
    assert!(matches!(
        duplicate_identity,
        RepositoryError::DuplicateExternalIdentity
    ));

    let duplicate_user_provider = auth
        .link_external_identity(tenant.id, &user.id, "clerk", "user_456")
        .await
        .unwrap_err();
    assert!(matches!(
        duplicate_user_provider,
        RepositoryError::DuplicateUserExternalIdentity
    ));
}

#[tokio::test]
async fn postgres_auth_and_audit_repository_behavior_when_configured() {
    let Some(database) = super::postgres::postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };

    let tenants = TenantRepository::new(database.clone());
    let auth = AuthRepository::new(database.clone());
    let audit = AuditEventRepository::new(database);
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let user = auth
        .create_user(
            tenant.id,
            "admin@example.test",
            "Admin",
            UserRole::TenantAdmin,
        )
        .await
        .unwrap();
    auth.create_api_token(tenant.id, &user.id, "admin", "postgres-secret")
        .await
        .unwrap();

    let authenticated = auth
        .authenticate_bearer("postgres-secret")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(authenticated.user.id, user.id);
    assert_eq!(authenticated.user.role, UserRole::TenantAdmin);

    let identity = auth
        .link_external_identity(tenant.id, &user.id, "logto", "logto-user")
        .await
        .unwrap();
    let external_authenticated = auth
        .authenticate_external_identity(tenant.id, "logto", "logto-user")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(external_authenticated.token_id, identity.id);
    assert_eq!(external_authenticated.user.id, user.id);
    assert_eq!(external_authenticated.user.role, UserRole::TenantAdmin);

    audit
        .record(RecordAuditEvent {
            tenant_id: tenant.id,
            actor_type: "user".to_owned(),
            user_id: Some(user.id),
            action: "job.create".to_owned(),
            target_type: "job".to_owned(),
            target_id: Some("job-id".to_owned()),
            metadata_json: "{}".to_owned(),
        })
        .await
        .unwrap();

    let events = audit.list_for_tenant(tenant.id).await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].action, "job.create");
}

#[tokio::test]
async fn postgres_external_identity_error_behavior_when_configured() {
    let Some(database) = super::postgres::postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };

    let tenants = TenantRepository::new(database.clone());
    let auth = AuthRepository::new(database);
    let tenant = tenants
        .create("postgres-identity-duplicates", "Postgres Identity")
        .await
        .unwrap();
    let user = auth
        .create_user(
            tenant.id,
            "admin@example.test",
            "Admin",
            UserRole::TenantAdmin,
        )
        .await
        .unwrap();

    let missing = auth
        .link_external_identity(tenant.id, "missing-user", "logto", "missing")
        .await
        .unwrap_err();
    assert!(matches!(missing, RepositoryError::MissingUser));

    auth.link_external_identity(tenant.id, &user.id, "logto", "subject-1")
        .await
        .unwrap();

    let duplicate_identity = auth
        .link_external_identity(tenant.id, &user.id, "logto", "subject-1")
        .await
        .unwrap_err();
    assert!(matches!(
        duplicate_identity,
        RepositoryError::DuplicateExternalIdentity
    ));

    let duplicate_user_provider = auth
        .link_external_identity(tenant.id, &user.id, "logto", "subject-2")
        .await
        .unwrap_err();
    assert!(matches!(
        duplicate_user_provider,
        RepositoryError::DuplicateUserExternalIdentity
    ));
}
