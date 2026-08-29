use super::*;

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
