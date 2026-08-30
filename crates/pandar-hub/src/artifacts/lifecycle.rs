use anyhow::Context;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, Condition, ConnectionTrait, EntityTrait,
    QueryFilter, QueryOrder, QuerySelect,
};
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    db::{ConnectionDialectExt, Database, UniqueConstraint, is_unique_violation},
    entities::{artifact_deletions, artifact_quota_reservations},
};

use super::ArtifactStorage;

const DELETION_BATCH_SIZE: u64 = 64;
const DELETION_LEASE_SECONDS: i64 = 300;

pub(crate) async fn enqueue_deletion<C>(connection: &C, storage_path: &str) -> anyhow::Result<()>
where
    C: ConnectionTrait,
{
    let now = pandar_core::created_at_now();
    let result = artifact_deletions::ActiveModel {
        id: Set(uuid::Uuid::new_v4().to_string()),
        storage_path: Set(storage_path.to_owned()),
        attempts: Set(0),
        last_error: Set(None),
        lease_owner: Set(None),
        lease_expires_at: Set(None),
        created_at: Set(now.clone()),
        updated_at: Set(now),
    }
    .insert(connection)
    .await;
    match result {
        Ok(_) => Ok(()),
        Err(err) if is_unique_violation(&err, UniqueConstraint::ArtifactDeletionStoragePath) => {
            Ok(())
        }
        Err(err) => Err(err).context("failed to enqueue artifact deletion"),
    }
}

pub async fn drain_deletions(
    database: &Database,
    storage: &dyn ArtifactStorage,
) -> anyhow::Result<u64> {
    drain_deletions_for_owner(database, storage, &uuid::Uuid::new_v4().to_string()).await
}

pub(crate) async fn drain_deletions_for_owner(
    database: &Database,
    storage: &dyn ArtifactStorage,
    owner: &str,
) -> anyhow::Result<u64> {
    let claimed = claim_deletions(database, owner).await?;
    let connection = database.sea_orm_connection();
    let mut deleted = 0;
    let mut first_error = None;

    for deletion in claimed {
        if let Err(err) = storage.delete_artifact(&deletion.storage_path).await {
            let redacted = crate::redaction::redact_secrets(&format!("{err:#}"));
            if let Err(update_err) =
                update_deletion_failure(database, &deletion, owner, redacted).await
            {
                tracing::error!(
                    error = %format!("{update_err:#}"),
                    "failed to persist artifact deletion retry"
                );
            }
            let err = err.context("failed to delete queued artifact [redacted]");
            if first_error.is_none() {
                first_error = Some(err);
            } else {
                tracing::warn!(
                    error = %crate::redaction::redact_secrets(&format!("{err:#}")),
                    "additional queued artifact deletion failed"
                );
            }
            continue;
        }

        let result = artifact_deletions::Entity::delete_many()
            .filter(artifact_deletions::Column::Id.eq(&deletion.id))
            .filter(artifact_deletions::Column::LeaseOwner.eq(owner))
            .exec(&connection)
            .await
            .context("failed to finalize queued artifact deletion");
        match result {
            Ok(result) => deleted += result.rows_affected,
            Err(err) if first_error.is_none() => first_error = Some(err),
            Err(err) => tracing::error!(
                error = %format!("{err:#}"),
                "additional queued artifact deletion finalization failed"
            ),
        }
    }

    match first_error {
        Some(err) => Err(err),
        None => Ok(deleted),
    }
}

