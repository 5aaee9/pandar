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
async fn users_can_be_listed_and_roles_updated() {
    let database = sqlite_database().await;
    let tenants = TenantRepository::new(database.clone());
    let auth = AuthRepository::new(database);
    let tenant = tenants.create("acme-users", "Acme Users").await.unwrap();
    let user = auth
        .create_user(tenant.id, "viewer@example.test", "Viewer", UserRole::Viewer)
        .await
        .unwrap();

    assert_eq!(
        auth.list_users_for_tenant(tenant.id).await.unwrap(),
        vec![user.clone()]
    );

    let updated = auth
        .update_user_role(tenant.id, &user.id, UserRole::Operator)
        .await
        .unwrap();
    assert_eq!(updated.id, user.id);
    assert_eq!(updated.role, UserRole::Operator);
    assert_eq!(
        auth.list_users_for_tenant(tenant.id).await.unwrap()[0].role,
        UserRole::Operator
    );
}

#[tokio::test]
async fn duplicate_user_email_is_reported() {
    let database = sqlite_database().await;
    let tenants = TenantRepository::new(database.clone());
    let auth = AuthRepository::new(database);
    let acme = tenants
        .create("acme-duplicate-email", "Acme Duplicate Email")
        .await
        .unwrap();
    let beta = tenants
        .create("beta-duplicate-email", "Beta Duplicate Email")
        .await
        .unwrap();

    auth.create_user(acme.id, "user@example.test", "User", UserRole::Viewer)
        .await
        .unwrap();
    let duplicate = auth
        .create_user(acme.id, "user@example.test", "Other", UserRole::Operator)
        .await
        .unwrap_err();
    assert!(matches!(duplicate, RepositoryError::DuplicateUserEmail));

    auth.create_user(beta.id, "user@example.test", "User", UserRole::Viewer)
        .await
        .unwrap();
}

#[tokio::test]
async fn api_tokens_can_be_listed_and_revoked() {
    let database = sqlite_database().await;
    let tenants = TenantRepository::new(database.clone());
    let auth = AuthRepository::new(database);
    let tenant = tenants
        .create("acme-token-revoke", "Acme Token Revoke")
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
    let token = auth
        .create_api_token(tenant.id, &user.id, "admin", "revoked-token")
        .await
        .unwrap();

    assert_eq!(
        auth.list_api_tokens_for_user(tenant.id, &user.id)
            .await
            .unwrap(),
        vec![token.clone()]
    );
    assert!(
        auth.authenticate_bearer("revoked-token")
            .await
            .unwrap()
            .is_some()
    );

    let revoked = auth.revoke_api_token(tenant.id, &token.id).await.unwrap();
    assert_eq!(revoked.id, token.id);
    assert!(revoked.revoked_at.is_some());
    assert!(
        auth.authenticate_bearer("revoked-token")
            .await
            .unwrap()
            .is_none()
    );

    let revoked_again = auth.revoke_api_token(tenant.id, &token.id).await.unwrap();
    assert_eq!(revoked_again, revoked);

    let missing = auth
        .revoke_api_token(tenant.id, "missing-token")
        .await
        .unwrap_err();
    assert!(matches!(missing, RepositoryError::MissingApiToken));
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
async fn external_identities_can_be_listed_for_user() {
    let database = sqlite_database().await;
    let tenants = TenantRepository::new(database.clone());
    let auth = AuthRepository::new(database);
    let tenant = tenants
        .create("acme-identity-list", "Acme Identity List")
        .await
        .unwrap();
    let user = auth
        .create_user(tenant.id, "viewer@example.test", "Viewer", UserRole::Viewer)
        .await
        .unwrap();
    let other_user = auth
        .create_user(tenant.id, "other@example.test", "Other", UserRole::Viewer)
        .await
        .unwrap();

    let identity = auth
        .link_external_identity(tenant.id, &user.id, "clerk", "user_123")
        .await
        .unwrap();

    assert_eq!(
        auth.list_external_identities_for_user(tenant.id, &user.id)
            .await
            .unwrap(),
        vec![identity]
    );
    assert!(
        auth.list_external_identities_for_user(tenant.id, &other_user.id)
            .await
            .unwrap()
            .is_empty()
    );
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

    let duplicate_user = auth
        .create_user(
            tenant.id,
            "admin@example.test",
            "Duplicate",
            UserRole::Operator,
        )
        .await
        .unwrap_err();
    assert!(matches!(
        duplicate_user,
        RepositoryError::DuplicateUserEmail
    ));

    let authenticated = auth
        .authenticate_bearer("postgres-secret")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(authenticated.user.id, user.id);
    assert_eq!(authenticated.user.role, UserRole::TenantAdmin);

    let token = auth
        .list_api_tokens_for_user(tenant.id, &user.id)
        .await
        .unwrap()
        .pop()
        .unwrap();
    let revoked = auth.revoke_api_token(tenant.id, &token.id).await.unwrap();
    assert!(revoked.revoked_at.is_some());
    assert!(
        auth.authenticate_bearer("postgres-secret")
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        auth.revoke_api_token(tenant.id, &token.id).await.unwrap(),
        revoked
    );

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
