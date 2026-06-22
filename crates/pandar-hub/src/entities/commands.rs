use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "commands")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub tenant_id: String,
    pub agent_id: String,
    pub printer_id: Option<String>,
    pub kind: String,
    pub status: String,
    pub payload_json: String,
    pub result_json: Option<String>,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