async fn claim_deletions(
    database: &Database,
    owner: &str,
) -> anyhow::Result<Vec<artifact_deletions::Model>> {
    let tx = database
        .begin_write_transaction()
        .await
        .context("failed to begin artifact deletion claim transaction")?;
    let now = OffsetDateTime::now_utc();
    let now_text = now
        .format(&Rfc3339)
        .context("failed to format artifact deletion claim timestamp")?;
    let lease_expires_at = (now + Duration::seconds(DELETION_LEASE_SECONDS))
        .format(&Rfc3339)
        .context("failed to format artifact deletion lease expiry")?;
    let query = artifact_deletions::Entity::find()
        .filter(
            Condition::any()
                .add(artifact_deletions::Column::LeaseOwner.is_null())
                .add(artifact_deletions::Column::LeaseExpiresAt.lte(now_text.clone())),
        )
        .order_by_asc(artifact_deletions::Column::CreatedAt)
        .order_by_asc(artifact_deletions::Column::Id)
        .limit(DELETION_BATCH_SIZE);
    let claimed = tx
        .lock_for_update(query)
        .all(&tx)
        .await
        .context("failed to load queued artifact deletion batch")?;
    for deletion in &claimed {
        let mut active: artifact_deletions::ActiveModel = deletion.clone().into();
        active.lease_owner = Set(Some(owner.to_owned()));
        active.lease_expires_at = Set(Some(lease_expires_at.clone()));
        active.updated_at = Set(now_text.clone());
        active
            .update(&tx)
            .await
            .context("failed to claim queued artifact deletion")?;
    }
    tx.commit()
        .await
        .context("failed to commit artifact deletion claim transaction")?;
    Ok(claimed)
}

async fn update_deletion_failure(
    database: &Database,
    deletion: &artifact_deletions::Model,
    owner: &str,
    error: String,
) -> anyhow::Result<()> {
    let tx = database
        .begin_write_transaction()
        .await
        .context("failed to begin artifact deletion retry transaction")?;
    let query = artifact_deletions::Entity::find_by_id(&deletion.id)
        .filter(artifact_deletions::Column::LeaseOwner.eq(owner));
    if let Some(current) = tx
        .lock_for_update(query)
        .one(&tx)
        .await
        .context("failed to load claimed artifact deletion retry")?
    {
        let mut active: artifact_deletions::ActiveModel = current.into();
        active.attempts = Set(deletion.attempts.saturating_add(1));
        active.last_error = Set(Some(error));
        active.lease_owner = Set(None);
        active.lease_expires_at = Set(None);
        active.updated_at = Set(pandar_core::created_at_now());
        active
            .update(&tx)
            .await
            .context("failed to update queued artifact deletion")?;
    }
    tx.commit()
        .await
        .context("failed to commit artifact deletion retry transaction")?;
    Ok(())
}

pub async fn reap_expired_reservations(database: &Database) -> anyhow::Result<u64> {
    let tx = database
        .begin_write_transaction()
        .await
        .context("failed to begin expired artifact reservation transaction")?;
    lock_artifact_quota(&tx).await?;
    let expired = reap_expired_reservations_in_transaction(&tx).await?;
    tx.commit()
        .await
        .context("failed to commit expired artifact reservation transaction")?;
    Ok(expired.len() as u64)
}

pub(crate) async fn reap_expired_reservations_in_transaction<C>(
    connection: &C,
) -> anyhow::Result<Vec<artifact_quota_reservations::Model>>
where
    C: ConnectionTrait,
{
    let cutoff = pandar_core::created_at_now();
    let query = artifact_quota_reservations::Entity::find()
        .filter(artifact_quota_reservations::Column::ExpiresAt.lte(cutoff.clone()));
    let expired = connection
        .lock_for_update(query)
        .all(connection)
        .await
        .context("failed to load expired artifact reservations")?;
    for reservation in &expired {
        enqueue_deletion(connection, &reservation.storage_path).await?;
    }
    if !expired.is_empty() {
        artifact_quota_reservations::Entity::delete_many()
            .filter(artifact_quota_reservations::Column::ExpiresAt.lte(cutoff))
            .exec(connection)
            .await
            .context("failed to delete expired artifact reservations")?;
    }
    Ok(expired)
}

pub(crate) async fn lock_artifact_quota<C>(connection: &C) -> anyhow::Result<()>
where
    C: ConnectionTrait,
{
    if connection.get_database_backend() == sea_orm::DatabaseBackend::Postgres {
        connection
            .execute_unprepared("SELECT pg_advisory_xact_lock(1886596974)")
            .await
            .context("failed to lock PostgreSQL global artifact quota")?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) async fn queued_deletion_count(database: &Database) -> anyhow::Result<u64> {
    use sea_orm::PaginatorTrait;
    artifact_deletions::Entity::find()
        .count(&database.sea_orm_connection())
        .await
        .context("failed to count queued artifact deletions")
}
