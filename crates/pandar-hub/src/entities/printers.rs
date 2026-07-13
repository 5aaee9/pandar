use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "printers")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub tenant_id: String,
    pub agent_id: String,
    pub serial_number: String,
    pub host: Option<String>,
    pub access_code: Option<String>,
    pub name: String,
    pub model: Option<String>,
    pub status: String,
    pub last_seen_at: Option<String>,
    pub created_at: String,
    pub nozzle_temperatures_json: String,
    pub active_nozzle: Option<String>,
    pub bed_temperature_celsius: Option<String>,
    pub bed_target_temperature_celsius: Option<String>,
    pub chamber_temperature_celsius: Option<String>,
    pub chamber_light_on: Option<bool>,
    pub bambu_fun_bits: Option<String>,
    pub bambu_fun_session_id: Option<String>,
    pub state_revision: i64,
    pub print_task_generation: i64,
    pub print_error_generation: i64,
    pub print_job_attr: Option<i64>,
    pub print_error_task_generation: Option<i64>,
    pub print_error_session_id: Option<String>,
    pub print_error_received_at: Option<String>,
    pub print_gcode_state: Option<String>,
    pub print_task_id: Option<String>,
    pub print_subtask_id: Option<String>,
    pub print_progress_percent: Option<i64>,
    pub print_remaining_time_minutes: Option<i64>,
    pub print_current_layer: Option<i64>,
    pub print_total_layers: Option<i64>,
    pub print_gcode_file: Option<String>,
    pub print_subtask_name: Option<String>,
    pub print_error: Option<i32>,
    pub print_job_id: Option<String>,
    pub hms_json: String,
    pub firmware_modules_json: Option<String>,
    pub firmware_upgrade_state_json: Option<String>,
    pub firmware_cfg: Option<String>,
    pub firmware_session_id: Option<String>,
    pub firmware_generation: Option<i64>,
    pub firmware_module_revision: i64,
    pub firmware_status_revision: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
