use super::*;
use crate::repositories::{AuditActor, ExternalIdentityProfile, RecordAuditEvent, UserRole};

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

#[tokio::test]
async fn self_create_tenant_links_external_admin_and_redacts_audit_subject() {
    let database = sqlite_database().await;
    let auth = AuthRepository::new(database.clone());
    let audit = AuditEventRepository::new(database);
    let profile = profile("clerk", "raw-subject-secret", "alice@example.test", "Alice");

    let membership = auth
        .self_create_tenant_for_external_identity("alice-lab", "Alice Lab", profile)
        .await
        .unwrap();

    assert_eq!(membership.tenant.slug, "alice-lab");
    assert_eq!(membership.user.email, "alice@example.test");
    assert_eq!(membership.user.role, UserRole::TenantAdmin);
    let resolved = auth
        .authenticate_external_identity(membership.tenant.id, "clerk", "raw-subject-secret")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(resolved.user.id, membership.user.id);

    let events = audit.list_for_tenant(membership.tenant.id).await.unwrap();
    assert!(
        events
            .iter()
            .any(|event| event.action == "tenant.self_create")
    );
    assert!(
        events
            .iter()
            .any(|event| event.action == "user.external_projection_create")
    );
    let audit_json = events
        .iter()
        .map(|event| event.metadata_json.as_str())
        .collect::<String>();
    assert!(!audit_json.contains("raw-subject-secret"));
}

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

fn profile(
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
