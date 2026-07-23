use sea_orm::{DatabaseTransaction, SqliteTransactionMode, TransactionOptions, TransactionTrait};

use crate::db::Database;

pub(super) async fn begin(database: &Database) -> Result<DatabaseTransaction, sea_orm::DbErr> {
    let connection = database.sea_orm_connection();
    match connection.get_database_backend() {
        sea_orm::DatabaseBackend::Sqlite => {
            connection
                .begin_with_options(TransactionOptions {
                    sqlite_transaction_mode: Some(SqliteTransactionMode::Immediate),
                    ..Default::default()
                })
                .await
        }
        _ => connection.begin().await,
    }
}
