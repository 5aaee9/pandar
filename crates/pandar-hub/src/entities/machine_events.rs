use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "machine_events")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub tenant_id: String,
    pub agent_id: String,
    pub printer_id: String,
    pub job_id: Option<String>,
    pub event_key: String,
    pub kind: String,
    pub severity: String,
    pub message: String,
    pub code: Option<String>,
    pub payload_json: String,
    pub observed_at: String,
    pub created_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
