use super::*;

#[tokio::test]
async fn join_link_create_list_revoke_hashes_token() {
    let database = sqlite_database().await;
    let tenants = TenantRepository::new(database.clone());
    let auth = AuthRepository::new(database);
    let tenant = tenants
        .create("acme-join-links", "Acme Join Links")
        .await
        .unwrap();
    let admin = auth
        .create_user(
            tenant.id,
            "admin@example.test",
            "Admin",
            UserRole::TenantAdmin,
        )
        .await
        .unwrap();

    let created = auth
        .create_join_link_with_audit(
            tenant.id,
            UserRole::Operator,
            Some("alice@example.test".to_owned()),
            60 * 60,
            1,
            AuditActor::user(admin.id.clone()),
        )
        .await
        .unwrap();

    assert!(created.plaintext_token.starts_with("pandar_join"));
    let listed = auth.list_join_links_for_tenant(tenant.id).await.unwrap();
    assert_eq!(listed, vec![created.join_link.clone()]);
    assert_ne!(created.plaintext_token, listed[0].id);
    assert_eq!(
        listed[0].created_by_user_id.as_deref(),
        Some(admin.id.as_str())
    );

    let revoked = auth
        .revoke_join_link_with_audit(tenant.id, &created.join_link.id, AuditActor::user(admin.id))
        .await
        .unwrap();
    assert!(revoked.revoked_at.is_some());
}

#[tokio::test]
async fn accept_join_link_creates_member_and_consumes_use() {
    let database = sqlite_database().await;
    let tenants = TenantRepository::new(database.clone());
    let auth = AuthRepository::new(database);
    let tenant = tenants
        .create("acme-join-accept", "Acme Join Accept")
        .await
        .unwrap();
    let admin = auth
        .create_user(
            tenant.id,
            "admin@example.test",
            "Admin",
            UserRole::TenantAdmin,
        )
        .await
        .unwrap();
    let link = auth
        .create_join_link_with_audit(
            tenant.id,
            UserRole::Operator,
            None,
            60 * 60,
            1,
            AuditActor::user(admin.id),
        )
        .await
        .unwrap();

    let accepted = auth
        .accept_join_link(
            &link.plaintext_token,
            profile("clerk", "new-member", "member@example.test", "Member"),
        )
        .await
        .unwrap();

    assert!(accepted.created);
    assert_eq!(accepted.tenant.id, tenant.id);
    assert_eq!(accepted.user.role, UserRole::Operator);
    let link_after = auth
        .list_join_links_for_tenant(tenant.id)
        .await
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(link_after.used_count, 1);
}

#[tokio::test]
async fn accept_join_link_existing_member_keeps_role_and_does_not_consume() {
    let database = sqlite_database().await;
    let tenants = TenantRepository::new(database.clone());
    let auth = AuthRepository::new(database);
    let tenant = tenants
        .create("acme-existing-member", "Acme Existing Member")
        .await
        .unwrap();
    let admin = auth
        .create_user(
            tenant.id,
            "admin@example.test",
            "Admin",
            UserRole::TenantAdmin,
        )
        .await
        .unwrap();
    auth.link_external_identity(tenant.id, &admin.id, "clerk", "existing")
        .await
        .unwrap();
    let link = auth
        .create_join_link_with_audit(
            tenant.id,
            UserRole::Viewer,
            None,
            60 * 60,
            1,
            AuditActor::user(admin.id.clone()),
        )
        .await
        .unwrap();

    let accepted = auth
        .accept_join_link(
            &link.plaintext_token,
            profile("clerk", "existing", "admin@example.test", "Admin"),
        )
        .await
        .unwrap();

    assert!(!accepted.created);
    assert_eq!(accepted.user.id, admin.id);
    assert_eq!(accepted.user.role, UserRole::TenantAdmin);
    let link_after = auth
        .list_join_links_for_tenant(tenant.id)
        .await
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(link_after.used_count, 0);
}

#[tokio::test]
async fn accept_join_link_existing_member_still_requires_matching_email_constraint() {
    let database = sqlite_database().await;
    let tenants = TenantRepository::new(database.clone());
    let auth = AuthRepository::new(database);
    let tenant = tenants
        .create("acme-existing-email", "Acme Existing Email")
        .await
        .unwrap();
    let admin = auth
        .create_user(
            tenant.id,
            "admin@example.test",
            "Admin",
            UserRole::TenantAdmin,
        )
        .await
        .unwrap();
    auth.link_external_identity(tenant.id, &admin.id, "clerk", "existing-email")
        .await
        .unwrap();
    let link = auth
        .create_join_link_with_audit(
            tenant.id,
            UserRole::Viewer,
            Some("allowed@example.test".to_owned()),
            60 * 60,
            1,
            AuditActor::user(admin.id),
        )
        .await
        .unwrap();

    assert!(matches!(
        auth.accept_join_link(
            &link.plaintext_token,
            profile("clerk", "existing-email", "changed@example.test", "Changed"),
        )
        .await
        .unwrap_err(),
        RepositoryError::JoinLinkEmailMismatch
    ));
    let link_after = auth
        .list_join_links_for_tenant(tenant.id)
        .await
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(link_after.used_count, 0);
}

