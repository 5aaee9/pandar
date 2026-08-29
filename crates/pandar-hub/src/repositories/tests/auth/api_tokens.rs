use super::*;

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
async fn bearer_auth_updates_last_used_only_for_accepted_non_revoked_tokens() {
    let database = sqlite_database().await;
    let tenants = TenantRepository::new(database.clone());
    let auth = AuthRepository::new(database);
    let tenant = tenants
        .create("acme-token-last-used", "Acme Token Last Used")
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
    let accepted = auth
        .create_api_token(tenant.id, &user.id, "accepted", "accepted-token")
        .await
        .unwrap();
    let revoked = auth
        .create_api_token(tenant.id, &user.id, "revoked", "revoked-token")
        .await
        .unwrap();
    let revoked = auth.revoke_api_token(tenant.id, &revoked.id).await.unwrap();
    let revoked_at = revoked.revoked_at.clone();

    let tokens = auth
        .list_api_tokens_for_user(tenant.id, &user.id)
        .await
        .unwrap();
    assert!(tokens.iter().all(|token| token.last_used_at.is_none()));

    assert!(
        auth.authenticate_bearer("accepted-token")
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        auth.authenticate_bearer("revoked-token")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        auth.authenticate_bearer("invalid-token")
            .await
            .unwrap()
            .is_none()
    );

    let tokens = auth
        .list_api_tokens_for_user(tenant.id, &user.id)
        .await
        .unwrap();
    let accepted = tokens
        .iter()
        .find(|token| token.id == accepted.id)
        .expect("accepted token should still be listed");
    let revoked = tokens
        .iter()
        .find(|token| token.id == revoked.id)
        .expect("revoked token should still be listed");

    assert!(accepted.last_used_at.is_some());
    assert_eq!(revoked.last_used_at, None);
    assert_eq!(revoked.revoked_at, revoked_at);
    assert_eq!(
        tokens
            .iter()
            .filter(|token| token.last_used_at.is_some())
            .count(),
        1
    );
}
