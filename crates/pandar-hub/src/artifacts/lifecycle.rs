use anyhow::Context;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    QueryOrder,
};

use crate::{
    db::{ConnectionDialectExt, Database, UniqueConstraint, is_unique_violation},
    entities::{artifact_deletions, artifact_quota_reservations},
};

use super::ArtifactStorage;

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
    let connection = database.sea_orm_connection();
    let queued = artifact_deletions::Entity::find()
        .order_by_asc(artifact_deletions::Column::CreatedAt)
        .all(&connection)
        .await
        .context("failed to load queued artifact deletions")?;
    let mut deleted = 0;
    for deletion in queued {
        if let Err(err) = storage.delete_artifact(&deletion.storage_path).await {
            let redacted = crate::redaction::redact_secrets(&format!("{err:#}"));
            if let Err(update_err) = update_deletion_failure(&connection, &deletion, redacted).await
            {
                tracing::error!(
                    error = %format!("{update_err:#}"),
                    "failed to persist artifact deletion retry"
                );
            }
            return Err(err).context("failed to delete queued artifact [redacted]");
        }
        artifact_deletions::Entity::delete_by_id(&deletion.id)
            .exec(&connection)
            .await
            .context("failed to finalize queued artifact deletion")?;
        deleted += 1;
    }
    Ok(deleted)
}

async fn update_deletion_failure<C>(
    connection: &C,
    deletion: &artifact_deletions::Model,
    error: String,
) -> anyhow::Result<()>
where
    C: ConnectionTrait,
{
    let mut active: artifact_deletions::ActiveModel = deletion.clone().into();
    active.attempts = Set(deletion.attempts.saturating_add(1));
    active.last_error = Set(Some(error));
    active.updated_at = Set(pandar_core::created_at_now());
    active
        .update(connection)
        .await
        .context("failed to update queued artifact deletion")?;
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
