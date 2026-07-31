use sea_orm::DatabaseTransaction;

use crate::db::{Database, TransactionDialectExt};

pub(super) async fn begin(database: &Database) -> Result<DatabaseTransaction, sea_orm::DbErr> {
    let connection = database.sea_orm_connection();
    connection.begin_write_transaction().await
}
