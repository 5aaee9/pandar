use serde::{Deserialize, Serialize};

use crate::{AgentId, BambuDeviceFeatures, CoreError, TenantId, required};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Printer {
    pub id: String,
    pub tenant_id: TenantId,
    pub agent_id: AgentId,
    pub serial_number: String,
    pub host: Option<String>,
    #[serde(skip_serializing)]
    pub access_code: Option<String>,
    pub name: String,
    pub model: Option<String>,
    pub status: String,
    pub last_seen_at: String,
    pub created_at: String,
    pub nozzle_temperatures: Vec<PrinterNozzleTemperature>,
    pub active_nozzle: Option<String>,
    pub bed_temperature_celsius: Option<String>,
    pub bed_target_temperature_celsius: Option<String>,
    pub chamber_temperature_celsius: Option<String>,
    pub chamber_target_temperature_celsius: Option<String>,
    pub chamber_light_on: Option<bool>,
    #[serde(skip_serializing, skip_deserializing)]
    pub bambu_device_features: Option<BambuDeviceFeatures>,
    #[serde(skip_serializing, skip_deserializing)]
    pub bambu_device_features_session_id: Option<String>,
    #[serde(skip_serializing, skip_deserializing)]
    pub mqtt_presence_session_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrinterParts {
    pub id: String,
    pub tenant_id: TenantId,
    pub agent_id: AgentId,
    pub serial_number: String,
    pub host: Option<String>,
    pub access_code: Option<String>,
    pub name: String,
    pub model: Option<String>,
    pub status: String,
    pub last_seen_at: String,
    pub created_at: String,
    pub nozzle_temperatures: Vec<PrinterNozzleTemperature>,
    pub active_nozzle: Option<String>,
    pub bed_temperature_celsius: Option<String>,
    pub bed_target_temperature_celsius: Option<String>,
    pub chamber_temperature_celsius: Option<String>,
    pub chamber_target_temperature_celsius: Option<String>,
    pub chamber_light_on: Option<bool>,
    pub bambu_device_features: Option<BambuDeviceFeatures>,
    pub bambu_device_features_session_id: Option<String>,
    pub mqtt_presence_session_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrinterNozzleTemperature {
    pub label: Option<String>,
    pub current_celsius: Option<String>,
    pub target_celsius: Option<String>,
    pub diameter_mm: Option<String>,
    pub nozzle_type: Option<String>,
}

impl Printer {
    pub fn from_parts(parts: PrinterParts) -> Result<Self, CoreError> {
        required(&parts.id, CoreError::EmptyPrinterId)?;
        required(&parts.serial_number, CoreError::EmptyPrinterSerialNumber)?;
        required(&parts.name, CoreError::EmptyPrinterName)?;
        required(&parts.status, CoreError::EmptyPrinterStatus)?;

        Ok(Self {
            id: parts.id,
            tenant_id: parts.tenant_id,
            agent_id: parts.agent_id,
            serial_number: parts.serial_number,
            host: parts.host.filter(|value| !value.trim().is_empty()),
            access_code: parts.access_code.filter(|value| !value.trim().is_empty()),
            name: parts.name,
            model: parts.model.filter(|model| !model.trim().is_empty()),
            status: parts.status,
            last_seen_at: parts.last_seen_at,
            created_at: parts.created_at,
            nozzle_temperatures: parts.nozzle_temperatures,
            active_nozzle: parts.active_nozzle.filter(|value| !value.trim().is_empty()),
            bed_temperature_celsius: parts.bed_temperature_celsius,
            bed_target_temperature_celsius: parts.bed_target_temperature_celsius,
            chamber_temperature_celsius: parts.chamber_temperature_celsius,
            chamber_target_temperature_celsius: parts.chamber_target_temperature_celsius,
            chamber_light_on: parts.chamber_light_on,
            bambu_device_features: parts.bambu_device_features,
            bambu_device_features_session_id: parts.bambu_device_features_session_id,
            mqtt_presence_session_id: parts.mqtt_presence_session_id,
        })
    }
}
