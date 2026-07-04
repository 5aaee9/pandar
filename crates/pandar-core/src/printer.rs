use serde::{Deserialize, Serialize};

use crate::{AgentId, CoreError, TenantId, required};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Printer {
    pub id: String,
    pub tenant_id: TenantId,
    pub agent_id: AgentId,
    pub serial_number: String,
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
    pub chamber_light_on: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrinterParts {
    pub id: String,
    pub tenant_id: TenantId,
    pub agent_id: AgentId,
    pub serial_number: String,
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
    pub chamber_light_on: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrinterNozzleTemperature {
    pub label: Option<String>,
    pub current_celsius: Option<String>,
    pub target_celsius: Option<String>,
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
            chamber_light_on: parts.chamber_light_on,
        })
    }
}
