use super::*;

#[tokio::test]
async fn postgres_auth_and_audit_repository_behavior_when_configured() {
    let Some(database) = crate::repositories::tests::postgres::postgres_database().await else {
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
    let Some(database) = crate::repositories::tests::postgres::postgres_database().await else {
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
