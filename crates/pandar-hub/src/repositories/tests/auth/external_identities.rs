use super::*;

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
        vec![identity.clone()]
    );
    assert_eq!(
        auth.list_external_identities_for_tenant(tenant.id)
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
async fn list_external_memberships_returns_linked_tenants_and_roles() {
    let database = sqlite_database().await;
    let tenants = TenantRepository::new(database.clone());
    let auth = AuthRepository::new(database);
    let acme = tenants
        .create("acme-membership", "Acme Membership")
        .await
        .unwrap();
    let beta = tenants
        .create("beta-membership", "Beta Membership")
        .await
        .unwrap();
    let acme_user = auth
        .create_user(acme.id, "alice@example.test", "Alice", UserRole::Viewer)
        .await
        .unwrap();
    let beta_user = auth
        .create_user(beta.id, "alice@example.test", "Alice", UserRole::Operator)
        .await
        .unwrap();
    auth.link_external_identity(acme.id, &acme_user.id, "clerk", "user_123")
        .await
        .unwrap();
    auth.link_external_identity(beta.id, &beta_user.id, "clerk", "user_123")
        .await
        .unwrap();

    let memberships = auth
        .list_external_memberships("clerk", "user_123")
        .await
        .unwrap();

    assert_eq!(memberships.len(), 2);
    assert_eq!(memberships[0].tenant.slug, "acme-membership");
    assert_eq!(memberships[0].user.role, UserRole::Viewer);
    assert_eq!(memberships[1].tenant.slug, "beta-membership");
    assert_eq!(memberships[1].user.role, UserRole::Operator);
}