#[tokio::test]
async fn concurrent_single_use_join_link_accept_creates_one_member() {
    let database = sqlite_database().await;
    let tenants = TenantRepository::new(database.clone());
    let auth = AuthRepository::new(database);
    let tenant = tenants
        .create("acme-concurrent-join", "Acme Concurrent Join")
        .await
        .unwrap();
    let admin = auth
        .create_user(
            tenant.id,
            "admin@example.test",
            "Admin",
            UserRole::TenantAdmin,
        )
        .await
        .unwrap();
    let link = auth
        .create_join_link_with_audit(
            tenant.id,
            UserRole::Viewer,
            None,
            60,
            1,
            AuditActor::user(admin.id),
        )
        .await
        .unwrap();

    assert_single_concurrent_accept(auth, tenant.id, link.join_link.id, link.plaintext_token).await;
}

#[tokio::test]
async fn accept_join_link_rejects_expired_revoked_used_up_and_email_mismatch() {
    let database = sqlite_database().await;
    let tenants = TenantRepository::new(database.clone());
    let auth = AuthRepository::new(database);
    let tenant = tenants
        .create("acme-join-rejects", "Acme Join Rejects")
        .await
        .unwrap();
    let admin = auth
        .create_user(
            tenant.id,
            "admin@example.test",
            "Admin",
            UserRole::TenantAdmin,
        )
        .await
        .unwrap();
    let actor = AuditActor::user(admin.id.clone());
    let expired = auth
        .create_join_link_with_audit(tenant.id, UserRole::Viewer, None, -1, 1, actor.clone())
        .await
        .unwrap();
    let revoked = auth
        .create_join_link_with_audit(tenant.id, UserRole::Viewer, None, 60, 1, actor.clone())
        .await
        .unwrap();
    auth.revoke_join_link_with_audit(tenant.id, &revoked.join_link.id, actor.clone())
        .await
        .unwrap();
    let constrained = auth
        .create_join_link_with_audit(
            tenant.id,
            UserRole::Viewer,
            Some("allowed@example.test".to_owned()),
            60,
            1,
            actor,
        )
        .await
        .unwrap();

    assert!(matches!(
        auth.accept_join_link(
            &expired.plaintext_token,
            profile("clerk", "expired", "expired@example.test", "Expired")
        )
        .await
        .unwrap_err(),
        RepositoryError::InvalidJoinLink
    ));
    assert!(matches!(
        auth.accept_join_link(
            &revoked.plaintext_token,
            profile("clerk", "revoked", "revoked@example.test", "Revoked")
        )
        .await
        .unwrap_err(),
        RepositoryError::InvalidJoinLink
    ));
    assert!(matches!(
        auth.accept_join_link(
            &constrained.plaintext_token,
            profile("clerk", "wrong-email", "wrong@example.test", "Wrong")
        )
        .await
        .unwrap_err(),
        RepositoryError::JoinLinkEmailMismatch
    ));

    auth.accept_join_link(
        &constrained.plaintext_token,
        profile("clerk", "allowed", "allowed@example.test", "Allowed"),
    )
    .await
    .unwrap();
    assert!(matches!(
        auth.accept_join_link(
            &constrained.plaintext_token,
            profile("clerk", "used-up", "used-up@example.test", "Used Up")
        )
        .await
        .unwrap_err(),
        RepositoryError::InvalidJoinLink
    ));
}

pub(super) fn profile(
    provider: &str,
    subject: &str,
    email: &str,
    display_name: &str,
) -> ExternalIdentityProfile {
    ExternalIdentityProfile {
        provider: provider.to_owned(),
        subject: subject.to_owned(),
        email: email.to_owned(),
        display_name: display_name.to_owned(),
    }
}

pub(crate) async fn assert_single_concurrent_accept(
    auth: AuthRepository,
    tenant_id: pandar_core::TenantId,
    join_link_id: String,
    plaintext_token: String,
) {
    let mut tasks = Vec::new();
    for index in 0..8 {
        let auth = auth.clone();
        let token = plaintext_token.clone();
        tasks.push(tokio::spawn(async move {
            auth.accept_join_link(
                &token,
                ExternalIdentityProfile {
                    provider: "betterauth".to_owned(),
                    subject: format!("concurrent-subject-{index}"),
                    email: format!("concurrent-{index}@example.test"),
                    display_name: format!("Concurrent {index}"),
                },
            )
            .await
        }));
    }

    let mut created = 0;
    let mut invalid = 0;
    for task in tasks {
        match task.await.unwrap() {
            Ok(accepted) => {
                assert!(accepted.created);
                created += 1;
            }
            Err(RepositoryError::InvalidJoinLink) => invalid += 1,
            Err(err) => panic!("unexpected concurrent accept error: {err:#}"),
        }
    }
    assert_eq!(created, 1);
    assert_eq!(invalid, 7);

    let links = auth.list_join_links_for_tenant(tenant_id).await.unwrap();
    let link = links
        .iter()
        .find(|link| link.id == join_link_id)
        .expect("concurrent join link should be listed");
    assert_eq!(link.used_count, 1);
    let mut memberships = 0;
    for index in 0..8 {
        memberships += auth
            .list_external_memberships("betterauth", &format!("concurrent-subject-{index}"))
            .await
            .unwrap()
            .len();
    }
    assert_eq!(memberships, 1);
}
