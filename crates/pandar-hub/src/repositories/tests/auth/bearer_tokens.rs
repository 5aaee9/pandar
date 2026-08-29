use super::*;

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
