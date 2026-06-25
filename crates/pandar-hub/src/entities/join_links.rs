use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "join_links")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub tenant_id: String,
    pub token_hash: String,
    pub role: String,
    pub email_constraint: Option<String>,
    pub expires_at: String,
    pub max_uses: i32,
    pub used_count: i32,
    pub created_by_user_id: Option<String>,
    pub revoked_at: Option<String>,
    pub created_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
