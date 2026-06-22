use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "job_filament_usages")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub tenant_id: String,
    pub job_id: String,
    pub slot_index: i32,
    pub source: String,
    pub ams_id: Option<String>,
    pub tray_id: Option<String>,
    pub global_tray_id: Option<i32>,
    pub external_id: Option<String>,
    pub filament_id: Option<String>,
    pub setting_id: Option<String>,
    pub filament_type: Option<String>,
    pub color: Option<String>,
    pub used_mm: Option<String>,
    pub used_grams: Option<String>,
    pub confidence: String,
    pub created_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
