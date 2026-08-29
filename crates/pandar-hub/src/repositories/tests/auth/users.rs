use super::*;

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
async fn remove_user_with_audit_deletes_user_and_identities() {
    let database = sqlite_database().await;
    let tenants = TenantRepository::new(database.clone());
    let auth = AuthRepository::new(database.clone());
    let audit = AuditEventRepository::new(database);
    let tenant = tenants
        .create("acme-remove-user", "Acme Remove User")
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
    let member = auth
        .create_user(tenant.id, "member@example.test", "Member", UserRole::Viewer)
        .await
        .unwrap();
    auth.link_external_identity(tenant.id, &member.id, "clerk", "member-subject")
        .await
        .unwrap();

    let removed = auth
        .remove_user_with_audit(tenant.id, &member.id, AuditActor::user(admin.id.clone()))
        .await
        .unwrap();
    assert_eq!(removed.id, member.id);
    assert_eq!(
        auth.list_users_for_tenant(tenant.id).await.unwrap(),
        vec![admin.clone()]
    );
    assert!(
        auth.list_external_identities_for_tenant(tenant.id)
            .await
            .unwrap()
            .is_empty()
    );

    let events = audit.list_for_tenant(tenant.id).await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].action, "user.remove");
    assert_eq!(events[0].target_id.as_deref(), Some(member.id.as_str()));
    assert_eq!(events[0].user_id.as_deref(), Some(admin.id.as_str()));
}

#[tokio::test]
async fn last_tenant_admin_cannot_be_demoted() {
    let database = sqlite_database().await;
    let tenants = TenantRepository::new(database.clone());
    let auth = AuthRepository::new(database);
    let tenant = tenants
        .create("acme-demote-admin", "Acme Demote Admin")
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

    let err = auth
        .update_user_role_with_audit(
            tenant.id,
            &admin.id,
            UserRole::Viewer,
            AuditActor::user(admin.id.clone()),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, RepositoryError::LastTenantAdmin));
    assert_eq!(
        auth.list_users_for_tenant(tenant.id).await.unwrap()[0].role,
        UserRole::TenantAdmin
    );
}

#[tokio::test]
async fn demoting_an_admin_succeeds_while_another_admin_remains() {
    let database = sqlite_database().await;
    let tenants = TenantRepository::new(database.clone());
    let auth = AuthRepository::new(database);
    let tenant = tenants
        .create("acme-demote-second-admin", "Acme Demote Second Admin")
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
    let second_admin = auth
        .create_user(
            tenant.id,
            "second-admin@example.test",
            "Second Admin",
            UserRole::TenantAdmin,
        )
        .await
        .unwrap();

    let demoted = auth
        .update_user_role_with_audit(
            tenant.id,
            &second_admin.id,
            UserRole::Operator,
            AuditActor::user(admin.id.clone()),
        )
        .await
        .unwrap();
    assert_eq!(demoted.role, UserRole::Operator);
    assert_eq!(
        auth.list_users_for_tenant(tenant.id).await.unwrap()[1].role,
        UserRole::Operator
    );
}

#[tokio::test]
async fn last_tenant_admin_cannot_be_removed() {
    let database = sqlite_database().await;
    let tenants = TenantRepository::new(database.clone());
    let auth = AuthRepository::new(database);
    let tenant = tenants
        .create("acme-last-admin", "Acme Last Admin")
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
    let second_admin = auth
        .create_user(
            tenant.id,
            "second-admin@example.test",
            "Second Admin",
            UserRole::TenantAdmin,
        )
        .await
        .unwrap();

    auth.remove_user_with_audit(
        tenant.id,
        &admin.id,
        AuditActor::user(second_admin.id.clone()),
    )
    .await
    .unwrap();
    let err = auth
        .remove_user_with_audit(
            tenant.id,
            &second_admin.id,
            AuditActor::user(admin.id.clone()),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, RepositoryError::LastTenantAdmin));
    assert_eq!(
        auth.list_users_for_tenant(tenant.id).await.unwrap(),
        vec![second_admin.clone()]
    );
}

#[tokio::test]
async fn removing_missing_user_reports_missing_user() {
    let database = sqlite_database().await;
    let tenants = TenantRepository::new(database.clone());
    let auth = AuthRepository::new(database);
    let tenant = tenants
        .create("acme-remove-missing", "Acme Remove Missing")
        .await
        .unwrap();

    let err = auth
        .remove_user_with_audit(tenant.id, "missing-user", AuditActor::user("actor"))
        .await
        .unwrap_err();
    assert!(matches!(err, RepositoryError::MissingUser));
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
