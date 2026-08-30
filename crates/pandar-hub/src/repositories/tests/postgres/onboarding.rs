use super::*;

#[tokio::test]
async fn postgres_external_onboarding_behavior_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };

    let auth = AuthRepository::new(database.clone());
    let audit = AuditEventRepository::new(database.clone());
    let admin = auth
        .self_create_tenant_for_external_identity(
            "pg-onboarding",
            "Postgres Onboarding",
            ExternalIdentityProfile {
                provider: "betterauth".to_owned(),
                subject: "admin-subject".to_owned(),
                email: "admin@example.test".to_owned(),
                display_name: "Admin".to_owned(),
            },
        )
        .await
        .unwrap();
    assert_eq!(admin.user.role, UserRole::TenantAdmin);

    let memberships = auth
        .list_external_memberships("betterauth", "admin-subject")
        .await
        .unwrap();
    assert_eq!(memberships.len(), 1);
    assert_eq!(memberships[0].tenant.id, admin.tenant.id);

    let link = auth
        .create_join_link_with_audit(
            admin.tenant.id,
            UserRole::Operator,
            Some("operator@example.test".to_owned()),
            60,
            1,
            AuditActor::user(admin.user.id.clone()),
        )
        .await
        .unwrap();
    let accepted = auth
        .accept_join_link(
            &link.plaintext_token,
            ExternalIdentityProfile {
                provider: "betterauth".to_owned(),
                subject: "operator-subject".to_owned(),
                email: "operator@example.test".to_owned(),
                display_name: "Operator".to_owned(),
            },
        )
        .await
        .unwrap();
    assert!(accepted.created);
    assert_eq!(accepted.user.role, UserRole::Operator);

    let existing_link = auth
        .create_join_link_with_audit(
            admin.tenant.id,
            UserRole::Viewer,
            Some("changed@example.test".to_owned()),
            60,
            1,
            AuditActor::user(admin.user.id.clone()),
        )
        .await
        .unwrap();
    let existing = auth
        .accept_join_link(
            &existing_link.plaintext_token,
            ExternalIdentityProfile {
                provider: "betterauth".to_owned(),
                subject: "operator-subject".to_owned(),
                email: "changed@example.test".to_owned(),
                display_name: "Operator Changed".to_owned(),
            },
        )
        .await
        .unwrap();
    assert!(!existing.created);
    assert_eq!(existing.user.id, accepted.user.id);
    assert_eq!(existing.user.role, UserRole::Operator);

    let listed = auth
        .list_join_links_for_tenant(admin.tenant.id)
        .await
        .unwrap();
    assert!(
        listed
            .iter()
            .any(|join_link| join_link.id == link.join_link.id && join_link.used_count == 1)
    );
    assert!(
        listed.iter().any(
            |join_link| join_link.id == existing_link.join_link.id && join_link.used_count == 0
        )
    );
    let revoked = auth
        .create_join_link_with_audit(
            admin.tenant.id,
            UserRole::Viewer,
            None,
            60,
            1,
            AuditActor::user(admin.user.id.clone()),
        )
        .await
        .unwrap();
    let revoked = auth
        .revoke_join_link_with_audit(
            admin.tenant.id,
            &revoked.join_link.id,
            AuditActor::user(admin.user.id.clone()),
        )
        .await
        .unwrap();
    assert!(revoked.revoked_at.is_some());

    let concurrent = auth
        .create_join_link_with_audit(
            admin.tenant.id,
            UserRole::Viewer,
            None,
            60,
            1,
            AuditActor::user(admin.user.id.clone()),
        )
        .await
        .unwrap();
    crate::repositories::tests::auth::assert_single_concurrent_accept(
        auth.clone(),
        admin.tenant.id,
        concurrent.join_link.id,
        concurrent.plaintext_token,
    )
    .await;

    let events = audit.list_for_tenant(admin.tenant.id).await.unwrap();
    assert!(
        events
            .iter()
            .any(|event| event.action == "join_link.accept")
    );
    let audit_json = events
        .iter()
        .map(|event| event.metadata_json.as_str())
        .collect::<String>();
    assert!(!audit_json.contains("admin-subject"));
    assert!(!audit_json.contains("operator-subject"));
    assert!(!audit_json.contains(&link.plaintext_token));
}
