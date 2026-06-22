use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "jobs")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub tenant_id: String,
    pub printer_id: String,
    pub agent_id: String,
    pub artifact_id: String,
    pub command_id: String,
    pub status: String,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub print_status: String,
    pub printer_state: Option<String>,
    pub progress_percent: Option<i32>,
    pub remaining_time_minutes: Option<i32>,
    pub current_layer: Option<i32>,
    pub total_layers: Option<i32>,
    pub active_file: Option<String>,
    pub last_progress_percent: Option<i32>,
    pub last_layer: Option<i32>,
    pub print_error: Option<String>,
    pub print_started_at: Option<String>,
    pub print_finished_at: Option<String>,
    pub print_updated_at: Option<String>,
    pub ams_mapping_json: Option<String>,
    pub ams_mapping2_json: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
