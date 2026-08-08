use pandar_core::{
    BambuDeviceFeatures, BambuNozzleSystem, PrinterCoolingSystem, PrinterNozzleTemperature,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrinterSnapshotUpsert {
    pub serial_number: String,
    pub host: Option<String>,
    pub access_code: Option<String>,
    pub name: String,
    pub model: Option<String>,
    pub status: Option<String>,
    pub observed_at: String,
    pub nozzle_temperatures: Vec<PrinterNozzleTemperature>,
    pub active_nozzle: Option<String>,
    pub bed_temperature_celsius: Option<String>,
    pub bed_target_temperature_celsius: Option<String>,
    pub chamber_temperature_celsius: Option<String>,
    pub chamber_target_temperature_celsius: Option<String>,
    pub chamber_light_on: Option<bool>,
    pub cooling_system: Option<PrinterCoolingSystem>,
    pub nozzle_system: Option<BambuNozzleSystem>,
    pub connection_authoritative: bool,
    pub telemetry_authoritative: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SnapshotSessionState<'a> {
    pub(crate) device_features: Option<BambuDeviceFeatures>,
    pub(crate) device_features_session_id: Option<&'a str>,
    pub(crate) nozzle_system_session_id: Option<&'a str>,
    pub(crate) mqtt_presence_session_id: Option<&'a str>,
}

pub(super) const EMPTY_SNAPSHOT_SESSION_STATE: SnapshotSessionState<'static> =
    SnapshotSessionState {
        device_features: None,
        device_features_session_id: None,
        nozzle_system_session_id: None,
        mqtt_presence_session_id: None,
    };
