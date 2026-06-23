use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "agents")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub status: String,
    pub version: Option<String>,
    pub last_seen_at: Option<String>,
    pub created_at: String,
    pub credential_hash: Option<String>,
    pub credential_rotated_at: Option<String>,
    pub credential_revoked_at: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
