use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};

use super::*;
use crate::{Database, repositories::ArtifactQuotaLimits};

#[tokio::test]
async fn concurrent_committed_artifact_reservations_admit_only_one_upload() {
    let (database, tenants, _, _, _, jobs) = repositories().await;
    let tenant = tenants
        .create("reservation-quota", "Reservation Quota")
        .await
        .unwrap();
    let quota = ArtifactQuotaLimits {
        tenant_bytes: 42,
        tenant_count: 1,
        global_bytes: 42,
        global_count: 1,
    };
    let first = jobs.reserve_artifact_quota(
        tenant.id,
        "reserved-artifact-1".to_owned(),
        "reservation-quota/reserved-artifact-1".to_owned(),
        42,
        quota,
    );
    let second = jobs.reserve_artifact_quota(
        tenant.id,
        "reserved-artifact-2".to_owned(),
        "reservation-quota/reserved-artifact-2".to_owned(),
        42,
        quota,
    );
    let (first, second) = tokio::join!(first, second);
    assert_eq!(
        [&first, &second]
            .into_iter()
            .filter(|result| result.is_ok())
            .count(),
        1
    );
    assert_eq!(
        [&first, &second]
            .into_iter()
            .filter(|result| matches!(result, Err(RepositoryError::ArtifactQuotaExceeded)))
            .count(),
        1
    );

    first.or(second).unwrap().release().await.unwrap();
    assert_eq!(
        crate::artifacts::lifecycle::queued_deletion_count(&database)
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn expired_artifact_reservation_is_released_before_next_admission() {
    exercise_expired_artifact_reservation(sqlite_database().await).await;
}

pub(in crate::repositories::tests) async fn exercise_expired_artifact_reservation(
    database: Database,
) {
    let tenants = TenantRepository::new(database.clone());
    let jobs = JobRepository::new(database.clone());
    let suffix = uuid::Uuid::new_v4();
    let tenant = tenants
        .create(
            &format!("expired-reservation-{suffix}"),
            "Expired Reservation",
        )
        .await
        .unwrap();
    let quota = ArtifactQuotaLimits {
        tenant_bytes: 42,
        tenant_count: 1,
        global_bytes: 42,
        global_count: 1,
    };
    let expired_artifact = format!("expired-artifact-{suffix}");
    jobs.reserve_artifact_quota(
        tenant.id,
        expired_artifact.clone(),
        format!("expired-reservation/{expired_artifact}"),
        42,
        quota,
    )
    .await
    .unwrap();
    match &database {
        Database::Sqlite(pool) => {
            sqlx::query(
                "UPDATE artifact_quota_reservations SET expires_at = '2000-01-01T00:00:00Z' WHERE artifact_id = ?1",
            )
            .bind(&expired_artifact)
            .execute(pool)
            .await
            .unwrap();
        }
        Database::Postgres(pool) => {
            sqlx::query(
                "UPDATE artifact_quota_reservations SET expires_at = '2000-01-01T00:00:00Z' WHERE artifact_id = $1",
            )
            .bind(&expired_artifact)
            .execute(pool)
            .await
            .unwrap();
        }
    }

    let next_artifact = format!("next-artifact-{suffix}");
    let next = jobs
        .reserve_artifact_quota(
            tenant.id,
            next_artifact.clone(),
            format!("expired-reservation/{next_artifact}"),
            42,
            quota,
        )
        .await
        .unwrap();
    assert_eq!(
        crate::artifacts::lifecycle::queued_deletion_count(&database)
            .await
            .unwrap(),
        1
    );
    next.release().await.unwrap();
}

#[tokio::test]
async fn committed_artifact_reservation_finalizes_with_job_and_audit() {
    exercise_committed_artifact_reservation_finalization(sqlite_database().await).await;
}

pub(in crate::repositories::tests) async fn exercise_committed_artifact_reservation_finalization(
    database: Database,
) {
    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let jobs = JobRepository::new(database.clone());
    let suffix = uuid::Uuid::new_v4();
    let tenant = tenants
        .create(
            &format!("reservation-finalize-{suffix}"),
            "Reservation Finalize",
        )
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let artifact_id = format!("finalized-artifact-{suffix}");
    let input = create_input(tenant.id, agent.id, &printer_id, &artifact_id);
    let reservation = jobs
        .reserve_artifact_quota(
            tenant.id,
            input.artifact.id.clone(),
            input.artifact.storage_path.clone(),
            input.artifact.size_bytes,
            crate::repositories::ArtifactQuotaLimits {
                tenant_bytes: 42,
                tenant_count: 1,
                global_bytes: 42,
                global_count: 1,
            },
        )
        .await
        .unwrap();

    let created = reservation
        .create_print_job_with_audit(input, crate::repositories::AuditActor::no_auth())
        .await
        .unwrap();

    assert_eq!(created.artifact.id, artifact_id);
    assert_eq!(
        crate::entities::artifact_quota_reservations::Entity::find()
            .filter(
                crate::entities::artifact_quota_reservations::Column::TenantId
                    .eq(tenant.id.to_string()),
            )
            .count(&database.sea_orm_connection())
            .await
            .unwrap(),
        0
    );
    let events = AuditEventRepository::new(database)
        .list_for_tenant(tenant.id)
        .await
        .unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|event| event.action == "job.create")
            .count(),
        1
    );
}
